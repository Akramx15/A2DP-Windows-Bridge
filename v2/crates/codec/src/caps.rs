//! Codec information (A2DP "Media Codec Specific Information Elements"):
//! the local stream endpoints, the remote's capabilities, the configuration
//! sent with SET_CONFIGURATION, and the codec choice.
//!
//! Vendor codec information here is what BTstack calls the media codec
//! information: vendor ID (4 bytes LE), codec ID (2 bytes LE), then the
//! codec-specific bytes. The values follow v1 (btstack_transport.cpp), which
//! follows Android and PipeWire.

use crate::CodecKind;

pub const LDAC_VENDOR_ID: u32 = 0x0000_012D; // Sony
pub const LDAC_CODEC_ID: u16 = 0x00AA;
pub const APTXHD_VENDOR_ID: u32 = 0x0000_00D7; // Qualcomm
pub const APTXHD_CODEC_ID: u16 = 0x0024;
pub const APTXLL_VENDOR_ID: u32 = 0x0000_000A; // CSR
pub const APTXLL_VENDOR_ID2: u32 = 0x0000_00D7; // some sinks list aptX LL under Qualcomm
pub const APTXLL_CODEC_ID: u16 = 0x0002;
pub const APTX_VENDOR_ID: u32 = 0x0000_004F; // APT
pub const APTX_CODEC_ID: u16 = 0x0001;
/// aptX Adaptive: no open-source encoder; detected for messages only
pub const APTXAD_VENDOR_ID: u32 = 0x0000_00D7;
pub const APTXAD_CODEC_ID: u16 = 0x00AD;
pub const LHDCV5_VENDOR_ID: u32 = 0x0000_053A; // Savitech
pub const LHDCV5_CODEC_ID: u16 = 0x4C35;

/// AVDTP media codec types
pub const CODEC_TYPE_SBC: u8 = 0x00;
pub const CODEC_TYPE_AAC: u8 = 0x02;
pub const CODEC_TYPE_VENDOR: u8 = 0xFF;

// LDAC byte 6: 0x20 44.1k, 0x10 48k, 0x08 88.2k, 0x04 96k; byte 7: 0x04 mono, 0x02 dual, 0x01 stereo
const LDAC_FREQ_ALL: u8 = 0x3C;
const LDAC_CH_ALL: u8 = 0x07;

// aptX family byte 6: high nibble frequency (0x20 44.1k, 0x10 48k), low nibble channels (0x02 stereo, 0x01 mono)
const APTX_FREQ_44100: u8 = 0x20;
const APTX_FREQ_48000: u8 = 0x10;
const APTX_FREQ_MASK: u8 = 0xF0;
const APTX_CH_STEREO: u8 = 0x02;
const APTX_CAPS_ALL: u8 = APTX_FREQ_44100 | APTX_FREQ_48000 | APTX_CH_STEREO;
const APTX_CONFIG_DEFAULT: u8 = APTX_FREQ_48000 | APTX_CH_STEREO;

/// aptX HD: classic aptX 7 bytes + 4 reserved
const APTXHD_INFO_LEN: usize = 11;
/// aptX LL: classic 7 bytes + flags (bit0 bidirect_link, bit1 has_new_caps);
/// with has_new_caps the sink appends 9 bytes
const APTXLL_INFO_LEN: usize = 8;
const APTXLL_EXT_INFO_LEN: usize = 17;
const APTXLL_HAS_NEW_CAPS: u8 = 0x02;
/// PipeWire's defaults for the extended LL configuration, raised by 3/2
const APTXLL_TARGET_LEVEL: u16 = 180 * 3 / 2;
const APTXLL_INITIAL_LEVEL: u16 = 360 * 3 / 2;
const APTXLL_GOOD_LEVEL: u16 = 180 * 3 / 2;
const APTXLL_SRA_MAX_RATE: u8 = 50;
const APTXLL_SRA_AVG_TIME: u8 = 1;

