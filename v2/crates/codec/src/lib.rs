//! A2DP codecs for A2DPWB.
//!
//! - [`Encoder`]: what the encode thread drives. One call encodes
//!   [`Encoder::pcm_frames_per_encode`] PCM frames.
//! - [`sbc`]: the SBC encoder, ported from Bluedroid to Rust (bit-exact).
//! - LDAC, aptX family, AAC, LHDC V5: safe wrappers of the C encoders
//!   (crates `*-sys`), behind cargo features.
//! - [`caps`]: codec information of the local stream endpoints, parsing of
//!   the remote's capabilities, the configuration sent with
//!   SET_CONFIGURATION, and the codec choice (Auto).

#![forbid(unsafe_code)]

pub mod caps;
pub mod sbc;

#[cfg(feature = "aac")]
pub mod aac;
#[cfg(feature = "aptx")]
pub mod aptx;
#[cfg(feature = "ldac")]
pub mod ldac;
#[cfg(feature = "lhdcv5")]
pub mod lhdcv5;

use std::fmt;

/// The A2DP codecs A2DPWB can stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodecKind {
    Sbc,
    Aac,
    Aptx,
    AptxHd,
    AptxLl,
    Ldac,
    LhdcV5,
}

impl CodecKind {
    pub const ALL: [CodecKind; 7] = [
        CodecKind::Ldac,
        CodecKind::AptxHd,
        CodecKind::AptxLl,
        CodecKind::Aptx,
        CodecKind::Sbc,
        CodecKind::Aac,
        CodecKind::LhdcV5,
    ];

    /// The choice of "Auto", best first. LHDC V5 stays below the aptX family
    /// and above AAC / SBC, as in v1.
    pub const AUTO_PRIORITY: [CodecKind; 7] = [
        CodecKind::Ldac,
        CodecKind::AptxHd,
        CodecKind::AptxLl,
        CodecKind::Aptx,
        CodecKind::LhdcV5,
        CodecKind::Aac,
        CodecKind::Sbc,
    ];

    /// Display name (as v1 shows it)
    pub fn name(self) -> &'static str {
        match self {
            CodecKind::Sbc => "SBC",
            CodecKind::Aac => "AAC",
            CodecKind::Aptx => "aptX",
            CodecKind::AptxHd => "aptX HD",
            CodecKind::AptxLl => "aptX Low Latency",
            CodecKind::Ldac => "LDAC",
            CodecKind::LhdcV5 => "LHDC V5",
        }
    }

    /// Key in profiles.json and on the command line
    pub fn key(self) -> &'static str {
        match self {
            CodecKind::Sbc => "sbc",
            CodecKind::Aac => "aac",
            CodecKind::Aptx => "aptx",
            CodecKind::AptxHd => "aptxhd",
            CodecKind::AptxLl => "aptxll",
            CodecKind::Ldac => "ldac",
            CodecKind::LhdcV5 => "lhdcv5",
        }
    }

    pub fn from_key(key: &str) -> Option<CodecKind> {
        CodecKind::ALL.into_iter().find(|c| c.key().eq_ignore_ascii_case(key))
    }

    /// Bytes of the A2DP media payload header in front of the codec frames:
    /// SBC and LDAC 1 (frame count), LHDC V5 2 (frame count, sequence),
    /// the others none
    pub fn media_header_len(self) -> usize {
        match self {
            CodecKind::Sbc | CodecKind::Ldac => 1,
            CodecKind::LhdcV5 => 2,
            _ => 0,
        }
    }

    /// Classic aptX and aptX LL go out without an RTP header (Android
    /// A2DP_APTX_OFFSET, PipeWire); all others with one
    pub fn uses_rtp(self) -> bool {
        !matches!(self, CodecKind::Aptx | CodecKind::AptxLl)
    }

    /// Highest capture sample rate the codec takes
    pub fn max_sample_rate(self) -> u32 {
        match self {
            CodecKind::Ldac => 96000,
            CodecKind::LhdcV5 => 192000,
            _ => 48000,
        }
    }

    /// Whether the encoder for this codec is built in
    pub fn available(self) -> bool {
        match self {
            CodecKind::Sbc => true,
            CodecKind::Aac => cfg!(feature = "aac"),
            CodecKind::Aptx | CodecKind::AptxHd | CodecKind::AptxLl => cfg!(feature = "aptx"),
            CodecKind::Ldac => cfg!(feature = "ldac"),
            CodecKind::LhdcV5 => cfg!(feature = "lhdcv5"),
        }
    }
}

impl fmt::Display for CodecKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Quality mode (codec-specific meaning, as in v1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// LDAC 990 kbps, SBC bitpool 53, AAC 256 kbps, LHDC V5 1000 kbps
    #[default]
    High,
    /// LDAC 660 kbps, SBC bitpool 35, AAC 192 kbps
    Standard,
    /// LDAC 330 kbps, SBC bitpool 19, AAC 128 kbps
    Mobile,
    /// LHDC V5 only (900 kbps); other codecs treat it as High
    MediumHigh,
}

