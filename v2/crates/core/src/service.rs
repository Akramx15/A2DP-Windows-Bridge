//! The service: owns the Bluetooth stack (started on demand, restarted when
//! it stops working or after sleep), runs streaming sessions on their own
//! thread and reports what happens as [`ServiceEvent`]s.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bt_host::{BtHost, DiscoveredDevice, HciCapture, HostConfig, HostEvent, InitFailure, LinkStats, TransportConfig};
use hci_transport::UsbSelector;

use crate::messages::Msg;
use crate::settings::Settings;
use crate::stream::{self, StreamInfo, StreamRequest, StreamState};

/// Wait for the controller to answer when checking a stack before use
const CONTROLLER_PROBE: Duration = Duration::from_secs(2);
/// Time for a USB adapter to come back after resume before it is opened
const RESUME_SETTLE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq)]
pub enum ServiceEvent {
    State(StreamState, Msg),
    StreamInfo(StreamInfo),
    /// The headphones reported their volume (0..127)
    Volume(u8),
}

pub struct ServiceOptions {
    pub config_dir: PathBuf,
    /// H4 over TCP to a virtual controller instead of USB (tools/emu)
    pub hci_tcp: Option<String>,
    pub capture: HciCapture,
    /// AFH policy instead of the settings' (command line)
    pub afh: Option<String>,
}

pub(crate) struct Inner {
    opts: ServiceOptions,
    pub settings: Mutex<Settings>,
    events: Mutex<Sender<ServiceEvent>>,
    host: Mutex<Option<Arc<BtHost>>>,
    stats: Arc<LinkStats>,
    stop: AtomicBool,
    running: AtomicBool,
    /// The PC resumed from sleep since the stack started: the adapter lost
    /// power or was re-enumerated, so the stack is restarted before the
    /// next use (v1 needed an app restart)
    pub btstack_stale: AtomicBool,
    resumed_at: Mutex<Option<Instant>>,
    tick: (Mutex<()>, Condvar),
    state: Mutex<StreamState>,
}

impl Inner {
    pub fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    pub fn notify(&self, state: StreamState, msg: Msg) {
        tracing::info!("state {state:?}: {}", msg.english());
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = state;
        let _ = self.events.lock().unwrap_or_else(|e| e.into_inner()).send(ServiceEvent::State(state, msg));
    }

    pub fn stream_info(&self, info: StreamInfo) {
        let _ = self.events.lock().unwrap_or_else(|e| e.into_inner()).send(ServiceEvent::StreamInfo(info));
    }

    /// Sleeps up to `d`, returning early on a stop
    pub fn wait_tick(&self, d: Duration) {
        let guard = self.tick.0.lock().unwrap_or_else(|e| e.into_inner());
        if !self.stop_requested() {
            let _ = self.tick.1.wait_timeout(guard, d);
        }
    }