// LHDC V5 (Android a2dp_vendor_lhdcv5_constants.h): 11 bytes
//   P6 rates 0x20 44.1k 0x10 48k 0x04 96k 0x01 192k
//   P7 bit depth 0x04 16 0x02 24, bits 7..4 bit rate limits
//   P8 version bitmap (0x01 V5 ver.1), 0x10 5 ms frames
const LHDCV5_INFO_LEN: usize = 11;
pub const LHDCV5_SR_44100: u8 = 0x20;
pub const LHDCV5_SR_48000: u8 = 0x10;
pub const LHDCV5_SR_96000: u8 = 0x04;
pub const LHDCV5_SR_192000: u8 = 0x01;
const LHDCV5_SR_MASK: u8 = 0x35;
const LHDCV5_BITS_16: u8 = 0x04;
const LHDCV5_BITS_24: u8 = 0x02;
const LHDCV5_BITS_MASK: u8 = 0x07;
const LHDCV5_BITRATE_LIMITS_MASK: u8 = 0xF0;
const LHDCV5_VERSION_1: u8 = 0x01;
const LHDCV5_VERSION_MASK: u8 = 0x0F;
const LHDCV5_FRAME_5MS: u8 = 0x10;
const LHDCV5_CAPS_P8: u8 = LHDCV5_VERSION_1 | LHDCV5_FRAME_5MS;

fn vendor_header(vendor_id: u32, codec_id: u16) -> Vec<u8> {
    let mut v = vendor_id.to_le_bytes().to_vec();
    v.extend_from_slice(&codec_id.to_le_bytes());
    v
}

fn read_vendor(info: &[u8]) -> Option<(u32, u16)> {
    if info.len() < 6 {
        return None;
    }
    Some((u32::from_le_bytes([info[0], info[1], info[2], info[3]]), u16::from_le_bytes([info[4], info[5]])))
}

/// A local stream endpoint: what we register with BTstack
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEndpoint {
    pub kind: CodecKind,
    /// AVDTP media codec type ([`CODEC_TYPE_SBC`], ...)
    pub codec_type: u8,
    /// Capabilities we offer
    pub caps: Vec<u8>,
    /// Default configuration (BTstack's initial value; the real one goes
    /// with SET_CONFIGURATION)
    pub config: Vec<u8>,
}

/// The endpoints of the built-in codecs, in v1's order (endpoint slots)
pub fn local_endpoints() -> Vec<LocalEndpoint> {
    let mut eps = Vec::new();
    let mut vendor = |kind: CodecKind, vid: u32, cid: u16, len: usize, caps_tail: &[u8], config_tail: &[u8]| {
        let mut caps = vendor_header(vid, cid);
        caps.extend_from_slice(caps_tail);
        caps.resize(len, 0);
        let mut config = vendor_header(vid, cid);
        config.extend_from_slice(config_tail);
        config.resize(len, 0);
        LocalEndpoint { kind, codec_type: CODEC_TYPE_VENDOR, caps, config }
    };
    if CodecKind::Ldac.available() {
        eps.push(vendor(CodecKind::Ldac, LDAC_VENDOR_ID, LDAC_CODEC_ID, 8, &[LDAC_FREQ_ALL, LDAC_CH_ALL], &[0x10, 0x01]));
    }
    if CodecKind::AptxHd.available() {
        eps.push(vendor(
            CodecKind::AptxHd,
            APTXHD_VENDOR_ID,
            APTXHD_CODEC_ID,
            APTXHD_INFO_LEN,
            &[APTX_CAPS_ALL],
            &[APTX_CONFIG_DEFAULT],
        ));
        eps.push(vendor(CodecKind::Aptx, APTX_VENDOR_ID, APTX_CODEC_ID, 7, &[APTX_CAPS_ALL], &[APTX_CONFIG_DEFAULT]));
        // One local SEP for sinks listing aptX LL under either vendor ID; the
        // configuration carries the remote's
        eps.push(vendor(
            CodecKind::AptxLl,
            APTXLL_VENDOR_ID,
            APTXLL_CODEC_ID,
            APTXLL_INFO_LEN,
            &[APTX_CAPS_ALL],
            &[APTX_CONFIG_DEFAULT],
        ));
    }
    // SBC: 44.1+48k, all channel modes, 16 blocks, 8 subbands, loudness, bitpool 2..53
    eps.push(LocalEndpoint {
        kind: CodecKind::Sbc,
        codec_type: CODEC_TYPE_SBC,
        caps: vec![0x3F, 0x15, 2, 53],
        config: vec![0x11, 0x15, 2, 53],
    });
    if CodecKind::Aac.available() {
        // MPEG-2 AAC-LC, 44.1+48k, 2 channels, 256 kbps
        eps.push(LocalEndpoint {
            kind: CodecKind::Aac,
            codec_type: CODEC_TYPE_AAC,
            caps: vec![0x80, 0x0C, 0x04, 0x03, 0xE8, 0x00],
            config: vec![0x80, 0x04, 0x04, 0x03, 0xE8, 0x00],
        });
    }
    if CodecKind::LhdcV5.available() {
        eps.push(vendor(
            CodecKind::LhdcV5,
            LHDCV5_VENDOR_ID,
            LHDCV5_CODEC_ID,
            LHDCV5_INFO_LEN,
            &[LHDCV5_SR_MASK, LHDCV5_BITS_16 | LHDCV5_BITS_24, LHDCV5_CAPS_P8],
            &[LHDCV5_SR_48000, LHDCV5_BITS_24, LHDCV5_CAPS_P8],
        ));
    }
    eps
}

