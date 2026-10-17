//! The resident core's state as the UI sees it (an ipc::Snapshot with a
//! revision), the events that change it, and the requests that act on it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use a2dpwb_core::diag::AudioRates;
use a2dpwb_core::profiles::{self, Profile as CoreProfile};
use a2dpwb_core::receiver::{self, ReceiverClient};
use a2dpwb_core::settings::Settings as CoreSettings;
use a2dpwb_core::{Service, ServiceEvent, SourceRequest, StreamRequest, StreamState as CoreStreamState, devices};
use bt_host::LinkStats;
use bt_host::chips::{self, ChipVendor};
use ipc::{
    AdapterInfo, AdapterState, AdapterStatus, AudioApp, AudioDevice, CaptureState, Device, DiagInfo, DriverKind,
    ErrorInfo, Event, FirmwareFile, LinkTotals, Method, Msg, Profile, ReceiverConnection, ReceiverState, Settings,
    Snapshot, SourceInfo, StreamState, StreamStatus, UpdateState,
};
use serde_json::Value;
use tokio::sync::broadcast;

/// Work of a request that blocks (scan, driver change), run off the runtime
type BlockingJob = Box<dyn FnOnce(&Arc<CoreState>) -> Result<Value, ErrorInfo> + Send>;

/// A tray balloon to show (on the tray's thread)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalloonRequest {
    pub title: String,
    pub text: String,
    /// Opened by a click on it
    pub url: Option<String>,
}

/// settings.json key (kept among the unknown ones): the "still running in
/// the tray" hint was shown once
const TRAY_HINT_SHOWN: &str = "tray_hint_shown";

/// What the tray shows
#[derive(Debug, Clone)]
pub struct TrayView {
    pub streaming: bool,
    pub tooltip: String,
    pub profiles: Vec<String>,
    pub active_profile: Option<String>,
    pub language: String,
}

pub struct CoreState {
    snapshot: Mutex<(u64, Snapshot)>,
    events: broadcast::Sender<(u64, Arc<Event>)>,
    pub service: Arc<Service>,
    /// Clients subscribed to link samples
    pub link_subscribers: AtomicUsize,
    /// Clients subscribed to the receiver test's state
    pub receiver_subscribers: AtomicUsize,
    /// The peer receiver test's connection: kept while no window is open
    pub receiver: ReceiverClient,
    pub started: Instant,
    /// What the stream moved over the last second (the sampler), for the
    /// flow's "Audio sending" step
    rates: Mutex<Option<AudioRates>>,
    tray: Box<dyn Fn(TrayView) + Send + Sync>,
    quit: Box<dyn Fn() + Send + Sync>,
    balloon: Box<dyn Fn(BalloonRequest) + Send + Sync>,
}

pub fn msg(m: &a2dpwb_core::Msg) -> Msg {
    Msg { key: m.key.to_string(), args: m.args.clone() }
}

fn err(code: &str, key: &str) -> ErrorInfo {
    ErrorInfo { code: code.into(), message: Msg::new(key) }
}

/// A request whose parameters do not make sense (ipc.error.bad_request)
fn bad_request(code: &str, method: &str, why: String) -> ErrorInfo {
    ErrorInfo {
        code: code.into(),
        message: Msg { key: "ipc.error.bad_request".into(), args: vec![method.into(), why] },
    }
}

/// A result as JSON (plain data: it cannot fail)
fn json<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn to_ipc_profile(p: &CoreProfile, legacy: u16) -> Profile {
    Profile {
        name: p.name.clone(),
        device_address: p.device_address.clone(),
        device_name: p.device_name.clone(),
        codec: p.codec.clone(),
        quality: p.quality.clone(),
        abr: p.abr,
        codec_fallback: p.codec_fallback,
        sample_rate: p.sample_rate,
        bit_depth: p.bit_depth,
        capture_mode: p.capture_mode.clone(),
        audio_device_id: p.audio_device_id.clone(),
        audio_device_name: p.audio_device_name.clone(),
        auto_switch_device: p.auto_switch_device,
        app_exe: p.app_exe.clone(),
        app_name: p.app_name.clone(),
        volume_sync: p.volume_sync.clone(),
        volume_device_id: p.volume_device_id.clone(),
        volume_device_name: p.volume_device_name.clone(),
        max_media_payload: p.max_media_payload.unwrap_or(legacy),
    }
}

/// Applies the UI's profile onto the stored one (unknown keys kept)
fn merge_profile(base: Option<&CoreProfile>, p: &Profile) -> CoreProfile {
    let mut c = base.cloned().unwrap_or_default();
    c.name = p.name.clone();
    c.device_address = p.device_address.to_uppercase();
    c.device_name = p.device_name.clone();
    c.codec = p.codec.clone();
    c.quality = p.quality.clone();
    c.abr = p.abr;
    c.codec_fallback = p.codec_fallback;
    c.sample_rate = p.sample_rate;
    c.bit_depth = p.bit_depth;
    c.capture_mode = p.capture_mode.clone();
    c.audio_device_id = p.audio_device_id.clone();
    c.audio_device_name = p.audio_device_name.clone();
    c.auto_switch_device = p.auto_switch_device;
    c.app_exe = p.app_exe.clone();
    c.app_name = p.app_name.clone();
    c.volume_sync = p.volume_sync.clone();
    c.volume_device_id = p.volume_device_id.clone();
    c.volume_device_name = p.volume_device_name.clone();
    c.max_media_payload = Some(bt_host::media::clamp_media_payload_limit(p.max_media_payload as u32));
    c
}

fn to_ipc_settings(s: &CoreSettings) -> Settings {
    Settings {
        language: s.language.clone(),
        theme: s.theme.clone(),
        start_with_windows: s.start_with_windows,
        start_minimized: s.start_minimized,
        minimize_to_tray: s.minimize_to_tray,
        check_updates_on_startup: s.check_updates_on_startup,
        include_prereleases: s.include_prereleases,
        media_keys: s.media_keys,
        remember_volume: s.remember_volume,
        developer_mode: s.debug_mode,
        bt_chip_pid: s.bt_chip_pid,
        bt_chip_fw_stem: s.bt_chip_fw_stem.clone(),
        afh: s.afh.clone(),
        remote_sink: s.remote_sink.clone(),
    }
}