    fn wait_after_resume(&self) {
        let Some(at) = *self.resumed_at.lock().unwrap_or_else(|e| e.into_inner()) else { return };
        let since = at.elapsed();
        if since >= RESUME_SETTLE {
            return;
        }
        tracing::info!("waiting {} ms for the adapter after resume", (RESUME_SETTLE - since).as_millis());
        while at.elapsed() < RESUME_SETTLE && !self.stop_requested() {
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn host_config(&self) -> HostConfig {
        let s = self.settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let transport = match &self.opts.hci_tcp {
            Some(addr) => TransportConfig::Tcp(addr.clone()),
            None => TransportConfig::Usb(UsbSelector::default()),
        };
        let mut c = HostConfig::new(transport);
        c.link_key_dir = Some(self.opts.config_dir.clone());
        c.firmware_dir = Some(self.opts.config_dir.clone());
        c.capture = self.opts.capture.clone();
        crate::adapter::configure(&mut c, &s, &self.opts.config_dir);
        c
    }

    fn start_host(self: &Arc<Self>) -> Result<Arc<BtHost>, Msg> {
        // A controller that did not answer at all is opened once more right
        // away: opening it resets its USB pipes, which can be enough
        let attempts = 2;
        let mut last = InitFailure::Failed;
        for attempt in 1..=attempts {
            if attempt > 1 {
                tracing::warn!("the Bluetooth adapter did not answer, opening it again ({attempt}/{attempts})");
            }
            match BtHost::start(self.host_config(), self.stats.clone()) {
                Ok(h) => {
                    let h = Arc::new(h);
                    let s = self.settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    h.set_media_keys(s.media_keys);
                    h.set_afh_policy(self.opts.afh.as_deref().unwrap_or(&s.afh));
                    self.pump_events(&h);
                    return Ok(h);
                }
                Err(f) => {
                    last = f;
                    if f != InitFailure::NoAnswer {
                        break;
                    }
                }
            }
        }
        Err(Msg::new(if last == InitFailure::NoAnswer { "error.btstack_no_answer" } else { "error.btstack_init" }))
    }

    /// Headphone buttons to Windows media keys, volume reports to the
    /// service's events
    fn pump_events(self: &Arc<Self>, host: &Arc<BtHost>) {
        let Some(rx) = host.take_events() else { return };
        let weak: Weak<Inner> = Arc::downgrade(self);
        let _ = std::thread::Builder::new().name("host-events".into()).spawn(move || {
            while let Ok(e) = rx.recv() {
                match e {
                    HostEvent::MediaKey(k) => {
                        let key = match k {
                            bt_host::MediaKey::PlayPause => win_sys::media_keys::MediaKey::PlayPause,
                            bt_host::MediaKey::Stop => win_sys::media_keys::MediaKey::Stop,
                            bt_host::MediaKey::Next => win_sys::media_keys::MediaKey::Next,
                            bt_host::MediaKey::Previous => win_sys::media_keys::MediaKey::Previous,
                        };
                        if let Err(e) = win_sys::media_keys::send(key) {
                            tracing::warn!("media key {k:?}: {e}");
                        }
                    }
                    HostEvent::Volume(v) => {
                        if let Some(inner) = weak.upgrade() {
                            let _ =
                                inner.events.lock().unwrap_or_else(|e| e.into_inner()).send(ServiceEvent::Volume(v));
                        }
                    }
                    HostEvent::ConnectionLost | HostEvent::TransportFailed => {}
                }
            }
        });
    }

    /// The running stack, started (or restarted) as needed
    pub fn ensure_host(self: &Arc<Self>) -> Result<Arc<BtHost>, Msg> {
        let mut slot = self.host.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = slot.as_ref() {
            // A stack that stopped working stays so until it is started anew
            // (v1 issues #42, #43: only an app restart helped)
            let why = if self.btstack_stale.load(Ordering::Acquire) {
                Some("the PC resumed from sleep")
            } else if !h.controller_responds(CONTROLLER_PROBE) {
                Some("the Bluetooth adapter stopped answering")
            } else {
                None
            };
            match why {
                None => return Ok(h.clone()),
                Some(why) => {
                    tracing::warn!("restarting BTstack: {why}");
                    h.shutdown();
                    *slot = None;
                }
            }
        }
        self.btstack_stale.store(false, Ordering::Release);
        self.wait_after_resume();
        let h = self.start_host()?;
        *slot = Some(h.clone());
        Ok(h)
    }

    /// Shuts the stack down and starts a new one (the session thread, for a
    /// stack that stopped working or is left from before sleep)
    pub fn restart_host(self: &Arc<Self>, why: &str, max_media_payload: u16) -> Result<Arc<BtHost>, Msg> {
        tracing::warn!("restarting BTstack: {why}");
        self.shutdown_host();
        let h = self.ensure_host()?;
        h.set_media_payload_limit(max_media_payload);
        Ok(h)
    }

    fn shutdown_host(&self) {
        let old = self.host.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(h) = old {
            // Also when the session still holds the Arc: the adapter must be
            // free for the next stack
            h.shutdown();
        }
    }

    pub fn saved_volume(&self, addr: &str) -> Option<u8> {
        let s = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        if !s.remember_volume {
            return None;
        }
        s.saved_volumes.get(&addr.to_uppercase()).and_then(|&v| u8::try_from(v).ok())
    }

    /// Keeps the headphones' volume for the next connection (saved with the
    /// settings when it changes)
    pub fn remember_volume(&self, addr: &str, volume: Option<u8>) {
        let Some(v) = volume else { return };
        let mut s = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        if !s.remember_volume {
            return;
        }
        let key = addr.to_uppercase();
        if s.saved_volumes.get(&key) == Some(&(v as i32)) {
            return;
        }
        s.saved_volumes.insert(key, v as i32);
        if let Err(e) = s.save(&self.opts.config_dir) {
            tracing::warn!("cannot save the settings: {e}");
        }
    }

    pub fn save_device(&self, addr: &str, name: &str) {
        if let Err(e) = crate::devices::save(&self.opts.config_dir, addr, name) {
            tracing::warn!("cannot save the device: {e}");
        }
    }
}

pub struct Service {
    inner: Arc<Inner>,
    worker: Mutex<Option<JoinHandle<()>>>,
    _power: Option<win_sys::power::PowerNotifier>,
}

impl Service {
    pub fn new(opts: ServiceOptions, events: Sender<ServiceEvent>) -> Service {
        let settings = Settings::load(&opts.config_dir);
        let inner = Arc::new(Inner {
            opts,
            settings: Mutex::new(settings),
            events: Mutex::new(events),
            host: Mutex::new(None),
            stats: Arc::new(LinkStats::default()),
            stop: AtomicBool::new(false),
            running: AtomicBool::new(false),
            btstack_stale: AtomicBool::new(false),
            resumed_at: Mutex::new(None),
            tick: (Mutex::new(()), Condvar::new()),
            state: Mutex::new(StreamState::Idle),
        });
        // Without a window, also while only the tray runs
        let weak = Arc::downgrade(&inner);
        let power = win_sys::power::PowerNotifier::new(move |e| {
            let Some(inner) = weak.upgrade() else { return };
            match e {
                win_sys::power::PowerEvent::Suspend => tracing::info!("the PC is going to sleep"),
                win_sys::power::PowerEvent::Resume => {
                    // Both resume kinds may come for one resume
                    if !inner.btstack_stale.swap(true, Ordering::AcqRel) {
                        tracing::info!("the PC resumed from sleep, Bluetooth will be restarted");
                        *inner.resumed_at.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
                    }
                }
            }
        });
        let power = match power {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!("no sleep / resume notifications: {e}");
                None
            }
        };
        pipeline::set_thread_priority_hook(|| {
            Box::new(win_sys::mmcss::MmcssGuard::register(win_sys::mmcss::MmcssTask::ProAudio).ok())
        });
        Service { inner, worker: Mutex::new(None), _power: power }
    }

    pub fn stats(&self) -> Arc<LinkStats> {
        self.inner.stats.clone()
    }

    pub fn config_dir(&self) -> &std::path::Path {
        &self.inner.opts.config_dir
    }

    pub fn capture(&self) -> &HciCapture {
        &self.inner.opts.capture
    }

    /// Replaces the settings (saved by the caller) and applies what affects
    /// a running stack
    pub fn set_settings(&self, settings: Settings) {
        if let Some(h) = self.host() {
            h.set_media_keys(settings.media_keys);
            h.set_afh_policy(self.inner.opts.afh.as_deref().unwrap_or(&settings.afh));
        }
        *self.inner.settings.lock().unwrap_or_else(|e| e.into_inner()) = settings;
    }

    /// Stops the stack, releasing the adapter (e.g. before its driver is
    /// changed); the next use starts a new one
    pub fn release_adapter(&self) {
        self.shutdown();
    }

    pub fn settings(&self) -> Settings {
        self.inner.settings.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn state(&self) -> StreamState {
        *self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::Acquire)
    }

    /// The running stack, if any (for diagnostics)
    pub fn host(&self) -> Option<Arc<BtHost>> {
        self.inner.host.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Starts a session; a running one is stopped first
    pub fn start_streaming(&self, req: StreamRequest) {
        self.stop_streaming();
        self.inner.stop.store(false, Ordering::Release);
        self.inner.running.store(true, Ordering::Release);
        self.inner.notify(StreamState::Connecting, Msg::new("status.initializing"));
        let inner = self.inner.clone();
        let t = std::thread::Builder::new()
            .name("session".into())
            .spawn(move || {
                // A panic in the session must not take the process down: the
                // state says what happened
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| stream::run(&inner, req)));
                if r.is_err() {
                    tracing::error!("the streaming session panicked");
                    inner.notify(StreamState::Error, Msg::new("error.btstack_crash_stream"));
                }
                inner.running.store(false, Ordering::Release);
            })
            .expect("spawning the session thread");
        *self.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(t);
    }