/// A codec the remote offers on one of its SEPs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RemoteSep {
    pub seid: u8,
}

/// What the remote reported during capability discovery
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteCaps {
    pub sbc: Option<RemoteSep>,
    pub aac: Option<RemoteSep>,
    pub ldac: Option<RemoteSep>,
    pub aptx: Option<RemoteSep>,
    pub aptx_hd: Option<RemoteSep>,
    pub aptx_ll: Option<RemoteSep>,
    pub lhdcv5: Option<RemoteSep>,
    /// aptX family byte 6 of each (0 = not reported: assume 44.1k + 48k)
    pub aptx_caps: u8,
    pub aptxhd_caps: u8,
    pub aptxll_caps: u8,
    /// aptX LL: the vendor ID the remote used and its codec information
    pub aptxll_vendor_id: u32,
    pub aptxll_info: Vec<u8>,
    /// aptX Adaptive offered (never selected: no open encoder)
    pub aptx_adaptive: bool,
    /// LHDC V5 codec information (11 bytes) when usable
    pub lhdcv5_info: [u8; LHDCV5_INFO_LEN],
    /// SEP discovery: audio sink SEPs, and how many of them were in use (a
    /// codec streaming to another device is missing from the above)
    pub audio_sinks: u8,
    pub audio_sinks_in_use: u8,
}

