//! profiles.json in v1's format: {"profiles": [ ... ]}. Unknown keys of a
//! profile are kept.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use bt_host::media::clamp_media_payload_limit;
use codec::{CodecKind, Quality};

use crate::stream::{SourceRequest, StreamRequest, VolumeSync};

fn auto() -> String {
    "auto".into()
}
fn hq() -> String {
    "hq".into()
}
fn loopback() -> String {
    "loopback".into()
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    /// XX:XX:XX:XX:XX:XX
    pub device_address: String,
    pub device_name: String,
    /// auto, ldac, aptxhd, aptxll, aptx, sbcxq, sbc, aac, lhdcv5, opus, faststream
    #[serde(default = "auto")]
    pub codec: String,
    /// hq, sq, mq; mhq for LHDC V5 only
    #[serde(default = "hq")]
    pub quality: String,
    pub abr: bool,
    /// Use the best codec the device offers when the profile's is missing
    #[serde(default = "yes")]
    pub codec_fallback: bool,
    /// 0 = auto
    pub sample_rate: u32,
    /// 0 = auto, 16, 24
    pub bit_depth: u32,
    /// loopback, virtual, app
    #[serde(default = "loopback")]
    pub capture_mode: String,
    pub audio_device_id: String,
    pub audio_device_name: String,
    /// Virtual device mode: make the device the default output while
    /// streaming
    #[serde(default = "yes")]
    pub auto_switch_device: bool,
    pub app_exe: String,
    pub app_name: String,
    /// auto, off, device
    #[serde(default = "auto")]
    pub volume_sync: String,
    pub volume_device_id: String,
    pub volume_device_name: String,
    /// Max media packet size (None in files from before v1.1: the legacy
    /// setting applies)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_media_payload: Option<u16>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub le_audio: bool,
    #[serde(flatten)]
    pub other: BTreeMap<String, serde_json::Value>,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            name: String::new(),
            device_address: String::new(),
            device_name: String::new(),
            codec: auto(),
            quality: hq(),
            abr: false,
            codec_fallback: true,
            sample_rate: 0,
            bit_depth: 0,
            capture_mode: loopback(),
            audio_device_id: String::new(),
            audio_device_name: String::new(),
            auto_switch_device: true,
            app_exe: String::new(),
            app_name: String::new(),
            volume_sync: auto(),
            volume_device_id: String::new(),
            volume_device_name: String::new(),
            max_media_payload: None,
            le_audio: false,
            other: BTreeMap::new(),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileError {
    #[error("invalid Bluetooth address \"{0}\"")]
    Address(String),
}

impl Profile {
    /// What to stream for this profile. An LE Audio (experimental) profile
    /// gives an LE request (`le` Some: the le_stream session, which refuses
    /// with error.le_audio_disabled while the setting is off); its codec,
    /// quality, rate and depth are not used (LC3 as the device and the
    /// preset decide).
    pub fn to_request(&self, legacy_max_media_payload: u16) -> Result<StreamRequest, ProfileError> {
        let device = bt_host::parse_addr(&self.device_address)
            .ok_or_else(|| ProfileError::Address(self.device_address.clone()))?;
        let source = match self.capture_mode.as_str() {
            "virtual" => SourceRequest::VirtualDevice {
                id: self.audio_device_id.clone(),
                name: self.audio_device_name.clone(),
                auto_switch: self.auto_switch_device,
            },
            "app" => SourceRequest::Application { exe: self.app_exe.clone() },
            // "loopback", and v1's legacy "apo"
            _ => SourceRequest::SystemLoopback,
        };
        let volume_sync = match self.volume_sync.as_str() {
            "off" => VolumeSync::Off,
            "device" => VolumeSync::Device(self.volume_device_id.clone()),
            _ => VolumeSync::Auto,
        };
        Ok(StreamRequest {
            device,
            device_name: self.device_name.clone(),
            codec: CodecKind::from_key(&self.codec),
            codec_fallback: self.codec_fallback,
            quality: Quality::from_key(&self.quality),
            abr: self.abr,
            sample_rate: (self.sample_rate != 0).then_some(self.sample_rate),
            bit_depth: match self.bit_depth {
                16 => Some(16),
                24 => Some(24),
                _ => None,
            },
            source,
            volume_sync,
            max_media_payload: clamp_media_payload_limit(
                self.max_media_payload.unwrap_or(legacy_max_media_payload) as u32
            ),
            // The low-latency setting is read by the session (GMAP presets)
            le: self.le_audio.then(bt_host::LeRequest::default),
        })
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProfilesFile {
    #[serde(default)]
    profiles: Vec<Profile>,
    #[serde(flatten)]
    other: BTreeMap<String, serde_json::Value>,
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join("profiles.json")
}

pub fn load(dir: &Path) -> Vec<Profile> {
    let Ok(text) = std::fs::read_to_string(path(dir)) else { return Vec::new() };
    match serde_json::from_str::<ProfilesFile>(&text) {
        Ok(f) => f.profiles,
        Err(e) => {
            tracing::warn!("profiles: {} is not valid ({e})", path(dir).display());
            Vec::new()
        }
    }
}

pub fn save(dir: &Path, profiles: &[Profile]) -> std::io::Result<()> {
    let file = ProfilesFile { profiles: profiles.to_vec(), other: BTreeMap::new() };
    let text = serde_json::to_string_pretty(&file).map_err(std::io::Error::other)?;
    let tmp = path(dir).with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_profile() {
        let text = r#"{"profiles":[{"name":"WH","device_address":"00:A2:D0:00:00:02","codec":"ldac","quality":"sq",
            "bit_depth":24,"capture_mode":"virtual","audio_device_id":"{x}","max_media_payload":1023,"extra":1}]}"#;
        let f: ProfilesFile = serde_json::from_str(text).unwrap();
        let p = &f.profiles[0];
        assert_eq!(p.other.get("extra"), Some(&serde_json::json!(1)));
        let r = p.to_request(679).unwrap();
        assert_eq!(r.codec, Some(CodecKind::Ldac));
        assert_eq!(r.quality, Quality::Standard);
        assert_eq!(r.bit_depth, Some(24));
        assert_eq!(r.max_media_payload, 1023);
        assert!(matches!(r.source, SourceRequest::VirtualDevice { auto_switch: true, .. }));
        let mut legacy = p.clone();
        legacy.max_media_payload = None;
        assert_eq!(legacy.to_request(1500).unwrap().max_media_payload, 1500);
        assert_eq!(r.le, None);
    }

    #[test]
    fn le_audio_profile_gives_an_le_request() {
        let text = r#"{"profiles":[{"name":"Buds LE Audio","device_address":"00:a2:d0:00:00:12","device_name":"Buds",
            "capture_mode":"loopback","le_audio":true}]}"#;
        let f: ProfilesFile = serde_json::from_str(text).unwrap();
        let p = &f.profiles[0];
        assert!(p.le_audio);
        let r = p.to_request(1023).unwrap();
        assert_eq!(r.device, [0x00, 0xA2, 0xD0, 0x00, 0x00, 0x12]);
        assert_eq!(r.le, Some(bt_host::LeRequest { extra_devices: Vec::new(), preset: None, low_latency: false }));
        assert_eq!(r.source, SourceRequest::SystemLoopback);
        // Written back as v1 wrote it; a classic profile has no key
        let out = serde_json::to_value(p).unwrap();
        assert_eq!(out["le_audio"], true);
        let classic = Profile { le_audio: false, ..p.clone() };
        assert!(serde_json::to_value(&classic).unwrap().get("le_audio").is_none());
        // A bad address is refused as for A2DP
        let bad = Profile { device_address: "nope".into(), ..p.clone() };
        assert_eq!(bad.to_request(1023), Err(ProfileError::Address("nope".into())));
    }
}
