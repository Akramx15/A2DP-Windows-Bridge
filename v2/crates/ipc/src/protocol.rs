//! The messages between a2dpwb-core.exe (server) and the UI (client).
//!
//! 1. The client sends [`ClientMessage::Hello`]; a different major protocol
//!    version is answered with [`ServerMessage::Reject`].
//! 2. The server sends [`ServerMessage::Welcome`] and a
//!    [`ServerMessage::Snapshot`] of its whole state with a revision.
//! 3. Then [`ServerMessage::Event`]s, each replacing one part of the state,
//!    with the next revision. A gap in the revisions means a lost event: the
//!    client asks for a new snapshot ([`Method::StateSnapshot`]).
//! 4. Requests ([`ClientMessage::Request`]) are answered by a
//!    [`ServerMessage::Response`] with the same id.
//!
//! Errors and status texts are translation keys of app/lang ([`Msg`]): the
//! UI translates them.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const PROTOCOL_VERSION: u32 = 1;

/// A message of app/lang: key and printf-style arguments
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]

pub struct Msg {
    pub key: String,
    pub args: Vec<String>,
}

impl Msg {
    pub fn new(key: impl Into<String>) -> Msg {
        Msg { key: key.into(), args: Vec::new() }
    }
}

/// An error answer: a code the UI can act on ("adapter.driver_missing"),
/// and the message to show
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]

pub struct ErrorInfo {
    pub code: String,
    pub message: Msg,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]

pub enum AdapterStatus {
    /// No stack running (started on the first scan or connection)
    #[default]
    Stopped,
    Initializing,
    Ready,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]

pub enum DriverKind {
    /// WinUSB: usable by A2DPWB
    Winusb,
    /// Windows' Bluetooth driver: the adapter is Windows'
    Bluetooth,
    Other,
    None,
}

/// A USB Bluetooth adapter
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]

pub struct AdapterInfo {
    pub instance_id: String,
    pub vid: u16,
    pub pid: u16,
    /// "RTL8761BU (0x2357:0x0604)"
    pub display_name: String,
    /// Windows' name of the device
    pub device_name: String,
    pub vendor: String,
    pub driver: DriverKind,
    /// Driver service (WinUSB, BTHUSB, ...)
    pub driver_service: String,
    /// Realtek chip for the firmware download, if one
    pub realtek_chip: Option<String>,
    /// The firmware files for it are in the settings folder (None: no
    /// firmware needed)
    pub firmware_present: Option<bool>,
    /// The firmware file names it needs ("rtl8761bu_fw.bin")
    pub firmware_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]

pub struct AdapterState {
    pub status: AdapterStatus,
    pub adapters: Vec<AdapterInfo>,
    pub error: Option<Msg>,
}

/// A device known from the saved list or a scan
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]

pub struct Device {
    /// "XX:XX:XX:XX:XX:XX"
    pub address: String,
    pub name: String,
    pub saved: bool,
    /// Audio class (headphones, speakers)
    pub audio: bool,
    /// From the last scan
    pub rssi: Option<i8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]

pub enum StreamState {
    #[default]
    Idle,
    Connecting,
    Streaming,
    Reconnecting,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]

pub struct StreamInfo {
    pub codec: String,
    pub bitrate_kbps: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub source_sample_rate: u32,
    pub source_channels: u32,
    pub source_bits: u32,
    /// The profile's codec, when the device did not offer it (fallback)
    pub requested_codec: Option<String>,
    pub requested_in_use: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]

pub struct StreamStatus {
    pub state: StreamState,
    pub message: Msg,
    /// Profile being streamed
    pub profile: Option<String>,
    pub info: Option<StreamInfo>,
    /// Headphone volume (AVRCP, 0..127), None while unknown
    pub volume: Option<u8>,
}

/// A connection profile, as in profiles.json
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]