fn merge_settings(base: &CoreSettings, s: &Settings) -> CoreSettings {
    let mut c = base.clone();
    c.language = s.language.clone();
    c.theme = s.theme.clone();
    c.start_with_windows = s.start_with_windows;
    c.start_minimized = s.start_minimized;
    c.minimize_to_tray = s.minimize_to_tray;
    c.check_updates_on_startup = s.check_updates_on_startup;
    c.include_prereleases = s.include_prereleases;
    c.media_keys = s.media_keys;
    c.remember_volume = s.remember_volume;
    c.debug_mode = s.developer_mode;
    c.bt_chip_pid = s.bt_chip_pid;
    c.bt_chip_fw_stem = s.bt_chip_fw_stem.clone();
    c.afh = s.afh.clone();
    c.remote_sink = s.remote_sink.clone();
    c
}

/// What the UI tells about an adapter's firmware besides Realtek's files
fn vendor_note(vendor: ChipVendor) -> Option<Msg> {
    let key = match vendor {
        ChipVendor::Intel => "firmware.vendor_intel",
        ChipVendor::Broadcom => "firmware.vendor_broadcom",
        ChipVendor::Csr => "firmware.vendor_csr",
        ChipVendor::MediaTek | ChipVendor::Qualcomm => "firmware.vendor_unsupported",
        ChipVendor::Realtek | ChipVendor::Unknown => return None,
    };
    Some(Msg::new(key))
}

fn adapter_infos(config_dir: &Path, settings: &CoreSettings) -> Vec<AdapterInfo> {
    a2dpwb_core::adapter::adapters(false)
        .into_iter()
        .map(|a| {
            let driver = match a.device.driver_kind {
                win_sys::driver::DriverKind::WinUsb => DriverKind::Winusb,
                win_sys::driver::DriverKind::Bluetooth => DriverKind::Bluetooth,
                win_sys::driver::DriverKind::Other => DriverKind::Other,
                win_sys::driver::DriverKind::None => DriverKind::None,
            };
            let chip_pid = chips::resolve_chip_pid(settings.bt_chip_pid, &settings.bt_chip_fw_stem, [a.realtek_pid]);
            let files = chips::firmware_files(chip_pid, &settings.bt_chip_fw_stem);
            AdapterInfo {
                instance_id: a.device.instance_id.clone(),
                vid: a.device.vid,
                pid: a.device.pid,
                display_name: a.display_name.clone(),
                device_name: a.device.name.clone(),
                vendor: a.vendor.name().to_string(),
                driver,
                driver_service: a.device.driver.service.clone(),
                realtek_chip: a.realtek_pid.and_then(chips::realtek_chip_name).map(str::to_owned),
                firmware_present: chips::firmware_present(config_dir, chip_pid, &settings.bt_chip_fw_stem),
                firmware_files: files.map(|(f, c)| vec![f, c]).unwrap_or_default(),
                vendor_note: vendor_note(a.vendor),
            }
        })
        .collect()
}

/// The saved devices, then the scanned ones; with the codecs each offered
fn saved_devices(config_dir: &Path, scanned: &[Device], codecs: &BTreeMap<String, Vec<String>>) -> Vec<Device> {
    let mut list: Vec<Device> = devices::load(config_dir)
        .into_iter()
        .map(|d| Device { address: d.address, name: d.name, saved: true, audio: true, rssi: None, codecs: Vec::new() })
        .collect();
    for s in scanned {
        match list.iter_mut().find(|d| d.address.eq_ignore_ascii_case(&s.address)) {
            Some(d) => {
                d.rssi = s.rssi;
                if d.name.is_empty() {
                    d.name = s.name.clone();
                }
            }
            None => list.push(s.clone()),
        }
    }
    for d in &mut list {
        if let Some(c) = codecs.get(&d.address.to_uppercase()) {
            d.codecs = c.clone();
        }
    }
    list
}

/// The cumulative media counters, those when the current stream started,
/// and the number of streams started
pub fn link_totals(s: &LinkStats) -> (LinkTotals, LinkTotals, u32) {
    let r = Ordering::Relaxed;
    // stream_starts is written after the start_* values
    let starts = s.stream_starts.load(Ordering::Acquire);
    let start = LinkTotals {
        packets: s.start_packets.load(r),
        bytes: s.start_bytes.load(r),
        dropped: s.start_dropped.load(r),
        capture_frames: s.start_capture_frames.load(r),
        capture_dropped: s.start_capture_dropped.load(r),
    };
    let total = LinkTotals {
        packets: s.packets_sent.load(r),
        bytes: s.bytes_sent.load(r),
        dropped: s.queue_drops.load(r) + s.send_errors.load(r),
        capture_frames: s.capture_frames.load(r),
        capture_dropped: s.capture_dropped_frames.load(r),
    };
    (total, start, starts)
}

/// The core's receiver types to the protocol's (the same fields)
fn convert<T: serde::Serialize, U: serde::de::DeserializeOwned>(v: &T) -> Option<U> {
    serde_json::to_value(v).ok().and_then(|j| serde_json::from_value(j).ok())
}

fn to_ipc_source(s: &a2dpwb_core::SourceInfo) -> SourceInfo {
    SourceInfo { kind: s.kind.to_string(), name: s.name.clone(), waiting: s.waiting }
}

/// The parameters of stream.start_custom
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CustomStream {
    pub device: String,
    pub device_name: Option<String>,
    pub codec: String,
    pub quality: String,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
    pub source: String,
    pub source_id: Option<String>,
    pub source_name: Option<String>,
    pub max_media_payload: Option<u16>,
    pub abr: Option<bool>,
    pub codec_fallback: Option<bool>,
}

/// What to stream for stream.start_custom: built as a profile is (same
/// checks and defaults), with the test tone as a source of its own
pub fn custom_request(c: &CustomStream, legacy_max_media_payload: u16) -> Result<StreamRequest, ErrorInfo> {
    const METHOD: &str = "stream.start_custom";
    let codec = c.codec.trim().to_ascii_lowercase();
    let codec = if codec.is_empty() { "auto".to_string() } else { codec };
    if codec != "auto" && codec::CodecKind::from_key(&codec).is_none() {
        return Err(bad_request("stream.bad_codec", METHOD, format!("unknown codec \"{codec}\"")));
    }
    let source = c.source.trim().to_ascii_lowercase();
    let (mode, tone) = match source.as_str() {
        "tone" => ("loopback", true),
        "" | "loopback" => ("loopback", false),
        "virtual" => ("virtual", false),
        "app" => ("app", false),
        other => return Err(bad_request("stream.bad_source", METHOD, format!("unknown source \"{other}\""))),
    };
    let id = c.source_id.clone().unwrap_or_default();
    let name = c.source_name.clone().unwrap_or_default();
    let p = CoreProfile {
        name: String::new(),
        device_address: c.device.trim().to_uppercase(),
        device_name: c.device_name.clone().unwrap_or_default(),
        codec,
        quality: if c.quality.trim().is_empty() { "hq".into() } else { c.quality.trim().to_ascii_lowercase() },
        abr: c.abr.unwrap_or(false),
        codec_fallback: c.codec_fallback.unwrap_or(false),
        sample_rate: c.sample_rate.unwrap_or(0),
        bit_depth: c.bit_depth.unwrap_or(0),
        capture_mode: mode.into(),
        audio_device_id: if mode == "virtual" { id.clone() } else { String::new() },
        audio_device_name: if mode == "virtual" { name.clone() } else { String::new() },
        // A test leaves Windows' default device alone
        auto_switch_device: false,
        app_exe: if mode == "app" { id } else { String::new() },
        app_name: if mode == "app" { name } else { String::new() },
        volume_sync: "off".into(),
        max_media_payload: c.max_media_payload.map(|v| bt_host::media::clamp_media_payload_limit(v as u32)),
        ..CoreProfile::default()
    };
    let mut req = p
        .to_request(legacy_max_media_payload)
        .map_err(|_| err("profile.invalid_address", "error.invalid_address_format"))?;
    if tone {
        req.source = SourceRequest::TestTone;
    }
    Ok(req)
}

