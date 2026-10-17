//! The resident core's state as the UI sees it (an ipc::Snapshot with a
//! revision), the events that change it, and the requests that act on it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use a2dpwb_core::profiles::{self, Profile as CoreProfile};
use a2dpwb_core::settings::Settings as CoreSettings;
use a2dpwb_core::{Service, ServiceEvent, StreamState as CoreStreamState, devices};
use bt_host::chips;
use ipc::{
    AdapterInfo, AdapterState, AdapterStatus, AudioApp, AudioDevice, Device, DiagInfo, DriverKind, ErrorInfo, Event,
    Method, Msg, Profile, Settings, Snapshot, StreamState, StreamStatus, UpdateState,
};
use serde_json::Value;
use tokio::sync::broadcast;

/// Work of a request that blocks (scan, driver change), run off the runtime
type BlockingJob = Box<dyn FnOnce(&Arc<CoreState>) -> Result<Value, ErrorInfo> + Send>;

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
    pub started: Instant,
    tray: Box<dyn Fn(TrayView) + Send + Sync>,
    quit: Box<dyn Fn() + Send + Sync>,
}

pub fn msg(m: &a2dpwb_core::Msg) -> Msg {
    Msg { key: m.key.to_string(), args: m.args.clone() }
}

fn err(code: &str, key: &str) -> ErrorInfo {
    ErrorInfo { code: code.into(), message: Msg::new(key) }
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
            }
        })
        .collect()
}

fn saved_devices(config_dir: &Path, scanned: &[Device]) -> Vec<Device> {
    let mut list: Vec<Device> = devices::load(config_dir)
        .into_iter()
        .map(|d| Device { address: d.address, name: d.name, saved: true, audio: true, rssi: None })
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
    list
}