impl Quality {
    /// "hq" / "sq" / "mq" / "mhq" (profiles.json)
    pub fn from_key(key: &str) -> Quality {
        match key {
            "sq" => Quality::Standard,
            "mq" => Quality::Mobile,
            "mhq" => Quality::MediumHigh,
            _ => Quality::High,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Quality::High => "hq",
            Quality::Standard => "sq",
            Quality::Mobile => "mq",
            Quality::MediumHigh => "mhq",
        }
    }
}

/// Sample format an encoder takes (interleaved)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmFormat {
    /// 16-bit signed
    I16,
    /// 32-bit signed, MSB-aligned (24-bit codecs use the top 24 bits)
    I32,
}

impl PcmFormat {
    pub fn bytes(self) -> usize {
        match self {
            PcmFormat::I16 => 2,
            PcmFormat::I32 => 4,
        }
    }
}

/// Interleaved PCM handed to [`Encoder::encode`]
#[derive(Debug, Clone, Copy)]
pub enum Pcm<'a> {
    I16(&'a [i16]),
    I32(&'a [i32]),
}

impl Pcm<'_> {
    pub fn samples(&self) -> usize {
        match self {
            Pcm::I16(s) => s.len(),
            Pcm::I32(s) => s.len(),
        }
    }
}

/// What the remote's capabilities add to the encoder setup
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PeerParams {
    /// LHDC V5: the sink's bit rate limits (codec info byte 7, bits 7..4)
    pub lhdcv5_bitrate_limits: u8,
}

/// Encoder setup, fixed for a stream
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderConfig {
    pub sample_rate: u32,
    /// 1 or 2
    pub channels: u32,
    pub quality: Quality,
    /// Media payload size per packet, RTP header excluded (the stream's MTU
    /// after the media packet size limit)
    pub mtu: u16,
    /// 16 or 24: LDAC, aptX HD and LHDC V5 take 24-bit PCM (as I32)
    pub bits_per_sample: u8,
    /// Adaptive bit rate (LDAC, LHDC V5)
    pub abr: bool,
    pub peer: PeerParams,
}

/// One [`Encoder::encode`] call's output
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Encoded {
    /// Bytes written to the output
    pub bytes: usize,
    /// Codec frames in them (counted in the media payload header; 0 while an
    /// encoder that buffers input has nothing to emit)
    pub frames: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum EncoderError {
    #[error("{0} is not built into this version")]
    NotAvailable(CodecKind),
    #[error("{codec}: unsupported configuration: {what}")]
    Unsupported { codec: CodecKind, what: String },
    #[error("{codec}: encoder initialization failed: {what}")]
    Init { codec: CodecKind, what: String },
    #[error("{codec}: encoding failed: {what}")]
    Encode { codec: CodecKind, what: String },
    #[error("output buffer too small ({have} < {need} bytes)")]
    OutputTooSmall { have: usize, need: usize },
    #[error("expected {expected} samples, got {got}")]
    InputLength { expected: usize, got: usize },
}

/// An A2DP encoder. Created per stream, used on the encode thread only.
/// `encode` must not allocate.
pub trait Encoder: Send {
    fn kind(&self) -> CodecKind;

    /// Sample format of the input
    fn pcm_format(&self) -> PcmFormat;

    /// PCM frames (samples per channel) one `encode` call consumes
    fn pcm_frames_per_encode(&self) -> usize;

    /// PCM frames one codec frame of [`Encoded::frames`] stands for. The
    /// RTP timestamp advances by this per frame sent, so it stays exact
    /// when an encoder buffers input (LDAC).
    fn pcm_frames_per_codec_frame(&self) -> usize {
        self.pcm_frames_per_encode()
    }

    /// Largest output of one `encode` call, for sizing the buffer
    fn max_output_bytes(&self) -> usize;

    /// Encodes `pcm_frames_per_encode() * channels` samples into `out`
    fn encode(&mut self, pcm: Pcm<'_>, out: &mut [u8]) -> Result<Encoded, EncoderError>;

    /// Current bit rate in kbps
    fn bitrate_kbps(&self) -> u32;

    /// Adaptive bit rate step with the send queue depth (packets waiting);
    /// called by the encode thread when [`EncoderConfig::abr`] is set
    fn abr_adjust(&mut self, _queue_depth: u32) {}
}

/// Creates the encoder for `kind`
pub fn create(kind: CodecKind, config: &EncoderConfig) -> Result<Box<dyn Encoder>, EncoderError> {
    if !kind.available() {
        return Err(EncoderError::NotAvailable(kind));
    }
    match kind {
        CodecKind::Sbc => Ok(Box::new(sbc::SbcEncoder::new(config)?)),
        #[cfg(feature = "aac")]
        CodecKind::Aac => Ok(Box::new(aac::AacEncoder::new(config)?)),
        #[cfg(feature = "aptx")]
        CodecKind::Aptx | CodecKind::AptxHd | CodecKind::AptxLl => Ok(Box::new(aptx::AptxEncoder::new(kind, config)?)),
        #[cfg(feature = "ldac")]
        CodecKind::Ldac => Ok(Box::new(ldac::LdacEncoder::new(config)?)),
        #[cfg(feature = "lhdcv5")]
        CodecKind::LhdcV5 => Ok(Box::new(lhdcv5::LhdcV5Encoder::new(config)?)),
        #[allow(unreachable_patterns)]
        _ => Err(EncoderError::NotAvailable(kind)),
    }
}