pub struct Profile {
    pub name: String,
    pub device_address: String,
    pub device_name: String,
    /// auto, ldac, aptxhd, aptxll, aptx, sbc, aac, lhdcv5
    pub codec: String,
    /// hq, sq, mq; mhq for LHDC V5
    pub quality: String,
    pub abr: bool,
    pub codec_fallback: bool,
    /// 0 = auto
    pub sample_rate: u32,
    /// 0 = auto, 16, 24
    pub bit_depth: u32,
    /// loopback, virtual, app
    pub capture_mode: String,
    pub audio_device_id: String,
    pub audio_device_name: String,
    pub auto_switch_device: bool,
    pub app_exe: String,
    pub app_name: String,
    /// auto, off, device
    pub volume_sync: String,
    pub volume_device_id: String,
    pub volume_device_name: String,
    pub max_media_payload: u16,
}

/// Application settings the UI edits
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]

pub struct Settings {
    pub language: String,
    /// system, light, dark
    pub theme: String,
    pub start_with_windows: bool,
    pub start_minimized: bool,
    pub minimize_to_tray: bool,
    pub check_updates_on_startup: bool,
    pub include_prereleases: bool,
    pub media_keys: bool,
    pub remember_volume: bool,
    /// Developer mode: receiver test, HCI capture, debug details
    pub developer_mode: bool,
    /// 0 = auto, a Realtek PID, 0xFFFE = not Realtek
    pub bt_chip_pid: u16,
    pub bt_chip_fw_stem: String,
    /// auto, off, or Wi-Fi channels such as "6,11"
    pub afh: String,
    pub remote_sink: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]

pub struct UpdateState {
    pub checking: bool,
    /// A newer release, if any
    pub available: Option<UpdateInfo>,
    pub current_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]

pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub prerelease: bool,
}

/// The whole state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]

pub struct Snapshot {
    pub adapter: AdapterState,
    pub devices: Vec<Device>,
    pub scanning: bool,
    pub stream: StreamStatus,
    pub profiles: Vec<Profile>,
    pub active_profile: Option<String>,
    pub settings: Settings,
    pub update: UpdateState,
    /// The settings folder (shown, and opened from the UI)
    pub config_dir: String,
}

/// Link quality, 4 times a second while subscribed
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]

pub struct LinkSample {
    /// Milliseconds since the core started
    pub time_ms: u64,
    pub rssi: Option<i8>,
    pub afh_in_use: Option<u32>,
    pub afh_usable: Option<u32>,
    pub afh_avoided: String,
    /// Media bytes sent per second (RTP and headers included)
    pub bytes_per_second: f64,
    pub packets_per_second: f64,
    pub queue_depth: u32,
    /// Packets lost before the radio since the stream started
    pub dropped: u64,
    /// Captured frames dropped because the encoder fell behind
    pub capture_dropped: u64,
    /// Share of the link's air time the stream takes (3-DH5 packets)
    pub airtime: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]

pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]

pub struct LogLine {
    pub time_ms: u64,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
}

/// One part of the state changed, or a subscribed stream produced data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]