/// HKCU Run value of v2: the resident core, which opens the UI unless
/// started with --background (start_minimized: only the tray at logon)
fn apply_autostart(enabled: bool, minimized: bool) -> Result<(), String> {
    use win_sys::autostart;
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        autostart::set(autostart::VALUE_NAME, &autostart::command_line(&exe, autostart_args(minimized)))
            .map_err(|e| e.to_string())
    } else {
        autostart::remove(autostart::VALUE_NAME).map(|_| ()).map_err(|e| e.to_string())
    }
}

fn autostart_args(minimized: bool) -> &'static str {
    if minimized { "--background" } else { "" }
}

/// The Run value needs writing for this settings change
fn autostart_changed(before: &CoreSettings, after: &CoreSettings) -> bool {
    before.start_with_windows != after.start_with_windows
        || (after.start_with_windows && before.start_minimized != after.start_minimized)
}

/// The chip or firmware setting changed: the stack must load firmware anew
fn chip_changed(before: &CoreSettings, after: &CoreSettings) -> bool {
    before.bt_chip_pid != after.bt_chip_pid || before.bt_chip_fw_stem != after.bt_chip_fw_stem
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl CoreState {
    pub fn new(
        service: Arc<Service>,
        tray: Box<dyn Fn(TrayView) + Send + Sync>,
        quit: Box<dyn Fn() + Send + Sync>,
        balloon: Box<dyn Fn(BalloonRequest) + Send + Sync>,
    ) -> Arc<CoreState> {
        let dir = service.config_dir().to_path_buf();
        let settings = service.settings();
        let legacy = settings.legacy_max_media_payload();
        let profiles: Vec<Profile> = profiles::load(&dir).iter().map(|p| to_ipc_profile(p, legacy)).collect();
        let active = Some(settings.last_profile.clone()).filter(|n| profiles.iter().any(|p| &p.name == n));
        let snapshot = Snapshot {
            adapter: AdapterState {
                status: AdapterStatus::Stopped,
                adapters: adapter_infos(&dir, &settings),
                error: None,
            },
            devices: saved_devices(&dir, &[], &service.device_codecs()),
            scanning: false,
            stream: StreamStatus { message: Msg::new("status.ready"), ..Default::default() },
            profiles,
            active_profile: active,
            settings: to_ipc_settings(&settings),
            update: UpdateState { current_version: env!("CARGO_PKG_VERSION").into(), ..Default::default() },
            config_dir: dir.display().to_string(),
            // Where main.rs has the log files written
            log_dir: dir.join("logs").display().to_string(),
            capture: None,
        };
        let (events, _) = broadcast::channel(256);
        let state = Arc::new(CoreState {
            snapshot: Mutex::new((0, snapshot)),
            events,
            service,
            link_subscribers: AtomicUsize::new(0),
            receiver_subscribers: AtomicUsize::new(0),
            receiver: ReceiverClient::new(),
            started: Instant::now(),
            rates: Mutex::new(None),
            tray,
            quit,
            balloon,
        });
        state.update_tray();
        state
    }

    pub fn config_dir(&self) -> PathBuf {
        self.service.config_dir().to_path_buf()
    }

    pub fn snapshot(&self) -> (u64, Snapshot) {
        self.snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Reads part of the snapshot without copying all of it
    pub fn with_snapshot<R>(&self, f: impl FnOnce(&Snapshot) -> R) -> R {
        f(&self.snapshot.lock().unwrap_or_else(|e| e.into_inner()).1)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<(u64, Arc<Event>)> {
        self.events.subscribe()
    }

    /// Applies an event to the snapshot and sends it to the clients
    pub fn publish(&self, event: Event) {
        let rev = {
            let mut g = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
            let s = &mut g.1;
            match &event {
                Event::Adapter(a) => s.adapter = a.clone(),
                Event::Devices(d) => s.devices = d.clone(),
                Event::Scanning(b) => s.scanning = *b,
                Event::Stream(st) => s.stream = st.clone(),
                Event::Profiles { profiles, active_profile } => {
                    s.profiles = profiles.clone();
                    s.active_profile = active_profile.clone();
                }
                Event::Settings(x) => s.settings = x.clone(),
                Event::Update(u) => s.update = u.clone(),
                Event::Capture(c) => s.capture = c.clone(),
                // Not state: no revision
                Event::Link(_) | Event::Log(_) | Event::Receiver(_) | Event::Bye => {
                    let _ = self.events.send((g.0, Arc::new(event)));
                    return;
                }
            }
            g.0 += 1;
            g.0
        };
        let _ = self.events.send((rev, Arc::new(event)));
        self.update_tray();
    }

    fn update_tray(&self) {
        let (_, s) = self.snapshot();
        let lang = s.settings.language.clone();
        let streaming = s.stream.state == StreamState::Streaming;
        let base = if streaming {
            a2dpwb_core::messages::translate(&lang, "tray.tooltip_streaming", "A2DPWB - Streaming")
        } else {
            a2dpwb_core::messages::translate(&lang, "tray.tooltip_idle", "A2DPWB - Idle")
        };
        let tooltip = match (&s.stream.info, streaming) {
            (Some(i), true) => format!("{base}\n{} {} kbps", i.codec, i.bitrate_kbps),
            _ => base,
        };
        (self.tray)(TrayView {
            streaming,
            tooltip,
            profiles: s.profiles.iter().map(|p| p.name.clone()).collect(),
            active_profile: s.active_profile.clone(),
            language: lang,
        });
    }

    /// A service event (from its thread)
    pub fn on_service_event(&self, e: ServiceEvent) {
        let (_, s) = self.snapshot();
        let mut stream = s.stream.clone();
        let mut devices_changed = false;
        match e {
            ServiceEvent::State(state, m) => {
                stream.state = match state {
                    CoreStreamState::Idle => StreamState::Idle,
                    CoreStreamState::Connecting => StreamState::Connecting,
                    CoreStreamState::Streaming => StreamState::Streaming,
                    CoreStreamState::Reconnecting => StreamState::Reconnecting,
                    CoreStreamState::Error => StreamState::Error,
                };
                stream.message = msg(&m);
                if matches!(state, CoreStreamState::Idle | CoreStreamState::Error) {
                    stream.info = None;
                    stream.volume = None;
                    if state == CoreStreamState::Idle {
                        stream.profile = None;
                    }
                }
                // The stack runs once anything connected
                let adapter_status = match state {
                    CoreStreamState::Error if m.key.starts_with("error.btstack") => Some(AdapterStatus::Error),
                    CoreStreamState::Streaming => Some(AdapterStatus::Ready),
                    _ => None,
                };
                if let Some(st) = adapter_status {
                    let mut a = s.adapter.clone();
                    if a.status != st {
                        a.status = st;
                        a.error = (st == AdapterStatus::Error).then(|| msg(&m));
                        self.publish(Event::Adapter(a));
                    }
                }
                // A connection learns the device's codecs, also one that
                // fails afterwards
                devices_changed = state == CoreStreamState::Error;
            }
            ServiceEvent::StreamInfo(i) => {
                stream.info = Some(ipc::StreamInfo {
                    codec: i.codec,
                    bitrate_kbps: i.bitrate_kbps,
                    sample_rate: i.sample_rate,
                    channels: i.channels,
                    source_sample_rate: i.source_sample_rate,
                    source_channels: i.source_channels,
                    source_bits: i.source_bits,
                    requested_codec: i.requested_codec,
                    requested_in_use: i.requested_in_use,
                    source: Some(to_ipc_source(&i.source)),
                });
                // The device was saved when it connected
                devices_changed = true;
            }
            ServiceEvent::Volume(v) => stream.volume = Some(v),
        }
        self.publish(Event::Stream(stream));
        if devices_changed {
            self.refresh_devices();
        }
    }

    /// The device list again (saved file, last scan, codecs); published if
    /// it changed
    fn refresh_devices(&self) {
        let (_, s) = self.snapshot();
        let scanned: Vec<Device> = s.devices.iter().filter(|d| !d.saved).cloned().collect();
        let list = saved_devices(&self.config_dir(), &scanned, &self.service.device_codecs());
        if list != s.devices {
            self.publish(Event::Devices(list));
        }
    }

    fn save_profiles(&self, list: &[CoreProfile], active: Option<String>) -> Result<(), ErrorInfo> {
        let dir = self.config_dir();
        profiles::save(&dir, list).map_err(|_| err("profile.save_failed", "error.profile_save"))?;
        let legacy = self.service.settings().legacy_max_media_payload();
        self.publish(Event::Profiles {
            profiles: list.iter().map(|p| to_ipc_profile(p, legacy)).collect(),
            active_profile: active,
        });
        Ok(())
    }

    fn save_settings(&self, s: CoreSettings) -> Result<(), ErrorInfo> {
        s.save(&self.config_dir()).map_err(|_| err("settings.save_failed", "error.settings_save"))?;
        let ui = to_ipc_settings(&s);
        self.service.set_settings(s);
        self.publish(Event::Settings(ui));
        Ok(())
    }

    fn set_active_profile(&self, name: Option<String>) {
        let mut s = self.service.settings();
        if let Some(n) = &name
            && s.last_profile != *n
        {
            s.last_profile = n.clone();
            let _ = s.save(&self.config_dir());
            self.service.set_settings(s);
        }
        let (_, snap) = self.snapshot();
        self.publish(Event::Profiles { profiles: snap.profiles, active_profile: name });
    }

    pub fn refresh_adapters(&self) -> Vec<AdapterInfo> {
        let (_, s) = self.snapshot();
        let list = adapter_infos(&self.config_dir(), &self.service.settings());
        self.publish(Event::Adapter(AdapterState { adapters: list.clone(), ..s.adapter }));
        list
    }

    /// The chip or firmware setting changed: the firmware state anew, and a
    /// stack that loads the firmware again (v1 reset_btstack)
    fn reload_adapter(&self) {
        self.refresh_adapters();
        let (_, s) = self.snapshot();
        let idle = !s.scanning && matches!(s.stream.state, StreamState::Idle | StreamState::Error);
        if self.service.reload_adapter(idle) {
            tracing::info!("adapter settings changed: Bluetooth stopped, it starts again at the next use");
            self.publish(Event::Adapter(AdapterState { status: AdapterStatus::Stopped, error: None, ..s.adapter }));
        } else {
            tracing::info!("adapter settings changed: Bluetooth restarts at its next use");
        }
    }

    /// Starts streaming a profile (tray and UI)
    pub fn start_profile(&self, name: &str) -> Result<(), ErrorInfo> {
        let dir = self.config_dir();
        let settings = self.service.settings();
        let list = profiles::load(&dir);
        let p =
            list.iter().find(|p| p.name == name).ok_or_else(|| err("profile.not_found", "error.profile_not_found"))?;
        let req = p
            .to_request(settings.legacy_max_media_payload())
            .map_err(|_| err("profile.invalid_address", "error.invalid_address"))?;
        self.set_active_profile(Some(name.to_string()));
        self.begin_stream(req, Some(name.to_string()));
        Ok(())
    }

    /// Starts streaming without a profile (stream.start_custom)
    pub fn start_custom(&self, c: &CustomStream) -> Result<(), ErrorInfo> {
        let req = custom_request(c, self.service.settings().legacy_max_media_payload())?;
        tracing::info!("custom stream to {}: codec {}, source {}", c.device, c.codec, req.source);
        self.begin_stream(req, None);
        Ok(())
    }

    fn begin_stream(&self, req: StreamRequest, profile: Option<String>) {
        self.publish(Event::Stream(StreamStatus {
            state: StreamState::Connecting,
            message: Msg::new("status.initializing"),
            profile,
            info: None,
            volume: None,
        }));
        let service = self.service.clone();
        // start_streaming joins a running session first: not on a runtime thread
        std::thread::spawn(move || service.start_streaming(req));
    }

    pub fn stop(&self) {
        let service = self.service.clone();
        std::thread::spawn(move || service.stop_streaming());
    }

    pub fn quit(&self) {
        self.publish(Event::Bye);
        (self.quit)();
    }

    /// Start with Windows: the Run value written again at every start, so
    /// that it names this copy of the program (it may have moved)
    pub fn refresh_autostart(&self) {
        let s = self.service.settings();
        if s.start_with_windows
            && let Err(e) = apply_autostart(true, s.start_minimized)
        {
            tracing::warn!("cannot refresh the start with Windows entry: {e}");
        }
    }

    // ---- Tray balloons ------------------------------------------------------

    fn tr(&self, key: &str, default: &str) -> String {
        a2dpwb_core::messages::translate(&self.service.settings().language, key, default)
    }

    /// The UI hid its window to the tray (app.hidden_to_tray): v1's "still
    /// running in the tray" balloon, once
    pub fn hidden_to_tray(&self) {
        let mut s = self.service.settings();
        if s.other.get(TRAY_HINT_SHOWN).and_then(Value::as_bool) == Some(true) {
            return;
        }
        (self.balloon)(BalloonRequest {
            title: "A2DPWB".into(),
            text: self.tr("tray.minimized_hint", "A2DPWB is still running in the system tray."),
            url: None,
        });
        s.other.insert(TRAY_HINT_SHOWN.into(), Value::Bool(true));
        if let Err(e) = s.save(&self.config_dir()) {
            tracing::warn!("cannot save the settings: {e}");
        }
        self.service.set_settings(s);
    }

    /// The automatic check found a newer release: a balloon whose click
    /// opens its page (v1)
    fn announce_update(&self, u: &UpdateState) {
        let Some(a) = &u.available else { return };
        let text = self
            .tr("update.balloon", "A2DPWB %s is available. Click here to open the download page.")
            .replacen("%s", &a.version, 1);
        (self.balloon)(BalloonRequest {
            title: self.tr("update.available_title", "Update Available"),
            text,
            url: Some(a.url.clone()),
        });
    }

    // ---- Audio rates (flow) ------------------------------------------------

    pub fn set_rates(&self, r: Option<AudioRates>) {
        *self.rates.lock().unwrap_or_else(|e| e.into_inner()) = r;
    }

    pub fn rates(&self) -> Option<AudioRates> {
        *self.rates.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- HCI capture -------------------------------------------------------

    fn start_capture(&self) -> Result<String, ErrorInfo> {
        let capture = self.service.capture();
        if let Some(p) = capture.path() {
            return Ok(p.display().to_string());
        }
        let path = self.config_dir().join(format!("hci_{}.pklg", local_stamp()));
        capture.start(&path).map_err(|_| err("diag.capture_failed", "error.capture_start"))?;
        let path = path.display().to_string();
        self.publish(Event::Capture(Some(CaptureState {
            path: path.clone(),
            bytes: capture.bytes_written(),
            started_ms: unix_ms(),
        })));
        Ok(path)
    }

    /// Stops the capture: its file and final size (None: none ran)
    fn stop_capture(&self) -> Option<CaptureState> {
        let capture = self.service.capture();
        let known = self.with_snapshot(|s| s.capture.clone());
        let path = capture.path();
        capture.stop();
        let done = match path {
            Some(p) => Some(CaptureState {
                path: p.display().to_string(),
                bytes: capture.bytes_written(),
                started_ms: known.as_ref().map_or(0, |k| k.started_ms),
            }),
            None => known.clone(),
        };
        if known.is_some() {
            self.publish(Event::Capture(None));
        }
        done
    }

    /// The running capture's size (the sampler, every few seconds)
    pub fn refresh_capture(&self) {
        let Some(known) = self.with_snapshot(|s| s.capture.clone()) else { return };
        let capture = self.service.capture();
        match capture.path() {
            None => self.publish(Event::Capture(None)),
            Some(_) => {
                let bytes = capture.bytes_written();
                if bytes != known.bytes {
                    self.publish(Event::Capture(Some(CaptureState { bytes, ..known })));
                }
            }
        }
    }

    // ---- Receiver test -----------------------------------------------------

    pub fn receiver_subscribed(&self) -> bool {
        self.receiver_subscribers.load(Ordering::Acquire) > 0
    }

    pub fn receiver_state(&self) -> ReceiverState {
        let (status, detail) = self.receiver.status();
        let connection = match status {
            receiver::ReceiverStatus::Off => ReceiverConnection::Off,
            receiver::ReceiverStatus::Connecting => ReceiverConnection::Connecting,
            receiver::ReceiverStatus::Connected => ReceiverConnection::Connected,
            receiver::ReceiverStatus::Failed => ReceiverConnection::Failed,
        };
        let stats = self.receiver.latest();
        let (total, start, starts) = link_totals(&self.service.stats());
        ReceiverState {
            connection,
            detail,
            target: if connection == ReceiverConnection::Off { String::new() } else { self.receiver.target() },
            receiver: self.receiver.receiver().as_ref().and_then(convert),
            stats_age_ms: stats.as_ref().and_then(|s| s.received).map(|t| t.elapsed().as_millis() as u64),
            stats: stats.as_ref().and_then(convert),
            sent: (starts > 0).then(|| total.since(&start)),
            stream_starts: starts,
        }
    }

    /// Sends the receiver state to its subscribers
    pub fn publish_receiver(&self) {
        if self.receiver_subscribed() {
            self.publish(Event::Receiver(Box::new(self.receiver_state())));
        }
    }

    // ---- Requests ----------------------------------------------------------

    /// One request; blocking work runs off the runtime's threads
    pub async fn handle(self: &Arc<Self>, method: Method) -> Result<Value, ErrorInfo> {
        let me = self.clone();
        let blocking = |f: BlockingJob| async move {
            tokio::task::spawn_blocking(move || f(&me))
                .await
                .unwrap_or_else(|_| Err(err("internal", "error.btstack_crash_stream")))
        };
        match method {
            Method::StateSnapshot => Ok(json(&self.snapshot().1)),
            Method::StreamStart { profile } => self.start_profile(&profile).map(|()| Value::Null),
            Method::StreamStartCustom {
                device,
                device_name,
                codec,
                quality,
                sample_rate,
                bit_depth,
                source,
                source_id,
                source_name,
                max_media_payload,
                abr,
                codec_fallback,
            } => {
                let c = CustomStream {
                    device,
                    device_name,
                    codec,
                    quality,
                    sample_rate,
                    bit_depth,
                    source,
                    source_id,
                    source_name,
                    max_media_payload,
                    abr,
                    codec_fallback,
                };
                self.start_custom(&c).map(|()| Value::Null)
            }
            Method::StreamStop => {
                self.stop();
                Ok(Value::Null)
            }
            Method::StreamSetVolume { volume } => {
                if let Some(h) = self.service.host() {
                    h.request_volume(volume.min(127));
                }
                Ok(Value::Null)
            }
            Method::DeviceScan { seconds } => {
                blocking(Box::new(move |me| {
                    me.publish(Event::Scanning(true));
                    let r = me.service.scan(seconds.clamp(3, 30));
                    me.publish(Event::Scanning(false));
                    let found = r.map_err(|m| ErrorInfo { code: "adapter.init_failed".into(), message: msg(&m) })?;
                    let scanned: Vec<Device> = found
                        .iter()
                        .map(|d| Device {
                            address: bt_host::addr_str(&d.addr),
                            name: d.name.clone(),
                            saved: false,
                            audio: d.is_audio(),
                            rssi: Some(d.rssi),
                            codecs: Vec::new(),
                        })
                        .collect();
                    let list = saved_devices(&me.config_dir(), &scanned, &me.service.device_codecs());
                    me.publish(Event::Devices(list.clone()));
                    Ok(json(&list))
                }))
                .await
            }
            Method::DeviceForget { address } => {
                devices::remove(&self.config_dir(), &address)
                    .map_err(|_| err("device.forget_failed", "error.device_forget"))?;
                self.refresh_devices();
                Ok(Value::Null)
            }
            Method::DeviceRename { address, name } => {
                let name = name.trim();
                let Some(addr) = bt_host::parse_addr(&address) else {
                    return Err(err("device.invalid_address", "error.invalid_address_format"));
                };
                if name.is_empty() || name.contains(['\n', '\r', '|']) {
                    return Err(bad_request("device.invalid_name", "device.rename", format!("name \"{name}\"")));
                }
                devices::save(&self.config_dir(), &bt_host::addr_str(&addr), name)
                    .map_err(|_| err("device.rename_failed", "error.device_rename"))?;
                self.refresh_devices();
                Ok(Value::Null)
            }
            Method::ProfileSave { profile, original_name } => {
                if profile.name.trim().is_empty() || bt_host::parse_addr(&profile.device_address).is_none() {
                    return Err(err("profile.invalid", "error.invalid_address_format"));
                }
                let dir = self.config_dir();
                let mut list = profiles::load(&dir);
                let key = original_name.as_deref().unwrap_or(&profile.name).to_string();
                let pos = list.iter().position(|p| p.name == key);
                let merged = merge_profile(pos.map(|i| &list[i]), &profile);
                // The profile's device joins the saved ones (as a
                // connection saves it)
                let address = merged.device_address.clone();
                if !devices::load(&dir).iter().any(|d| d.address == address)
                    && let Err(e) = devices::save(&dir, &address, &merged.device_name)
                {
                    tracing::warn!("cannot save the device {address}: {e}");
                }
                match pos {
                    Some(i) => list[i] = merged,
                    None => list.push(merged),
                }
                let (_, s) = self.snapshot();
                let active = match s.active_profile {
                    Some(a) if a == key => Some(profile.name.clone()),
                    other => other.or(Some(profile.name.clone())),
                };
                self.save_profiles(&list, active)?;
                self.refresh_devices();
                Ok(Value::Null)
            }
            Method::ProfileDelete { name } => {
                let mut list = profiles::load(&self.config_dir());
                list.retain(|p| p.name != name);
                let (_, s) = self.snapshot();
                let active = s.active_profile.filter(|a| *a != name);
                self.save_profiles(&list, active)?;
                Ok(Value::Null)
            }
            Method::ProfileSetActive { name } => {
                self.set_active_profile(Some(name));
                Ok(Value::Null)
            }
            Method::SettingsSet { settings } => {
                blocking(Box::new(move |me| {
                    let base = me.service.settings();
                    let merged = merge_settings(&base, &settings);
                    if autostart_changed(&base, &merged) {
                        apply_autostart(merged.start_with_windows, merged.start_minimized).map_err(|e| {
                            tracing::warn!("start with Windows: {e}");
                            err("settings.autostart_failed", "error.autostart")
                        })?;
                    }
                    let chip = chip_changed(&base, &merged);
                    me.save_settings(merged)?;
                    if chip {
                        me.reload_adapter();
                    }
                    Ok(Value::Null)
                }))
                .await
            }
            Method::AdapterList => blocking(Box::new(|me| Ok(json(&me.refresh_adapters())))).await,
            Method::AdapterAssignWinusb { instance_id } => {
                blocking(Box::new(move |me| crate::driver::assign(me, &instance_id))).await
            }
            Method::AdapterRestoreDriver { instance_id } => {
                blocking(Box::new(move |me| crate::driver::restore(me, &instance_id))).await
            }
            Method::AdapterRestart => {
                blocking(Box::new(|me| {
                    me.service.release_adapter();
                    let (_, s) = me.snapshot();
                    me.publish(Event::Adapter(AdapterState {
                        status: AdapterStatus::Stopped,
                        error: None,
                        ..s.adapter
                    }));
                    Ok(Value::Null)
                }))
                .await
            }
            Method::AdapterFirmwareFiles => {
                blocking(Box::new(|me| {
                    let list: Vec<FirmwareFile> = chips::scan_firmware_files(&me.config_dir())
                        .into_iter()
                        .map(|f| FirmwareFile {
                            stem: f.stem,
                            display_name: f.display_name,
                            pid: f.pid,
                            has_config: f.has_config,
                        })
                        .collect();
                    Ok(json(&list))
                }))
                .await
            }
            Method::AudioDevices => {
                blocking(Box::new(|_| {
                    let list: Vec<AudioDevice> = audio_capture::render_devices()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|d| {
                            let mix = audio_capture::mix_format(Some(&d.id));
                            AudioDevice {
                                mix_rate: mix.map(|m| m.rate),
                                mix_channels: mix.map(|m| m.channels),
                                mix_bits: mix.map(|m| m.bits),
                                id: d.id,
                                name: d.name,
                                is_default: d.is_default,
                                is_virtual: d.is_virtual,
                            }
                        })
                        .collect();
                    Ok(json(&list))
                }))
                .await
            }
            Method::AudioApps => {
                blocking(Box::new(|_| {
                    let list: Vec<AudioApp> = audio_capture::audio_apps()
                        .into_iter()
                        .map(|a| AudioApp { exe: a.exe, name: a.name })
                        .collect();
                    Ok(json(&list))
                }))
                .await
            }
            // Handled per client by the server
            Method::DiagSubscribe { .. } => Ok(Value::Null),
            Method::DiagCaptureStart => self.start_capture().map(Value::String),
            Method::DiagCaptureStop => Ok(json(&self.stop_capture())),
            Method::DiagInfo(params) => {
                let mask = params.and_then(|p| p.mask).unwrap_or(true);
                // Lists the USB adapters: off the runtime
                blocking(Box::new(move |me| Ok(json(&DiagInfo { text: crate::diag_text(me, mask) })))).await
            }
            Method::DiagFlow => Ok(json(&crate::flow_view(self))),
            Method::DiagRecentLogs { level } => {
                let mut lines = crate::logging::hub().recent();
                lines.retain(|l| l.level <= level);
                Ok(json(&lines))
            }
            Method::ReceiverDiscover => {
                blocking(Box::new(|_| {
                    let found: Vec<ipc::ReceiverInfo> =
                        receiver::discover(None, receiver::DEFAULT_PORT, receiver::DISCOVER_TIMEOUT)
                            .iter()
                            .filter_map(convert)
                            .collect();
                    Ok(json(&found))
                }))
                .await
            }
            Method::ReceiverConnect { target } => {
                let target = if target.trim().is_empty() { receiver::AUTO.to_string() } else { target.trim().into() };
                if receiver::split_target(&target).is_none() {
                    return Err(bad_request("receiver.invalid_target", "receiver.connect", format!("\"{target}\"")));
                }
                blocking(Box::new(move |me| {
                    // Ends a previous connection first (bounded by its
                    // connect and discovery timeouts)
                    me.receiver.start(&target);
                    let mut s = me.service.settings();
                    if s.remote_sink != target {
                        s.remote_sink = target;
                        me.save_settings(s)?;
                    }
                    me.publish_receiver();
                    Ok(json(&me.receiver_state()))
                }))
                .await
            }
            Method::ReceiverDisconnect => {
                blocking(Box::new(|me| {
                    me.receiver.stop();
                    me.publish_receiver();
                    Ok(Value::Null)
                }))
                .await
            }
            Method::ReceiverConfigure { mtu, buffer_ms } => {
                let (mtu, buffer_ms) = (mtu.unwrap_or(0), buffer_ms.unwrap_or(0));
                if mtu > u16::MAX as u32 || (mtu != 0 && mtu < 48) || buffer_ms > 10_000 {
                    return Err(bad_request(
                        "receiver.bad_settings",
                        "receiver.configure",
                        format!("mtu {mtu}, buffer {buffer_ms} ms"),
                    ));
                }
                if !self.receiver.configure(mtu, buffer_ms) {
                    return Err(err("receiver.not_connected", "receiver.off"));
                }
                Ok(Value::Null)
            }
            Method::ReceiverStatus => Ok(json(&self.receiver_state())),
            Method::AppCheckUpdate => blocking(Box::new(|me| me.check_update().map(|u| json(&u)))).await,
            Method::AppHiddenToTray => {
                self.hidden_to_tray();
                Ok(Value::Null)
            }
            Method::AppQuit => {
                self.quit();
                Ok(Value::Null)
            }
            Method::AppQuitForUpdate => match self.service.state() {
                CoreStreamState::Connecting | CoreStreamState::Streaming | CoreStreamState::Reconnecting => {
                    Err(err("app.streaming", "ui.update.streaming"))
                }
                CoreStreamState::Idle | CoreStreamState::Error => {
                    tracing::info!("exiting for an update");
                    self.quit();
                    Ok(Value::Null)
                }
            },
        }
    }

    /// Asks GitHub for a newer release (blocking) and publishes the result;
    /// Err (update.failed) when it could not ask, and the state says so too
    pub fn check_update(&self) -> Result<UpdateState, ErrorInfo> {
        use a2dpwb_core::update;
        let (_, s) = self.snapshot();
        self.publish(Event::Update(UpdateState { checking: true, ..s.update.clone() }));
        let prereleases = self.service.settings().include_prereleases;
        let current_version = update::current_version().to_string();
        match update::check(update::current_version(), prereleases) {
            Ok(r) => {
                let available = r.map(|r| ipc::UpdateInfo {
                    manifest_url: update::manifest_url(&r.tag),
                    version: r.version,
                    url: r.url,
                    prerelease: r.prerelease,
                });
                let u = UpdateState { checking: false, available, current_version, error: None };
                self.publish(Event::Update(u.clone()));
                Ok(u)
            }
            Err(e) => {
                tracing::warn!("update check failed: {e}");
                let error = Msg::new("update.error");
                self.publish(Event::Update(UpdateState {
                    checking: false,
                    available: s.update.available.clone(),
                    current_version,
                    error: Some(error.clone()),
                }));
                Err(ErrorInfo { code: "update.failed".into(), message: error })
            }
        }
    }

    /// The automatic check: at most once a day (recorded before asking, as v1)
    pub fn check_update_if_due(self: &Arc<Self>) {
        use a2dpwb_core::update;
        let mut s = self.service.settings();
        let today = update::today();
        if !update::automatic_check_due(&s, &today) {
            return;
        }
        s.last_update_check = today;
        let _ = s.save(&self.config_dir());
        self.service.set_settings(s);
        let me = self.clone();
        std::thread::spawn(move || {
            // A failure is in the update state
            if let Ok(u) = me.check_update() {
                me.announce_update(&u);
            }
        });
    }

    pub fn link_subscribed(&self) -> bool {
        self.link_subscribers.load(Ordering::Acquire) > 0
    }
}

/// yyyyMMdd_HHmmss of the local time, for file names (as v1)
fn local_stamp() -> String {
    let t = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    format!("{:04}{:02}{:02}_{:02}{:02}{:02}", t.year(), u8::from(t.month()), t.day(), t.hour(), t.minute(), t.second())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(source: &str) -> CustomStream {
        CustomStream {
            device: "aa:bb:cc:dd:ee:ff".into(),
            codec: "ldac".into(),
            quality: "sq".into(),
            source: source.into(),
            ..CustomStream::default()
        }
    }

    #[test]
    fn custom_streams_are_built_as_profiles() {
        let r = custom_request(&custom("tone"), 679).unwrap();
        assert_eq!(r.device, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        assert_eq!(r.codec, Some(codec::CodecKind::Ldac));
        assert_eq!(r.quality, codec::Quality::Standard);
        assert_eq!(r.source, SourceRequest::TestTone);
        assert_eq!(r.volume_sync, a2dpwb_core::VolumeSync::Off);
        assert!(!r.codec_fallback);
        assert_eq!(r.max_media_payload, 679);
        assert_eq!((r.sample_rate, r.bit_depth), (None, None));

        let mut c = custom("app");
        c.source_id = Some("Spotify.exe".into());
        c.sample_rate = Some(96000);
        c.bit_depth = Some(24);
        c.max_media_payload = Some(1023);
        c.codec = "auto".into();
        let r = custom_request(&c, 679).unwrap();
        assert_eq!(r.source, SourceRequest::Application { exe: "Spotify.exe".into() });
        assert_eq!((r.sample_rate, r.bit_depth, r.codec, r.max_media_payload), (Some(96000), Some(24), None, 1023));

        let mut v = custom("virtual");
        v.source_id = Some("{dev}".into());
        v.source_name = Some("CABLE Input".into());
        let r = custom_request(&v, 679).unwrap();
        assert_eq!(
            r.source,
            SourceRequest::VirtualDevice { id: "{dev}".into(), name: "CABLE Input".into(), auto_switch: false }
        );
        assert_eq!(custom_request(&custom("loopback"), 679).unwrap().source, SourceRequest::SystemLoopback);
    }

    #[test]
    fn bad_custom_streams_are_refused() {
        assert_eq!(custom_request(&custom("radio"), 679).unwrap_err().code, "stream.bad_source");
        let mut c = custom("tone");
        c.codec = "mp3".into();
        assert_eq!(custom_request(&c, 679).unwrap_err().code, "stream.bad_codec");
        c.codec = "sbc".into();
        c.device = "not an address".into();
        assert_eq!(custom_request(&c, 679).unwrap_err().code, "profile.invalid_address");
    }

    #[test]
    fn receiver_types_convert_field_for_field() {
        let line = concat!(
            r#"{"type":"stats","version":1,"connected":true,"streaming":true,"device":"AA:BB:CC:DD:EE:FF","#,
            r#""buffer_target_ms":200,"mtu":1005,"codecs":["SBC","LDAC"],"rssi":-3,"tx_power":4,"rssi_note":"","#,
            r#""afh_channels":40,"stream":{"id":2,"codec":"LDAC","config":"LDAC 96 kHz","decoding":true,"#,
            r#""packets":100,"bytes":100000,"frames":400,"lost":1,"late":2,"ts_errors":3,"frame_errors":4,"#,
            r#""decode_errors":5,"underruns":6,"overflows":7,"pauses":8,"idle":false,"audio_ms":1000.5,"#,
            r#""underrun_ms":12.5,"overflow_ms":1.5,"max_gap_ms":30.0,"jitter_ms":2.5,"buffer_ms":180.0,"#,
            r#""interval":{"packets":120,"bytes":120000,"max_gap_ms":20.0}}}"#
        );
        let core = receiver::parse_stats(line).expect("a stats line");
        let ipc: ipc::ReceiverStats = convert(&core).expect("converts");
        // Every field arrives: both give the same JSON
        assert_eq!(json(&ipc), json(&core));
        assert_eq!(ipc.stream.as_ref().map(|s| (s.decode_errors, s.interval_bytes)), Some((5, 120000)));

        let info = receiver::ReceiverInfo {
            ip: "192.168.1.5".into(),
            port: 51201,
            host: "sink".into(),
            bt_address: "00:11:22:33:44:55".into(),
            bt_name: "hci0".into(),
            codecs: vec!["SBC".into()],
            mtu: 895,
            buffer_ms: 200,
        };
        let i: ipc::ReceiverInfo = convert(&info).expect("converts");
        assert_eq!(json(&i), json(&info));
    }

    #[test]
    fn link_totals_since_the_stream_started() {
        let s = LinkStats::default();
        let r = Ordering::Relaxed;
        s.packets_sent.store(10, r);
        s.queue_drops.store(1, r);
        s.send_errors.store(2, r);
        s.start_packets.store(4, r);
        s.start_dropped.store(1, r);
        s.stream_starts.store(1, r);
        let (total, start, starts) = link_totals(&s);
        assert_eq!(starts, 1);
        assert_eq!(total.dropped, 3);
        let since = total.since(&start);
        assert_eq!((since.packets, since.dropped), (6, 2));
    }

    #[test]
    fn autostart_follows_both_settings() {
        let off = CoreSettings::default();
        let on = CoreSettings { start_with_windows: true, ..CoreSettings::default() };
        let on_min = CoreSettings { start_minimized: true, ..on.clone() };
        assert!(autostart_changed(&off, &on));
        assert!(autostart_changed(&on, &on_min));
        assert!(!autostart_changed(&on, &on));
        // Minimized alone does not matter while autostart is off
        assert!(!autostart_changed(&off, &CoreSettings { start_minimized: true, ..off.clone() }));
        assert_eq!(autostart_args(true), "--background");
        assert_eq!(autostart_args(false), "");
    }

    #[test]
    fn chip_changes_need_new_firmware() {
        let a = CoreSettings::default();
        assert!(!chip_changed(&a, &a.clone()));
        assert!(chip_changed(&a, &CoreSettings { bt_chip_pid: 0x8771, ..a.clone() }));
        assert!(chip_changed(&a, &CoreSettings { bt_chip_fw_stem: "rtl8761bu".into(), ..a.clone() }));
    }

    #[test]
    fn vendors_get_their_notes() {
        assert_eq!(vendor_note(ChipVendor::Intel).unwrap().key, "firmware.vendor_intel");
        assert_eq!(vendor_note(ChipVendor::Qualcomm).unwrap().key, "firmware.vendor_unsupported");
        assert!(vendor_note(ChipVendor::Realtek).is_none());
    }

    #[test]
    fn devices_carry_their_codecs() {
        let dir = std::env::temp_dir().join(format!("a2dpwb-state-dev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        devices::save(&dir, "AA:BB:CC:DD:EE:FF", "Phones").unwrap();
        let scanned = vec![Device {
            address: "11:22:33:44:55:66".into(),
            name: "Speaker".into(),
            saved: false,
            audio: true,
            rssi: Some(-60),
            codecs: Vec::new(),
        }];
        let mut codecs = BTreeMap::new();
        codecs.insert("AA:BB:CC:DD:EE:FF".to_string(), vec!["sbc".to_string(), "ldac".to_string()]);
        let list = saved_devices(&dir, &scanned, &codecs);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].codecs, ["sbc", "ldac"]);
        assert!(list[0].saved && !list[1].saved && list[1].codecs.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn capture_names_use_the_local_time() {
        let s = local_stamp();
        assert_eq!(s.len(), 15);
        assert_eq!(&s[8..9], "_");
        assert!(s.bytes().enumerate().all(|(i, b)| i == 8 || b.is_ascii_digit()));
    }
}