/// How a vendor codec capability was taken, for the log
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VendorCapability {
    Taken(CodecKind),
    /// Known codec, but not usable (e.g. mono-only aptX, an LHDC version we
    /// do not encode)
    Ignored(CodecKind, &'static str),
    AptxAdaptive,
    Unknown { vendor_id: u32, codec_id: u16 },
    TooShort,
}

impl RemoteCaps {
    pub fn has(&self, kind: CodecKind) -> bool {
        self.sep(kind).is_some()
    }

    pub fn sep(&self, kind: CodecKind) -> Option<RemoteSep> {
        match kind {
            CodecKind::Sbc => self.sbc,
            CodecKind::Aac => self.aac,
            CodecKind::Ldac => self.ldac,
            CodecKind::Aptx => self.aptx,
            CodecKind::AptxHd => self.aptx_hd,
            CodecKind::AptxLl => self.aptx_ll,
            CodecKind::LhdcV5 => self.lhdcv5,
        }
    }

    pub fn add_sbc(&mut self, seid: u8) {
        self.sbc = Some(RemoteSep { seid });
    }

    pub fn add_aac(&mut self, seid: u8) {
        self.aac = Some(RemoteSep { seid });
    }

    /// A vendor codec capability (media codec information) of SEP `seid`
    pub fn add_vendor(&mut self, seid: u8, info: &[u8]) -> VendorCapability {
        let Some((vid, cid)) = read_vendor(info) else {
            return VendorCapability::TooShort;
        };
        let byte = |i: usize| info.get(i).copied().unwrap_or(0);
        if vid == LDAC_VENDOR_ID && cid == LDAC_CODEC_ID {
            self.ldac = Some(RemoteSep { seid });
            return VendorCapability::Taken(CodecKind::Ldac);
        }
        if vid == APTXHD_VENDOR_ID && cid == APTXHD_CODEC_ID {
            // Only stereo is sent: skip mono-only sinks
            let caps = byte(6);
            if info.len() >= 7 && caps & APTX_CH_STEREO == 0 {
                return VendorCapability::Ignored(CodecKind::AptxHd, "no stereo");
            }
            self.aptx_hd = Some(RemoteSep { seid });
            self.aptxhd_caps = caps;
            return VendorCapability::Taken(CodecKind::AptxHd);
        }
        if (vid == APTXLL_VENDOR_ID || vid == APTXLL_VENDOR_ID2) && cid == APTXLL_CODEC_ID {
            let caps = byte(6);
            if info.len() >= 7 && caps & APTX_CH_STEREO == 0 {
                return VendorCapability::Ignored(CodecKind::AptxLl, "no stereo");
            }
            // Keep the first usable LL SEP if listed under both IDs
            if self.aptx_ll.is_none() {
                self.aptx_ll = Some(RemoteSep { seid });
                self.aptxll_caps = caps;
                self.aptxll_vendor_id = vid;
                self.aptxll_info = info[..info.len().min(APTXLL_EXT_INFO_LEN)].to_vec();
            }
            return VendorCapability::Taken(CodecKind::AptxLl);
        }
        if vid == APTXAD_VENDOR_ID && cid == APTXAD_CODEC_ID {
            self.aptx_adaptive = true;
            return VendorCapability::AptxAdaptive;
        }
        if vid == LHDCV5_VENDOR_ID && cid == LHDCV5_CODEC_ID {
            let usable = info.len() >= LHDCV5_INFO_LEN
                && info[6] & LHDCV5_SR_MASK != 0
                && info[7] & (LHDCV5_BITS_16 | LHDCV5_BITS_24) != 0
                && info[8] & LHDCV5_VERSION_MASK & LHDCV5_VERSION_1 != 0
                && info[8] & LHDCV5_FRAME_5MS != 0;
            if !usable || !CodecKind::LhdcV5.available() {
                return VendorCapability::Ignored(CodecKind::LhdcV5, "version / frame length / rates");
            }
            if self.lhdcv5.is_none() {
                self.lhdcv5 = Some(RemoteSep { seid });
                self.lhdcv5_info.copy_from_slice(&info[..LHDCV5_INFO_LEN]);
            }
            return VendorCapability::Taken(CodecKind::LhdcV5);
        }
        if vid == APTX_VENDOR_ID && cid == APTX_CODEC_ID {
            let caps = byte(6);
            if info.len() >= 7 && caps & APTX_CH_STEREO == 0 {
                return VendorCapability::Ignored(CodecKind::Aptx, "no stereo");
            }
            self.aptx = Some(RemoteSep { seid });
            self.aptx_caps = caps;
            return VendorCapability::Taken(CodecKind::Aptx);
        }
        VendorCapability::Unknown { vendor_id: vid, codec_id: cid }
    }

    pub fn lhdcv5_rates(&self) -> u8 {
        self.lhdcv5_info[6] & LHDCV5_SR_MASK
    }

    pub fn lhdcv5_24bit(&self) -> bool {
        self.lhdcv5_info[7] & LHDCV5_BITS_24 != 0
    }

    pub fn lhdcv5_bitrate_limits(&self) -> u8 {
        self.lhdcv5_info[7] & LHDCV5_BITRATE_LIMITS_MASK
    }

    /// The best codec both sides have (Auto)
    pub fn best(&self) -> Option<CodecKind> {
        CodecKind::AUTO_PRIORITY.into_iter().find(|&k| k.available() && self.has(k))
    }

    /// Capture rate for the aptX family: 44.1 or 48 kHz as the remote lists
    /// them, preferring `wanted` (other rates: 48k, then 44.1k)
    pub fn pick_aptx_sample_rate(&self, kind: CodecKind, wanted: u32) -> u32 {
        let caps = match kind {
            CodecKind::Aptx => self.aptx_caps,
            CodecKind::AptxHd => self.aptxhd_caps,
            CodecKind::AptxLl => self.aptxll_caps,
            _ => return wanted,
        };
        let freqs = caps & APTX_FREQ_MASK;
        let has441 = freqs == 0 || freqs & APTX_FREQ_44100 != 0;
        let has48 = freqs == 0 || freqs & APTX_FREQ_48000 != 0;
        if wanted == 48000 && has48 {
            48000
        } else if wanted == 44100 && has441 {
            44100
        } else if has48 {
            48000
        } else if has441 {
            44100
        } else {
            wanted
        }
    }

    /// Capture rate for LHDC V5: 44.1 / 48 / 96 / 192 kHz as the remote
    /// lists them, preferring `wanted` (88.2k maps to 96k)
    pub fn pick_lhdcv5_sample_rate(&self, wanted: u32) -> u32 {
        let rates = self.lhdcv5_rates();
        let has = |bit: u8| rates & bit != 0;
        let wanted = if wanted == 88200 { 96000 } else { wanted };
        match wanted {
            44100 if has(LHDCV5_SR_44100) => return 44100,
            48000 if has(LHDCV5_SR_48000) => return 48000,
            96000 if has(LHDCV5_SR_96000) => return 96000,
            192000 if has(LHDCV5_SR_192000) => return 192000,
            _ => {}
        }
        for (bit, rate) in [(LHDCV5_SR_48000, 48000), (LHDCV5_SR_44100, 44100), (LHDCV5_SR_96000, 96000), (LHDCV5_SR_192000, 192000)] {
            if has(bit) {
                return rate;
            }
        }
        wanted
    }

    /// The SET_CONFIGURATION for a stream of `kind`
    pub fn configuration(&self, kind: CodecKind, sample_rate: u32, channels: u32, bits_per_sample: u8) -> Result<Configuration, ConfigError> {
        let Some(sep) = self.sep(kind) else {
            return Err(ConfigError::NotOffered(kind));
        };
        let remote_seid = sep.seid;
        let mono = channels == 1;
        let request = match kind {
            CodecKind::Sbc => {
                if sample_rate != 44100 && sample_rate != 48000 {
                    return Err(ConfigError::SampleRate(kind, sample_rate));
                }
                ConfigRequest::Sbc { sampling_frequency: sample_rate as u16, mono, min_bitpool: 2, max_bitpool: 53 }
            }
            CodecKind::Aac => ConfigRequest::Aac { sampling_frequency: sample_rate, channels: channels as u8, bit_rate: 256_000 },
            CodecKind::Ldac => {
                let freq = match sample_rate {
                    44100 => 0x20,
                    48000 => 0x10,
                    88200 => 0x08,
                    _ => 0x04,
                };
                let mut info = vendor_header(LDAC_VENDOR_ID, LDAC_CODEC_ID);
                info.extend_from_slice(&[freq, if mono { 0x04 } else { 0x01 }]);
                ConfigRequest::Vendor(info)
            }
            CodecKind::AptxHd => {
                let freq = aptx_freq_bits(kind, self.aptxhd_caps, sample_rate)?;
                let mut info = vendor_header(APTXHD_VENDOR_ID, APTXHD_CODEC_ID);
                // Always stereo on the wire; the encoder duplicates mono input
                info.push(freq | APTX_CH_STEREO);
                info.resize(APTXHD_INFO_LEN, 0);
                ConfigRequest::Vendor(info)
            }
            CodecKind::Aptx => {
                let freq = aptx_freq_bits(kind, self.aptx_caps, sample_rate)?;
                let mut info = vendor_header(APTX_VENDOR_ID, APTX_CODEC_ID);
                info.push(freq | APTX_CH_STEREO);
                ConfigRequest::Vendor(info)
            }
            CodecKind::AptxLl => {
                let freq = aptx_freq_bits(kind, self.aptxll_caps, sample_rate)?;
                let mut info = vendor_header(self.aptxll_vendor_id, APTXLL_CODEC_ID);
                info.push(freq | APTX_CH_STEREO);
                info.push(0); // no backchannel
                let rc = &self.aptxll_info;
                if rc.len() >= APTXLL_EXT_INFO_LEN && rc[7] & APTXLL_HAS_NEW_CAPS != 0 {
                    // The sink sent the extended caps: echo them back like
                    // PipeWire, with the buffer levels at least its defaults
                    let le16 = |i: usize| u16::from_le_bytes([rc[i], rc[i + 1]]);
                    let target = le16(9).max(APTXLL_TARGET_LEVEL);
                    let initial = le16(11).max(APTXLL_INITIAL_LEVEL);
                    let good = le16(15).max(APTXLL_GOOD_LEVEL);
                    info[7] = APTXLL_HAS_NEW_CAPS;
                    info.push(rc[8]);
                    info.extend_from_slice(&target.to_le_bytes());
                    info.extend_from_slice(&initial.to_le_bytes());
                    info.push(if rc[13] != 0 { rc[13] } else { APTXLL_SRA_MAX_RATE });
                    info.push(if rc[14] != 0 { rc[14] } else { APTXLL_SRA_AVG_TIME });
                    info.extend_from_slice(&good.to_le_bytes());
                }
                ConfigRequest::Vendor(info)
            }
            CodecKind::LhdcV5 => {
                // One rate, one bit depth, the sink's bit rate limits echoed,
                // V5 ver.1, 5 ms frames, no optional features; stereo only
                let freq = match sample_rate {
                    44100 => LHDCV5_SR_44100,
                    48000 => LHDCV5_SR_48000,
                    96000 => LHDCV5_SR_96000,
                    192000 => LHDCV5_SR_192000,
                    _ => 0,
                };
                if freq & self.lhdcv5_rates() == 0 {
                    return Err(ConfigError::SampleRate(kind, sample_rate));
                }
                let bits = if bits_per_sample == 24 { LHDCV5_BITS_24 } else { LHDCV5_BITS_16 };
                if bits & self.lhdcv5_info[7] & LHDCV5_BITS_MASK == 0 {
                    return Err(ConfigError::BitDepth(kind, bits_per_sample));
                }
                let mut info = vendor_header(LHDCV5_VENDOR_ID, LHDCV5_CODEC_ID);
                info.extend_from_slice(&[freq, bits | self.lhdcv5_bitrate_limits(), LHDCV5_CAPS_P8]);
                info.resize(LHDCV5_INFO_LEN, 0);
                ConfigRequest::Vendor(info)
            }
        };
        Ok(Configuration { kind, remote_seid, request })
    }
}

fn aptx_freq_bits(kind: CodecKind, remote_caps: u8, sample_rate: u32) -> Result<u8, ConfigError> {
    let freq = match sample_rate {
        44100 => APTX_FREQ_44100,
        48000 => APTX_FREQ_48000,
        _ => return Err(ConfigError::SampleRate(kind, sample_rate)),
    };
    let remote = remote_caps & APTX_FREQ_MASK;
    if remote != 0 && remote & freq == 0 {
        return Err(ConfigError::SampleRate(kind, sample_rate));
    }
    Ok(freq)
}

/// SET_CONFIGURATION of a stream
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration {
    pub kind: CodecKind,
    pub remote_seid: u8,
    pub request: ConfigRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigRequest {
    /// Joint stereo (mono for one channel), 16 blocks, 8 subbands, loudness
    Sbc { sampling_frequency: u16, mono: bool, min_bitpool: u8, max_bitpool: u8 },
    /// MPEG-2 AAC-LC, constant bit rate
    Aac { sampling_frequency: u32, channels: u8, bit_rate: u32 },
    /// Vendor codec information
    Vendor(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("the device does not offer {0}")]
    NotOffered(CodecKind),
    #[error("{0} does not support {1} Hz here")]
    SampleRate(CodecKind, u32),
    #[error("{0} does not support {1}-bit here")]
    BitDepth(CodecKind, u8),
}

/// Why the codec of a profile is not used
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// The codec may be on a SEP another device streams to (multipoint):
    /// BTstack does not query SEPs in use
    SepInUse { in_use: u8, total: u8 },
    /// The sink offers only aptX Adaptive, which has no open encoder
    AdaptiveOnly,
    NotSupported,
}

/// The codec to stream with
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub kind: CodecKind,
    /// The requested codec was replaced by `kind` (codec fallback)
    pub replaced: Option<(CodecKind, Unavailable)>,
}