impl CoreState {
    pub fn new(
        service: Arc<Service>,
        tray: Box<dyn Fn(TrayView) + Send + Sync>,
        quit: Box<dyn Fn() + Send + Sync>,
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
            devices: saved_devices(&dir, &[]),
            scanning: false,
            stream: StreamStatus { message: Msg::new("status.ready"), ..Default::default() },
            profiles,
            active_profile: active,
            settings: to_ipc_settings(&settings),
            update: UpdateState { current_version: env!("CARGO_PKG_VERSION").into(), ..Default::default() },
            config_dir: dir.display().to_string(),
        };
        let (events, _) = broadcast::channel(256);
        let state = Arc::new(CoreState {
            snapshot: Mutex::new((0, snapshot)),
            events,
            service,
            link_subscribers: AtomicUsize::new(0),
            started: Instant::now(),
            tray,
            quit,
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
                // Not state: no revision
                Event::Link(_) | Event::Log(_) | Event::Bye => {
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
                })
            }
            ServiceEvent::Volume(v) => stream.volume = Some(v),
        }
        self.publish(Event::Stream(stream));
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
        let (_, s) = self.snapshot();
        self.publish(Event::Stream(StreamStatus {
            state: StreamState::Connecting,
            message: Msg::new("status.initializing"),
            profile: Some(name.to_string()),
            info: None,
            volume: None,
        }));
        drop(s);
        let service = self.service.clone();
        // start_streaming joins a running session first: not on a runtime thread
        std::thread::spawn(move || service.start_streaming(req));
        Ok(())
    }

    pub fn stop(&self) {
        let service = self.service.clone();
        std::thread::spawn(move || service.stop_streaming());
    }

    pub fn quit(&self) {
        self.publish(Event::Bye);
        (self.quit)();
    }

    /// One request; blocking work runs off the runtime's threads
    pub async fn handle(self: &Arc<Self>, method: Method) -> Result<Value, ErrorInfo> {
        let me = self.clone();
        let blocking = |f: BlockingJob| async move {
            tokio::task::spawn_blocking(move || f(&me))
                .await
                .unwrap_or_else(|_| Err(err("internal", "error.btstack_crash_stream")))
        };
        match method {
            Method::StateSnapshot => Ok(serde_json::to_value(self.snapshot().1).unwrap_or(Value::Null)),
            Method::StreamStart { profile } => self.start_profile(&profile).map(|()| Value::Null),
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
                        })
                        .collect();
                    let list = saved_devices(&me.config_dir(), &scanned);
                    me.publish(Event::Devices(list.clone()));
                    Ok(serde_json::to_value(list).unwrap_or(Value::Null))
                }))
                .await
            }
            Method::DeviceForget { address } => {
                devices::remove(&self.config_dir(), &address)
                    .map_err(|_| err("device.forget_failed", "error.device_forget"))?;
                let (_, s) = self.snapshot();
                let scanned: Vec<Device> = s.devices.into_iter().filter(|d| !d.saved).collect();
                self.publish(Event::Devices(saved_devices(&self.config_dir(), &scanned)));
                Ok(Value::Null)
            }
            Method::ProfileSave { profile, original_name } => {
                if profile.name.trim().is_empty() || bt_host::parse_addr(&profile.device_address).is_none() {
                    return Err(err("profile.invalid", "error.invalid_address_format"));
                }
                let mut list = profiles::load(&self.config_dir());
                let key = original_name.as_deref().unwrap_or(&profile.name).to_string();
                let pos = list.iter().position(|p| p.name == key);
                let merged = merge_profile(pos.map(|i| &list[i]), &profile);
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
                let base = self.service.settings();
                let merged = merge_settings(&base, &settings);
                if merged.start_with_windows != base.start_with_windows {
                    apply_autostart(merged.start_with_windows, merged.start_minimized)
                        .map_err(|_| err("settings.autostart_failed", "error.autostart"))?;
                }
                self.save_settings(merged)?;
                Ok(Value::Null)
            }
            Method::AdapterList => {
                blocking(Box::new(|me| Ok(serde_json::to_value(me.refresh_adapters()).unwrap_or(Value::Null)))).await
            }
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
            Method::AudioDevices => {
                blocking(Box::new(|_| {
                    let list: Vec<AudioDevice> = audio_capture::render_devices()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|d| AudioDevice {
                            id: d.id,
                            name: d.name,
                            is_default: d.is_default,
                            is_virtual: d.is_virtual,
                        })
                        .collect();
                    Ok(serde_json::to_value(list).unwrap_or(Value::Null))
                }))
                .await
            }
            Method::AudioApps => {
                blocking(Box::new(|_| {
                    let list: Vec<AudioApp> = audio_capture::audio_apps()
                        .into_iter()
                        .map(|a| AudioApp { exe: a.exe, name: a.name })
                        .collect();
                    Ok(serde_json::to_value(list).unwrap_or(Value::Null))
                }))
                .await
            }
            // Handled per client by the server
            Method::DiagSubscribe { .. } => Ok(Value::Null),
            Method::DiagCaptureStart => {
                let capture = self.service.capture();
                if let Some(p) = capture.path() {
                    return Ok(Value::String(p.display().to_string()));
                }
                let name = format!("hci_{}.pklg", chrono_like_stamp());
                let path = self.config_dir().join(name);
                capture.start(&path).map_err(|_| err("diag.capture_failed", "error.capture_start"))?;
                Ok(Value::String(path.display().to_string()))
            }
            Method::DiagCaptureStop => {
                self.service.capture().stop();
                Ok(Value::Null)
            }
            Method::DiagInfo => {
                Ok(serde_json::to_value(DiagInfo { text: crate::diag_text(self) }).unwrap_or(Value::Null))
            }
            Method::AppCheckUpdate => Ok(serde_json::to_value(self.snapshot().1.update).unwrap_or(Value::Null)),
            Method::AppQuit => {
                self.quit();
                Ok(Value::Null)
            }
        }
    }

    pub fn link_subscribed(&self) -> bool {
        self.link_subscribers.load(Ordering::Acquire) > 0
    }
}

/// HKCU Run value of v2: the resident core, in the background
fn apply_autostart(enabled: bool, _minimized: bool) -> Result<(), String> {
    use win_sys::autostart;
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        autostart::set(autostart::VALUE_NAME, &autostart::command_line(&exe, "--background")).map_err(|e| e.to_string())
    } else {
        autostart::remove(autostart::VALUE_NAME).map(|_| ()).map_err(|e| e.to_string())
    }
}

/// yyyyMMdd_HHmmss of the local time, for file names
fn chrono_like_stamp() -> String {
    // UTC: the capture's own records carry exact times
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = (secs / 86400, secs % 86400);
    // Civil date from days since 1970 (Howard Hinnant's algorithm)
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}{m:02}{d:02}_{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}
