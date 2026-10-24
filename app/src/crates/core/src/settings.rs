//! settings.json, read and written in v1's format. Keys v2 does not know are
//! kept (written back as they were), so going back to v1 loses nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use bt_host::media::{MEDIA_PAYLOAD_LIMIT_DEFAULT, clamp_media_payload_limit};

/// settings.json's format: 1 is v1's (which wrote no schema_version), 2 is
/// v2's, 3 has LE Audio on by default, 4 remembers each device's volume by
/// default, 5 has one Auto-select for every device (see migrate.rs)
pub const SCHEMA_VERSION: u32 = 5;
/// The first schema_version v2 wrote: a lower one is v1's folder
pub const FIRST_V2_SCHEMA: u32 = 2;

fn v1_schema() -> u32 {
    1
}
/// The languages of app/lang
pub const LANGUAGES: [&str; 8] = ["en", "ja", "de", "fr", "es", "ko", "zh-CN", "zh-TW"];

/// One of LANGUAGES for a BCP 47 name ("ja-JP", "zh-Hant-HK", "pt-BR"):
/// None when the app has no strings for it
pub fn app_language_for(tag: &str) -> Option<&'static str> {
    let tag = tag.trim().to_ascii_lowercase().replace('_', "-");
    let mut parts = tag.split('-');
    let primary = parts.next().unwrap_or_default();
    if primary == "zh" {
        // Traditional script, or a region that writes it, unless the script
        // says otherwise
        let rest: Vec<&str> = parts.collect();
        let traditional = if rest.contains(&"hant") {
            true
        } else if rest.contains(&"hans") {
            false
        } else {
            rest.iter().any(|r| matches!(*r, "tw" | "hk" | "mo"))
        };
        return Some(if traditional { "zh-TW" } else { "zh-CN" });
    }
    LANGUAGES.iter().copied().find(|l| *l == primary)
}

/// A new settings file's language: the one chosen in the installer, else
/// the first of the user's Windows display languages the app has, else
/// English (v1's installer wrote the setup's language into settings.json;
/// the display languages also cover the portable copy)
fn default_language() -> String {
    let mut tags: Vec<String> = win_sys::locale::setup_language().into_iter().collect();
    tags.extend(win_sys::locale::preferred_ui_languages());
    pick_language(&tags).into()
}

fn pick_language(tags: &[String]) -> &'static str {
    tags.iter().find_map(|t| app_language_for(t)).unwrap_or("en")
}
fn default_theme() -> String {
    "dark".into()
}
fn default_true() -> bool {
    true
}
fn default_afh() -> String {
    "auto".into()
}
/// A pre-release build ("2.0.2-beta.5") offers pre-releases by default: its
/// user is on the beta channel already
fn default_prereleases() -> bool {
    is_prerelease(env!("CARGO_PKG_VERSION"))
}
fn is_prerelease(version: &str) -> bool {
    version.contains('-')
}

/// Per-device overrides of the global options; None follows the global
/// value
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DeviceOptions {
    /// The headphones' buttons act as Windows media keys
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_keys: Option<bool>,
    /// Remember the headphones' volume and set it again when they connect
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remember_volume: Option<bool>,
}