    /// Stops the session and waits for it (bounded by the stack's own
    /// timeouts: blocking waits are cancelled)
    pub fn stop_streaming(&self) {
        let Some(t) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
        self.inner.stop.store(true, Ordering::Release);
        self.inner.tick.1.notify_all();
        if let Some(h) = self.host() {
            h.cancel_pending_waits();
        }
        let _ = t.join();
    }

    /// Inquiry for nearby devices
    pub fn scan(&self, seconds: u32) -> Result<Vec<DiscoveredDevice>, Msg> {
        if self.is_running() {
            return Err(Msg::new("status.streaming"));
        }
        self.inner.notify(StreamState::Idle, Msg::new("status.initializing_scan"));
        let host = self.inner.ensure_host()?;
        self.inner.notify(StreamState::Idle, Msg::new("status.scanning_devices"));
        let found = host.scan(seconds);
        self.inner.notify(StreamState::Idle, Msg::with("status.scan_complete", &[&found.len()]));
        Ok(found)
    }

    /// Stops streaming and the stack (the adapter is released)
    pub fn shutdown(&self) {
        self.stop_streaming();
        self.inner.shutdown_host();
    }

    pub fn saved_volumes(&self) -> BTreeMap<String, i32> {
        self.settings().saved_volumes
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown();
    }
}