/// Picks the codec for `requested` (None = Auto). Without `fallback`, a
/// requested codec the device does not offer is an error.
pub fn choose(caps: &RemoteCaps, requested: Option<CodecKind>, fallback: bool) -> Result<Choice, Option<(CodecKind, Unavailable)>> {
    let Some(wanted) = requested else {
        return caps.best().map(|kind| Choice { kind, replaced: None }).ok_or(None);
    };
    if wanted.available() && caps.has(wanted) {
        return Ok(Choice { kind: wanted, replaced: None });
    }
    let why = if caps.audio_sinks_in_use > 0 {
        Unavailable::SepInUse { in_use: caps.audio_sinks_in_use, total: caps.audio_sinks }
    } else if matches!(wanted, CodecKind::Aptx | CodecKind::AptxHd | CodecKind::AptxLl) && caps.aptx_adaptive {
        Unavailable::AdaptiveOnly
    } else {
        Unavailable::NotSupported
    };
    if fallback {
        if let Some(kind) = caps.best() {
            return Ok(Choice { kind, replaced: Some((wanted, why)) });
        }
    }
    Err(Some((wanted, why)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_prefers_ldac_then_aptx_family() {
        let mut caps = RemoteCaps::default();
        caps.add_sbc(1);
        caps.add_aac(2);
        assert_eq!(caps.best(), if CodecKind::Aac.available() { Some(CodecKind::Aac) } else { Some(CodecKind::Sbc) });
        caps.add_vendor(3, &[0x4F, 0, 0, 0, 0x01, 0x00, 0x22]);
        if CodecKind::Aptx.available() {
            assert_eq!(caps.best(), Some(CodecKind::Aptx));
        }
        caps.add_vendor(4, &[0x2D, 0x01, 0, 0, 0xAA, 0x00, 0x3C, 0x07]);
        if CodecKind::Ldac.available() {
            assert_eq!(caps.best(), Some(CodecKind::Ldac));
        }
    }

    #[test]
    fn mono_only_aptx_is_ignored() {
        let mut caps = RemoteCaps::default();
        let r = caps.add_vendor(3, &[0x4F, 0, 0, 0, 0x01, 0x00, 0x21]);
        assert_eq!(r, VendorCapability::Ignored(CodecKind::Aptx, "no stereo"));
        assert!(!caps.has(CodecKind::Aptx));
    }

    #[test]
    fn aptx_ll_extended_config_echoes_raised_levels() {
        let mut caps = RemoteCaps::default();
        let mut info = vec![0x0A, 0, 0, 0, 0x02, 0x00, 0x32, 0x02, 0x00];
        info.extend_from_slice(&100u16.to_le_bytes()); // target
        info.extend_from_slice(&1000u16.to_le_bytes()); // initial
        info.extend_from_slice(&[0, 0]); // sra
        info.extend_from_slice(&50u16.to_le_bytes()); // good
        assert_eq!(caps.add_vendor(7, &info), VendorCapability::Taken(CodecKind::AptxLl));
        let c = caps.configuration(CodecKind::AptxLl, 48000, 2, 16).unwrap();
        let ConfigRequest::Vendor(v) = c.request else { panic!() };
        assert_eq!(v.len(), 17);
        assert_eq!(v[6], 0x12);
        assert_eq!(v[7], APTXLL_HAS_NEW_CAPS);
        assert_eq!(u16::from_le_bytes([v[9], v[10]]), APTXLL_TARGET_LEVEL);
        assert_eq!(u16::from_le_bytes([v[11], v[12]]), 1000);
        assert_eq!(v[13], APTXLL_SRA_MAX_RATE);
        assert_eq!(u16::from_le_bytes([v[15], v[16]]), APTXLL_GOOD_LEVEL);
    }

    #[test]
    fn fallback_reports_why() {
        let mut caps = RemoteCaps::default();
        caps.add_sbc(1);
        caps.audio_sinks = 2;
        caps.audio_sinks_in_use = 1;
        let c = choose(&caps, Some(CodecKind::Ldac), true).unwrap();
        assert_eq!(c.kind, CodecKind::Sbc);
        assert_eq!(c.replaced, Some((CodecKind::Ldac, Unavailable::SepInUse { in_use: 1, total: 2 })));
        assert!(choose(&caps, Some(CodecKind::Ldac), false).is_err());
    }

    #[test]
    fn aptx_rates() {
        let mut caps = RemoteCaps::default();
        caps.add_vendor(3, &[0x4F, 0, 0, 0, 0x01, 0x00, 0x22]);
        assert_eq!(caps.pick_aptx_sample_rate(CodecKind::Aptx, 48000), 44100);
        assert_eq!(caps.pick_aptx_sample_rate(CodecKind::Aptx, 96000), 44100);
        assert!(caps.configuration(CodecKind::Aptx, 48000, 2, 16).is_err());
    }
}