pub enum Event {
    Adapter(AdapterState),
    Devices(Vec<Device>),
    Scanning(bool),
    Stream(StreamStatus),
    Profiles {
        profiles: Vec<Profile>,
        active_profile: Option<String>,
    },
    Settings(Settings),
    Update(UpdateState),
    /// Subscription (diag.subscribe link)
    Link(LinkSample),
    /// Subscription (diag.subscribe logs)
    Log(LogLine),
    /// The core is shutting down
    Bye,
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/// A request and its parameters. Results (in [`ServerMessage::Response`])
/// are noted per method; `null` where none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "method", content = "params")]

pub enum Method {
    /// -> Snapshot
    #[serde(rename = "state.snapshot")]
    StateSnapshot,
    /// Streams this profile (stopping one that runs)
    #[serde(rename = "stream.start")]
    StreamStart { profile: String },
    #[serde(rename = "stream.stop")]
    StreamStop,
    /// AVRCP absolute volume 0..127
    #[serde(rename = "stream.set_volume")]
    StreamSetVolume { volume: u8 },
    /// -> Device[] (also as an event)
    #[serde(rename = "device.scan")]
    DeviceScan { seconds: u32 },
    #[serde(rename = "device.forget")]
    DeviceForget { address: String },
    /// Adds or replaces a profile (`original_name`: the one renamed)
    #[serde(rename = "profile.save")]
    ProfileSave { profile: Profile, original_name: Option<String> },
    #[serde(rename = "profile.delete")]
    ProfileDelete { name: String },
    #[serde(rename = "profile.set_active")]
    ProfileSetActive { name: String },
    #[serde(rename = "settings.set")]
    SettingsSet { settings: Settings },
    /// -> AdapterInfo[] (also as an event)
    #[serde(rename = "adapter.list")]
    AdapterList,
    /// Assigns WinUSB (UAC prompt) -> AdapterInfo[]
    #[serde(rename = "adapter.assign_winusb")]
    AdapterAssignWinusb { instance_id: String },
    /// Back to the previous driver (UAC prompt) -> AdapterInfo[]
    #[serde(rename = "adapter.restore_driver")]
    AdapterRestoreDriver { instance_id: String },
    /// Restarts the Bluetooth stack
    #[serde(rename = "adapter.restart")]
    AdapterRestart,
    /// -> AudioDevice[]
    #[serde(rename = "audio.devices")]
    AudioDevices,
    /// -> AudioApp[]
    #[serde(rename = "audio.apps")]
    AudioApps,
    /// Log lines at `logs` level and above (None: off), link samples
    #[serde(rename = "diag.subscribe")]
    DiagSubscribe { logs: Option<LogLevel>, link: bool },
    /// -> the capture file's path
    #[serde(rename = "diag.capture_start")]
    DiagCaptureStart,
    #[serde(rename = "diag.capture_stop")]
    DiagCaptureStop,
    /// -> DiagInfo (text for "copy diagnostic info")
    #[serde(rename = "diag.info")]
    DiagInfo,
    /// -> UpdateState
    #[serde(rename = "app.check_update")]
    AppCheckUpdate,
    /// Ends the core (streaming stops)
    #[serde(rename = "app.quit")]
    AppQuit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]

pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_virtual: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]

pub struct AudioApp {
    pub exe: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]

pub struct DiagInfo {
    pub text: String,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]

pub enum ClientMessage {
    Hello {
        protocol: u32,
        /// "a2dpwb-ui/2.0.0"
        client: String,
    },
    Request {
        id: u64,
        #[serde(flatten)]
        call: Method,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]

pub enum ServerMessage {
    Welcome {
        protocol: u32,
        /// "a2dpwb-core/2.0.0"
        server: String,
    },
    Reject {
        reason: String,
    },
    Snapshot {
        rev: u64,
        state: Box<Snapshot>,
    },
    Event {
        rev: u64,
        event: Box<Event>,
    },
    Response {
        id: u64,
        #[ts(type = "unknown")]
        result: Option<serde_json::Value>,
        error: Option<ErrorInfo>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format() {
        let m = ClientMessage::Request { id: 7, call: Method::StreamStart { profile: "WH".into() } };
        let j = serde_json::to_value(&m).unwrap();
        assert_eq!(
            j,
            serde_json::json!({"type": "request", "id": 7, "method": "stream.start", "params": {"profile": "WH"}})
        );
        let back: ClientMessage = serde_json::from_value(j).unwrap();
        assert_eq!(back, m);
        let stop: ClientMessage =
            serde_json::from_value(serde_json::json!({"type": "request", "id": 1, "method": "stream.stop"})).unwrap();
        assert_eq!(stop, ClientMessage::Request { id: 1, call: Method::StreamStop });
        let e = ServerMessage::Event { rev: 3, event: Box::new(Event::Scanning(true)) };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({"type": "event", "rev": 3, "event": {"kind": "scanning", "value": true}})
        );
    }
}