impl DeviceOptions {
    /// No override set
    pub fn is_empty(&self) -> bool {
        self.media_keys.is_none() && self.remember_volume.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Missing in v1's files
    #[serde(default = "v1_schema")]
    pub schema_version: u32,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_theme")]
    pub theme: String,

    pub start_with_windows: bool,
    pub start_minimized: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,

    #[serde(default = "default_true")]
    pub check_updates_on_startup: bool,
    /// ISO date of the last automatic check
    pub last_update_check: String,
    /// Also offer releases marked as pre-release on GitHub (beta channel);
    /// on by default in a pre-release build
    #[serde(default = "default_prereleases")]
    pub include_prereleases: bool,

    /// The headphones' buttons act as Windows media keys
    #[serde(default = "default_true")]
    pub media_keys: bool,
    /// Remember the headphones' volume per device (address -> AVRCP volume
    /// 0..127) and set it again when they connect; on by default (issue
    /// #86: one device's volume must not carry over to another)
    #[serde(default = "default_true")]
    pub remember_volume: bool,
    pub saved_volumes: BTreeMap<String, i32>,
    /// Per-device overrides of media_keys and remember_volume (address in
    /// capitals "AA:BB:..")
    pub device_options: BTreeMap<String, DeviceOptions>,

    pub debug_mode: bool,
    /// Developer mode: overrides of the codec and media path (in force only
    /// while debug_mode is on)
    pub dev_overrides: codec::dev::DevOverrides,
    /// Developer mode: how the core logs (in force only while debug_mode is
    /// on; otherwise [`DevLog::default`])
    pub dev_log: DevLog,

    /// Realtek chip for the firmware download: 0 = auto; a Realtek PID for
    /// OEM adapters whose VID is not Realtek's
    pub bt_chip_pid: u16,
    /// Firmware stem for chips not in the chip table (e.g. "rtl8761bu")
    pub bt_chip_fw_stem: String,

    /// AFH host channel classification: "auto", "off" or Wi-Fi channels "6,11"
    #[serde(default = "default_afh")]
    pub afh: String,

    /// Peer receiver test: measuring receiver, host[:port] or "auto"
    pub remote_sink: String,

    pub last_profile: String,

    /// LE Audio (experimental): BTstack starts with LE (SM, GATT, ISO) and
    /// LE Audio profiles can stream. On by default since 2.0.1 (schema 3):
    /// choosing an LE Audio device is the user's call.
    pub le_audio_experimental: bool,
    /// Before 2.0.1: GMAP low latency for every LE Audio profile. Now a
    /// profile option (`le_low_latency`); kept only so the file round-trips.
    pub le_audio_low_latency: bool,

    /// Codecs > "Enable experimental codecs" (on by default, issue #86):
    /// off, the experimental codecs ([`codec::CodecKind::experimental`])
    /// are hidden there and never used, by Auto or as a fallback
    pub experimental_codecs: bool,

    /// Codecs > "Auto-select the codec with the highest quality", for every
    /// device (issue #86; each device had its own before schema 5): on,
    /// every profile streams as Auto with Auto's defaults; off, each one
    /// with the codec chosen for it (a profile with none chosen, "auto",
    /// still as Auto). LE Audio profiles (LC3) are not affected.
    pub auto_codec: bool,

    /// Codecs > Auto's order (issue #86): codec keys, what Auto picks first
    /// and the way a chosen codec falls back. Empty: highest sound quality
    /// first ([`codec::CodecKind::AUTO_PRIORITY`]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub auto_order: Vec<String>,

    /// Before v1.1 the max media packet size was a setting; it now seeds
    /// profiles saved without one. Read, not written.
    #[serde(skip_serializing)]
    pub max_media_payload: Option<i64>,

    /// Keys of other versions, kept as they are
    #[serde(flatten)]
    pub other: BTreeMap<String, serde_json::Value>,
}

/// Daily log files kept by default
pub const LOG_KEEP_DAYS: u32 = 7;
/// What a day's log file takes by default before its debug and trace lines
/// stop
pub const LOG_DAY_CAP_MB: u32 = 100;

/// Developer mode's logging: retention, the daily size cap, verbose logging
/// that does not run out, and the stream health line every interval
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DevLog {
    /// Daily log files kept (1..=365)
    pub keep_days: u32,
    /// MB a day's file takes before its debug and trace lines stop
    /// (10..=10240)
    pub day_cap_mb: u32,
    /// Verbose logging that stays on (no 24 h / 1 h limit) and covers the
    /// start-up: "debug", "trace", or "" (off)
    pub always_verbose: String,
    /// The stream health line every 10 s for the whole stream, not only in
    /// its first minute (issue #84: comparing sessions)
    pub health_every_interval: bool,
}

impl Default for DevLog {
    fn default() -> Self {
        DevLog {
            keep_days: LOG_KEEP_DAYS,
            day_cap_mb: LOG_DAY_CAP_MB,
            always_verbose: String::new(),
            health_every_interval: false,
        }
    }
}

impl DevLog {
    /// Values in range; an unknown level is off
    pub fn clamped(&self) -> DevLog {
        DevLog {
            keep_days: self.keep_days.clamp(1, 365),
            day_cap_mb: self.day_cap_mb.clamp(10, 10240),
            always_verbose: match self.always_verbose.as_str() {
                "debug" | "trace" => self.always_verbose.clone(),
                _ => String::new(),
            },
            health_every_interval: self.health_every_interval,
        }
    }
}

/// "dark", "light" or "system"; anything else is dark, as v1 read it
pub fn normalize_theme(theme: &str) -> &'static str {
    match theme {
        "light" => "light",
        "system" => "system",
        _ => "dark",
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema_version: SCHEMA_VERSION,
            language: default_language(),
            theme: default_theme(),
            start_with_windows: false,
            start_minimized: false,
            minimize_to_tray: true,
            check_updates_on_startup: true,
            last_update_check: String::new(),
            include_prereleases: default_prereleases(),
            media_keys: true,
            remember_volume: true,
            saved_volumes: BTreeMap::new(),
            device_options: BTreeMap::new(),
            debug_mode: false,
            dev_overrides: codec::dev::DevOverrides::default(),
            dev_log: DevLog::default(),
            bt_chip_pid: 0,
            bt_chip_fw_stem: String::new(),
            afh: default_afh(),
            remote_sink: String::new(),
            last_profile: String::new(),
            le_audio_experimental: true,
            le_audio_low_latency: false,
            experimental_codecs: true,
            auto_codec: true,
            auto_order: Vec::new(),
            max_media_payload: None,
            other: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Auto's order as streams use it
    pub fn auto_order(&self) -> codec::AutoOrder {
        codec::AutoOrder::from_keys(&self.auto_order)
    }

    /// The logging in force: developer mode's, else the defaults
    pub fn log(&self) -> DevLog {
        if self.debug_mode { self.dev_log.clamped() } else { DevLog::default() }
    }

    /// A device's override, if one (any case of the address)
    fn device(&self, addr: &str) -> Option<&DeviceOptions> {
        self.device_options.get(&addr.to_uppercase())
    }

    /// Whether a device's buttons act as media keys: its override, else the
    /// global media_keys
    pub fn media_keys_for(&self, addr: &str) -> bool {
        self.device(addr).and_then(|o| o.media_keys).unwrap_or(self.media_keys)
    }

    /// Whether a device's volume is remembered: its override, else the
    /// global remember_volume
    pub fn remember_volume_for(&self, addr: &str) -> bool {
        self.device(addr).and_then(|o| o.remember_volume).unwrap_or(self.remember_volume)
    }

    pub fn path(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }

    /// Missing or unreadable: defaults (a broken file is kept as .bad)
    pub fn load(dir: &Path) -> Settings {
        let path = Settings::path(dir);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                tracing::debug!("settings: {} not read ({e}), using defaults", path.display());
                return Settings::default();
            }
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(mut s) => {
                let volumes = s.saved_volumes.len();
                s.saved_volumes.retain(|_, v| (0..=127).contains(v));
                if s.saved_volumes.len() != volumes {
                    tracing::debug!(
                        "settings: dropped {} saved volume(s) out of range",
                        volumes - s.saved_volumes.len()
                    );
                }
                s.theme = normalize_theme(&s.theme).into();
                s.device_options = std::mem::take(&mut s.device_options)
                    .into_iter()
                    .filter(|(_, o)| !o.is_empty())
                    .map(|(a, o)| (a.to_uppercase(), o))
                    .collect();
                tracing::debug!(
                    "settings: loaded {} (schema {}, {} unknown key(s))",
                    path.display(),
                    s.schema_version,
                    s.other.len()
                );
                tracing::trace!("settings: {s:?}");
                s
            }
            Err(e) => {
                tracing::warn!("settings: {} is not valid ({e}), using defaults", path.display());
                let _ = std::fs::copy(&path, path.with_extension("json.bad"));
                Settings::default()
            }
        }
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let mut s = self.clone();
        s.schema_version = SCHEMA_VERSION;
        let text = serde_json::to_string_pretty(&s).map_err(std::io::Error::other)?;
        let path = Settings::path(dir);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        let r = std::fs::rename(&tmp, &path);
        tracing::debug!("settings: saved {}: {r:?}", path.display());
        r
    }

    /// The media packet size limit of a settings file from before v1.1, for
    /// profiles saved without one
    pub fn legacy_max_media_payload(&self) -> u16 {
        self.max_media_payload
            .map(|v| clamp_media_payload_limit(v.clamp(0, u32::MAX as i64) as u32))
            .unwrap_or(MEDIA_PAYLOAD_LIMIT_DEFAULT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_from_the_windows_display_languages() {
        assert_eq!(app_language_for("ja-JP"), Some("ja"));
        assert_eq!(app_language_for("en-GB"), Some("en"));
        assert_eq!(app_language_for("de"), Some("de"));
        assert_eq!(app_language_for("fr-CA"), Some("fr"));
        assert_eq!(app_language_for("es-419"), Some("es"));
        assert_eq!(app_language_for("ko-KR"), Some("ko"));
        assert_eq!(app_language_for("zh-CN"), Some("zh-CN"));
        assert_eq!(app_language_for("zh-SG"), Some("zh-CN"));
        assert_eq!(app_language_for("zh-Hans-HK"), Some("zh-CN"));
        assert_eq!(app_language_for("zh-TW"), Some("zh-TW"));
        assert_eq!(app_language_for("zh-HK"), Some("zh-TW"));
        assert_eq!(app_language_for("zh-Hant-TW"), Some("zh-TW"));
        assert_eq!(app_language_for("pt-BR"), None);
        assert_eq!(app_language_for(""), None);
        let tags = |t: &[&str]| t.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(pick_language(&tags(&["pt-BR", "fr-FR", "en-US"])), "fr");
        assert_eq!(pick_language(&tags(&["pt-BR"])), "en");
        assert_eq!(pick_language(&[]), "en");
        // A file without the key gets one of them
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(LANGUAGES.contains(&s.language.as_str()));
    }

    #[test]
    fn dev_log_counts_only_in_developer_mode() {
        let mut s: Settings = serde_json::from_str(r#"{"dev_log":{"keep_days":0,"always_verbose":"loud"}}"#).unwrap();
        assert_eq!(s.dev_log.day_cap_mb, LOG_DAY_CAP_MB);
        assert_eq!(s.log(), DevLog::default());
        s.debug_mode = true;
        let l = s.log();
        assert_eq!((l.keep_days, l.always_verbose.as_str()), (1, ""));
        s.dev_log.always_verbose = "trace".into();
        s.dev_log.day_cap_mb = 1 << 20;
        assert_eq!((s.log().always_verbose.as_str(), s.log().day_cap_mb), ("trace", 10240));
    }

    #[test]
    fn v1_file_round_trips_unknown_keys() {
        let v1 = r#"{"language":"ja","theme":"light","media_keys":false,"saved_volumes":{"AA:BB:CC:DD:EE:FF":64,"X":999},
                    "le_audio_experimental":true,"max_media_payload":1500,"window_x":10}"#;
        let s: Settings = serde_json::from_str(v1).unwrap();
        assert_eq!(s.language, "ja");
        assert!(!s.media_keys);
        assert!(s.minimize_to_tray);
        assert_eq!(s.legacy_max_media_payload(), 1500);
        assert!(s.le_audio_experimental);
        assert!(!s.le_audio_low_latency);
        assert!(!s.other.contains_key("le_audio_experimental"));
        let out = serde_json::to_value(&s).unwrap();
        assert_eq!(out["window_x"], 10);
        // v1 wrote both LE Audio keys
        assert_eq!(out["le_audio_experimental"], true);
        assert_eq!(out["le_audio_low_latency"], false);
        // v1 wrote no schema; save() writes the current one (migrate.rs)
        assert_eq!(out["schema_version"], 1);
        assert!(out.get("max_media_payload").is_none());
    }

    #[test]
    fn device_options_override_the_global_values() {
        let mut s: Settings = serde_json::from_str(
            r#"{"media_keys":true,"remember_volume":false,
                "device_options":{"AA:BB:CC:DD:EE:FF":{"media_keys":false},"11:22:33:44:55:66":{"remember_volume":true}}}"#,
        )
        .unwrap();
        assert!(!s.media_keys_for("aa:bb:cc:dd:ee:ff"));
        assert!(!s.remember_volume_for("AA:BB:CC:DD:EE:FF"));
        assert!(s.media_keys_for("11:22:33:44:55:66"));
        assert!(s.remember_volume_for("11:22:33:44:55:66"));
        // No override: the global values
        assert!(s.media_keys_for("00:00:00:00:00:01"));
        assert!(!s.remember_volume_for("00:00:00:00:00:01"));
        s.media_keys = false;
        s.remember_volume = true;
        assert!(!s.media_keys_for("00:00:00:00:00:01"));
        assert!(s.remember_volume_for("00:00:00:00:00:01"));
        // Only its media keys are its own: the volume follows the global value
        assert!(s.remember_volume_for("aa:bb:cc:dd:ee:ff"));
        assert!(!s.media_keys_for("11:22:33:44:55:66"));
    }

    #[test]
    fn device_options_round_trip() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(s.device_options.is_empty());
        let mut s = Settings::default();
        s.device_options
            .insert("AA:BB:CC:DD:EE:FF".into(), DeviceOptions { media_keys: Some(false), remember_volume: None });
        let out = serde_json::to_value(&s).unwrap();
        assert_eq!(out["device_options"], serde_json::json!({"AA:BB:CC:DD:EE:FF": {"media_keys": false}}));
        let back: Settings = serde_json::from_value(out).unwrap();
        assert_eq!(back, s);

        // load(): keys in capitals, empty overrides dropped
        let dir = std::env::temp_dir().join(format!("a2dpwb-devopt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            Settings::path(&dir),
            r#"{"device_options":{"aa:bb:cc:dd:ee:ff":{"remember_volume":true},"11:22:33:44:55:66":{}}}"#,
        )
        .unwrap();
        let loaded = Settings::load(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(loaded.device_options.len(), 1);
        assert_eq!(loaded.device_options["AA:BB:CC:DD:EE:FF"].remember_volume, Some(true));
    }

    #[test]
    fn prereleases_by_default_in_a_prerelease_build() {
        assert!(is_prerelease("2.0.2-beta.5"));
        assert!(!is_prerelease("2.0.2"));
        let expected = env!("CARGO_PKG_VERSION").contains('-');
        assert_eq!(Settings::default().include_prereleases, expected);
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.include_prereleases, expected);
        // A saved choice stays
        let s: Settings = serde_json::from_str(r#"{"include_prereleases":false}"#).unwrap();
        assert!(!s.include_prereleases);
    }

    #[test]
    fn unknown_theme_is_dark_as_in_v1() {
        let dir = std::env::temp_dir().join(format!("a2dpwb-theme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (v1, v2) in [("white", "dark"), ("light", "light"), ("system", "system"), ("", "dark")] {
            std::fs::write(Settings::path(&dir), format!(r#"{{"theme":"{v1}"}}"#)).unwrap();
            assert_eq!(Settings::load(&dir).theme, v2, "{v1}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
