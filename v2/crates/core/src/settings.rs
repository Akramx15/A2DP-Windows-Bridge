//! settings.json, read and written in v1's format. Keys v2 does not know are
//! kept (written back as they were), so going back to v1 loses nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use bt_host::media::{MEDIA_PAYLOAD_LIMIT_DEFAULT, clamp_media_payload_limit};

pub const SCHEMA_VERSION: u32 = 2;

fn default_language() -> String {
    "en".into()
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
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
    /// Also offer releases marked as pre-release on GitHub (beta channel)
    pub include_prereleases: bool,

    /// The headphones' buttons act as Windows media keys
    #[serde(default = "default_true")]
    pub media_keys: bool,
    /// Remember the headphones' volume per device (address -> AVRCP volume
    /// 0..127) and set it again when they connect
    pub remember_volume: bool,
    pub saved_volumes: BTreeMap<String, i32>,

    pub debug_mode: bool,

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

    /// Before v1.1 the max media packet size was a setting; it now seeds
    /// profiles saved without one. Read, not written.
    #[serde(skip_serializing)]
    pub max_media_payload: Option<i64>,

    /// Keys of other versions, kept as they are
    #[serde(flatten)]
    pub other: BTreeMap<String, serde_json::Value>,
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
            include_prereleases: false,
            media_keys: true,
            remember_volume: false,
            saved_volumes: BTreeMap::new(),
            debug_mode: false,
            bt_chip_pid: 0,
            bt_chip_fw_stem: String::new(),
            afh: default_afh(),
            remote_sink: String::new(),
            last_profile: String::new(),
            max_media_payload: None,
            other: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }

    /// Missing or unreadable: defaults (a broken file is kept as .bad)
    pub fn load(dir: &Path) -> Settings {
        let path = Settings::path(dir);
        let Ok(text) = std::fs::read_to_string(&path) else { return Settings::default() };
        match serde_json::from_str::<Settings>(&text) {
            Ok(mut s) => {
                s.saved_volumes.retain(|_, v| (0..=127).contains(v));
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
        std::fs::rename(&tmp, &path)
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
    fn v1_file_round_trips_unknown_keys() {
        let v1 = r#"{"language":"ja","theme":"light","media_keys":false,"saved_volumes":{"AA:BB:CC:DD:EE:FF":64,"X":999},
                    "le_audio_experimental":true,"max_media_payload":1500,"window_x":10}"#;
        let s: Settings = serde_json::from_str(v1).unwrap();
        assert_eq!(s.language, "ja");
        assert!(!s.media_keys);
        assert!(s.minimize_to_tray);
        assert_eq!(s.legacy_max_media_payload(), 1500);
        assert_eq!(s.other.get("le_audio_experimental"), Some(&serde_json::Value::Bool(true)));
        let out = serde_json::to_value(&s).unwrap();
        assert_eq!(out["window_x"], 10);
        assert_eq!(out["schema_version"], SCHEMA_VERSION);
        assert!(out.get("max_media_payload").is_none());
    }
}
