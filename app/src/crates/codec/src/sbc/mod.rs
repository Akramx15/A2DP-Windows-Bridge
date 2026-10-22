//! SBC encoder: Bluedroid's fixed-point encoder (the one BTstack bundles in
//! 3rd-party/bluedroid/encoder), ported from C to safe Rust. The output is
//! bit-exact with the C encoder as BTstack configures it; tests/sbc_bitexact.rs
//! compares the two over all configurations.
//!
//! Ported from Bluedroid, Copyright (C) 1999-2012 Broadcom Corporation,
//! licensed under the Apache License, Version 2.0
//! (http://www.apache.org/licenses/LICENSE-2.0). Each ported file carries
//! the notice.
//!
//! - [`SbcFrameEncoder`]: the general encoder, any [`SbcConfig`].
//! - [`SbcEncoder`]: the A2DP stream encoder (as v1's wrapper): 44.1 / 48
//!   kHz, joint stereo (or mono), 16 blocks, 8 subbands, loudness, the
//!   bitpool by [`Quality`]; [`SbcEncoder::new_xq`] is SBC XQ: dual channel
//!   with a high bitpool per channel ([`xq_bitpool`]). A profile may ask
//!   for other blocks, subbands, allocation and channel mode
//!   ([`SbcParams`]).
//! - [`FastStreamEncoder`]: CSR FastStream, SBC frames of a fixed setup.

mod analysis;
mod bit_alloc;
mod faststream;
mod frame;
mod packing;

pub use faststream::{
    FASTSTREAM_BITPOOL, FASTSTREAM_FRAME_BYTES, FASTSTREAM_FRAMES_PER_PACKET, FASTSTREAM_MAX_PAYLOAD, FastStreamEncoder,
};
pub use frame::{Allocation, ChannelMode, SbcConfig, SbcFrameEncoder};

use crate::{CodecKind, Encoded, Encoder, EncoderConfig, EncoderDetails, EncoderError, Pcm, PcmFormat, Quality};

const BLOCKS: u8 = 16;
const SUBBANDS: u8 = 8;
/// The local SEP's largest bitpool
const SBC_MAX_BITPOOL: u8 = 53;

/// The SBC frame setup a profile asks for: 0 / None is the default (16
/// blocks, 8 subbands, loudness; joint stereo for SBC, while SBC XQ is
/// always dual channel). The SET_CONFIGURATION and the encoder take the
/// same values (`caps::RemoteCaps::sbc_params` drops the ones the sink does
/// not list).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SbcParams {
    /// 4, 8, 12 or 16
    pub blocks: u8,
    /// 4 or 8
    pub subbands: u8,
    pub allocation: Option<Allocation>,
    /// The channel mode of stereo input (SBC only; mono input is mono).
    /// Mono here mixes stereo input down.
    pub channel_mode: Option<ChannelMode>,
}

impl SbcParams {
    pub fn blocks(&self) -> u8 {
        if matches!(self.blocks, 4 | 8 | 12) { self.blocks } else { BLOCKS }
    }

    pub fn subbands(&self) -> u8 {
        if self.subbands == 4 { 4 } else { SUBBANDS }
    }

    pub fn allocation(&self) -> Allocation {
        self.allocation.unwrap_or(Allocation::Loudness)
    }

    /// The channel mode of a `kind` stream of `channels` input channels
    pub fn channel_mode(&self, kind: CodecKind, channels: u32) -> ChannelMode {
        if channels == 1 {
            ChannelMode::Mono
        } else if kind == CodecKind::SbcXq {
            ChannelMode::DualChannel
        } else {
            self.channel_mode.unwrap_or(ChannelMode::JointStereo)
        }
    }

    /// Nothing asked for: today's setup
    pub fn is_default(&self) -> bool {
        *self == SbcParams::default()
    }
}

impl ChannelMode {
    /// "mono", "dual_channel", "stereo", "joint_stereo" (profiles, IPC)
    pub fn key(self) -> &'static str {
        match self {
            ChannelMode::Mono => "mono",
            ChannelMode::DualChannel => "dual_channel",
            ChannelMode::Stereo => "stereo",
            ChannelMode::JointStereo => "joint_stereo",
        }
    }

    pub fn from_key(key: &str) -> Option<ChannelMode> {
        ChannelMode::ALL.into_iter().find(|m| m.key() == key)
    }
}

impl Allocation {
    /// "loudness", "snr" (profiles, IPC)
    pub fn key(self) -> &'static str {
        match self {
            Allocation::Loudness => "loudness",
            Allocation::Snr => "snr",
        }
    }

    pub fn from_key(key: &str) -> Option<Allocation> {
        [Allocation::Loudness, Allocation::Snr].into_iter().find(|a| a.key() == key)
    }
}

/// SBC bitpool for a quality mode (joint stereo; mono takes the same)
pub fn quality_bitpool(quality: Quality) -> u8 {
    match quality {
        Quality::High | Quality::MediumHigh | Quality::LowLatency => 53,
        Quality::Standard => 35,
        Quality::Mobile => 19,
    }
}

/// SBC XQ bitpool (per channel, dual channel; mono takes the same) for a
/// quality mode and sample rate:
/// - High: 47 at 44.1 kHz (the "SBC XQ" of Android custom ROMs and
///   pulseaudio-modules-bt: 200-byte frames, ~551 kbps), 43 at 48 kHz (the
///   same ~552 kbps). Three frames and the media payload header fill one
///   2-DH5 packet, five one 3-DH5.
/// - Standard: 43 / 39, PipeWire's SBC-XQ default (a2dp-codec-sbc.c,
///   default_bitpool): the ~512 kbps every A2DP 1.2 sink has to take.
/// - Mobile: 32 / 29, PipeWire's dual channel default without XQ (~385 kbps).
pub fn xq_bitpool(quality: Quality, sample_rate: u32) -> u8 {
    let (at_44100, at_48000) = match quality {
        Quality::High | Quality::MediumHigh | Quality::LowLatency => (47, 43),
        Quality::Standard => (43, 39),
        Quality::Mobile => (32, 29),
    };
    if sample_rate == 44100 { at_44100 } else { at_48000 }
}

/// Largest SBC XQ bitpool at a sample rate (any quality): the maximum the
/// SET_CONFIGURATION gives
pub fn xq_max_bitpool(sample_rate: u32) -> u8 {
    xq_bitpool(Quality::High, sample_rate)
}

/// `wanted` within the sink's SBC bitpool range when it is known (as the
/// SET_CONFIGURATION's range, `caps::RemoteCaps::sbc_bitpool_range`)
fn sink_bitpool(wanted: u8, config: &EncoderConfig) -> u8 {
    let (min, max) = (config.peer.sbc_min_bitpool, config.peer.sbc_max_bitpool);
    if max < 2 {
        tracing::debug!("SBC: bitpool {wanted} (profile {}, sink range not known)", config.peer.sbc_bitpool);
        return wanted;
    }
    let bitpool = wanted.min(max).max(min.min(max)).max(2);
    tracing::debug!(
        "SBC: bitpool {bitpool} (wanted {wanted}, profile {}, sink range {min}..{max})",
        config.peer.sbc_bitpool
    );
    bitpool
}

/// The bitpool a profile's Auto takes for a channel mode: the quality's
/// joint stereo bitpool (mono and stereo the same); dual channel halves it
/// per channel, for about the same bit rate
pub fn auto_bitpool(quality: Quality, channel_mode: ChannelMode) -> u8 {
    let bitpool = quality_bitpool(quality);
    if channel_mode == ChannelMode::DualChannel { bitpool.div_ceil(2) } else { bitpool }
}

/// Bit rate of SBC frames of `frame_len` bytes ([`SbcConfig::frame_len`],
/// the A2DP specification's formula), each `pcm_frames` PCM frames at
/// `sample_rate`: kbps, rounded down (as v1)
pub fn bitrate_kbps(frame_len: usize, sample_rate: u32, pcm_frames: usize) -> u32 {
    (8 * frame_len as u64 * sample_rate as u64 / pcm_frames.max(1) as u64 / 1000) as u32
}

/// The A2DP SBC encoder: one SBC frame (blocks x subbands PCM frames, 128
/// by default) per `encode`
pub struct SbcEncoder {
    kind: CodecKind,
    frame: SbcFrameEncoder,
    frame_len: usize,
    sample_rate: u32,
    /// Mono frames from stereo input: the input mixed down into this
    downmix: Option<Vec<i16>>,
}

impl SbcEncoder {
    /// SBC: joint stereo (mono for one channel) unless the profile asks for
    /// another setup, the bitpool by quality within the sink's range
    pub fn new(config: &EncoderConfig) -> Result<SbcEncoder, EncoderError> {
        let mode = config.peer.sbc.channel_mode(CodecKind::Sbc, config.channels);
        let wanted = match config.peer.sbc_bitpool {
            0 => auto_bitpool(config.quality, mode),
            n => n.min(SBC_MAX_BITPOOL),
        };
        let bitpool = sink_bitpool(wanted, config);
        Self::with_mode(CodecKind::Sbc, config, bitpool)
    }

    /// SBC XQ: dual channel (mono for one channel), [`xq_bitpool`] within
    /// the sink's range
    pub fn new_xq(config: &EncoderConfig) -> Result<SbcEncoder, EncoderError> {
        // Never above the configuration's maximum for the rate
        let wanted = match config.peer.sbc_bitpool {
            0 => xq_bitpool(config.quality, config.sample_rate),
            n => n.min(xq_max_bitpool(config.sample_rate)),
        };
        let bitpool = sink_bitpool(wanted, config);
        Self::with_mode(CodecKind::SbcXq, config, bitpool)
    }

    fn with_mode(kind: CodecKind, config: &EncoderConfig, bitpool: u8) -> Result<SbcEncoder, EncoderError> {
        let unsupported = |what: String| EncoderError::Unsupported { codec: kind, what };
        if config.sample_rate != 44100 && config.sample_rate != 48000 {
            return Err(unsupported(format!("sample rate {} Hz (44100 or 48000)", config.sample_rate)));
        }
        if !matches!(config.channels, 1 | 2) {
            return Err(unsupported(format!("{} channels", config.channels)));
        }
        let params = config.peer.sbc;
        let channel_mode = params.channel_mode(kind, config.channels);
        let frame = SbcFrameEncoder::new(SbcConfig {
            sample_rate: config.sample_rate,
            channel_mode,
            blocks: params.blocks(),
            subbands: params.subbands(),
            allocation: params.allocation(),
            bitpool,
        })?;
        // Bitpool is never 0 here, so every frame has exactly this length
        let frame_len = frame.max_frame_len();
        let downmix = (config.channels == 2 && channel_mode == ChannelMode::Mono).then(|| vec![0; frame.pcm_samples()]);
        Ok(SbcEncoder { kind, frame, frame_len, sample_rate: config.sample_rate, downmix })
    }

    fn pcm_frames(&self) -> usize {
        self.frame.config().pcm_frames_per_frame()
    }

    /// Length of every SBC frame of this stream
    pub fn frame_len(&self) -> usize {
        self.frame_len
    }

    pub fn bitpool(&self) -> u8 {
        self.frame.config().bitpool
    }
}

impl Encoder for SbcEncoder {
    fn kind(&self) -> CodecKind {
        self.kind
    }

    fn pcm_format(&self) -> PcmFormat {
        PcmFormat::I16
    }

    fn pcm_frames_per_encode(&self) -> usize {
        self.pcm_frames()
    }

    fn max_output_bytes(&self) -> usize {
        self.frame_len
    }

    fn encode(&mut self, pcm: Pcm<'_>, out: &mut [u8]) -> Result<Encoded, EncoderError> {
        let Pcm::I16(pcm) = pcm else {
            return Err(EncoderError::Encode { codec: self.kind, what: "SBC takes 16-bit PCM".into() });
        };
        let bytes = match self.downmix.as_mut() {
            Some(mono) => {
                if pcm.len() != 2 * mono.len() {
                    return Err(EncoderError::InputLength { expected: 2 * mono.len(), got: pcm.len() });
                }
                for (m, [l, r]) in mono.iter_mut().zip(pcm.as_chunks::<2>().0) {
                    *m = ((*l as i32 + *r as i32) >> 1) as i16;
                }
                self.frame.encode(mono, out)?
            }
            None => self.frame.encode(pcm, out)?,
        };
        Ok(Encoded { bytes, frames: 1 })
    }

    /// As v1: 8 * frame length * sample rate / (subbands * blocks), in kbps,
    /// rounded down
    fn bitrate_kbps(&self) -> u32 {
        bitrate_kbps(self.frame_len, self.sample_rate, self.pcm_frames())
    }

    fn details(&self) -> EncoderDetails {
        let c = *self.frame.config();
        EncoderDetails {
            frame_bytes: Some(self.frame_len as u32),
            sbc: Some(SbcConfig { bitpool: c.effective_bitpool(), ..c }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PeerParams;

    fn config(sample_rate: u32, channels: u32, quality: Quality) -> EncoderConfig {
        EncoderConfig {
            sample_rate,
            channels,
            quality,
            mtu: 895,
            bits_per_sample: 16,
            abr: false,
            peer: PeerParams::default(),
        }
    }

    #[test]
    fn profile_bitpool_overrides_quality_within_limits() {
        let with = |bitpool: u8, peer_max: u8| EncoderConfig {
            peer: crate::PeerParams { sbc_bitpool: bitpool, sbc_max_bitpool: peer_max, ..Default::default() },
            ..config(44100, 2, Quality::Mobile)
        };
        assert_eq!(SbcEncoder::new(&with(0, 0)).unwrap().bitpool(), 19);
        assert_eq!(SbcEncoder::new(&with(40, 0)).unwrap().bitpool(), 40);
        assert_eq!(SbcEncoder::new(&with(200, 0)).unwrap().bitpool(), 53);
        // The sink's range still wins
        assert_eq!(SbcEncoder::new(&with(50, 35)).unwrap().bitpool(), 35);
        // XQ never goes above its configured maximum
        assert_eq!(SbcEncoder::new_xq(&with(53, 0)).unwrap().bitpool(), xq_max_bitpool(44100));
    }

    #[test]
    fn frame_length_and_bitrate() {
        // (rate, channels, quality, bitpool, frame length, kbps)
        let cases = [
            (44100, 2, Quality::High, 53, 119, 327),
            (44100, 2, Quality::Standard, 35, 83, 228),
            (44100, 2, Quality::Mobile, 19, 51, 140),
            (48000, 2, Quality::High, 53, 119, 357),
            (48000, 2, Quality::Standard, 35, 83, 249),
            (48000, 2, Quality::Mobile, 19, 51, 153),
            (44100, 1, Quality::High, 53, 114, 314),
            (44100, 1, Quality::Standard, 35, 78, 214),
            (44100, 1, Quality::Mobile, 19, 46, 126),
            (48000, 1, Quality::High, 53, 114, 342),
            (48000, 1, Quality::Standard, 35, 78, 234),
            (48000, 1, Quality::Mobile, 19, 46, 138),
        ];
        for (rate, ch, quality, bitpool, len, kbps) in cases {
            let mut e = SbcEncoder::new(&config(rate, ch, quality)).unwrap();
            assert_eq!(e.bitpool(), bitpool);
            assert_eq!(e.frame_len(), len, "{rate} {ch} {quality:?}");
            assert_eq!(e.max_output_bytes(), len);
            assert_eq!(e.bitrate_kbps(), kbps, "{rate} {ch} {quality:?}");
            assert_eq!(e.pcm_frames_per_encode(), 128);
            assert_eq!(e.pcm_format(), PcmFormat::I16);

            // Every frame has that length, whatever the input
            let mut out = vec![0u8; len];
            let mut seed = 1u32;
            for i in 0..50 {
                let pcm: Vec<i16> = (0..128 * ch as usize)
                    .map(|_| {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        if i % 10 == 0 { 0 } else { seed as i16 }
                    })
                    .collect();
                let r = e.encode(Pcm::I16(&pcm), &mut out).unwrap();
                assert_eq!(r, Encoded { bytes: len, frames: 1 });
                assert_eq!(out[0], 0x9C);
                assert_eq!(out[2], bitpool);
            }
        }
    }

    #[test]
    fn medium_high_is_high() {
        let e = SbcEncoder::new(&config(48000, 2, Quality::MediumHigh)).unwrap();
        assert_eq!(e.bitpool(), 53);
    }

    #[test]
    fn header() {
        let mut e = SbcEncoder::new(&config(44100, 2, Quality::High)).unwrap();
        let mut out = [0u8; 119];
        e.encode(Pcm::I16(&[0; 256]), &mut out).unwrap();
        // 44.1 kHz (10), 16 blocks (11), joint stereo (11), loudness (0), 8
        // subbands (1)
        assert_eq!(out[1], 0b1011_1101);
        let mut e = SbcEncoder::new(&config(48000, 1, Quality::High)).unwrap();
        e.encode(Pcm::I16(&[0; 128]), &mut out).unwrap();
        // 48 kHz, 16 blocks, mono, loudness, 8 subbands
        assert_eq!(out[1], 0b1111_0001);
    }

    #[test]
    fn unsupported_configurations() {
        for (rate, ch) in [(32000, 2), (16000, 1), (96000, 2), (44100, 0), (48000, 3)] {
            assert!(matches!(
                SbcEncoder::new(&config(rate, ch, Quality::High)),
                Err(EncoderError::Unsupported { codec: CodecKind::Sbc, .. })
            ));
        }
    }

    #[test]
    fn input_and_output_length_errors() {
        let mut e = SbcEncoder::new(&config(44100, 2, Quality::High)).unwrap();
        let mut out = [0u8; 119];
        for n in [0, 128, 255, 257, 512] {
            let pcm = vec![0i16; n];
            assert!(matches!(
                e.encode(Pcm::I16(&pcm), &mut out),
                Err(EncoderError::InputLength { expected: 256, got }) if got == n
            ));
        }
        assert!(matches!(
            e.encode(Pcm::I16(&[0; 256]), &mut [0u8; 118]),
            Err(EncoderError::OutputTooSmall { have: 118, need: 119 })
        ));
        assert!(matches!(e.encode(Pcm::I32(&[0; 256]), &mut out), Err(EncoderError::Encode { .. })));
        // Still usable
        assert_eq!(e.encode(Pcm::I16(&[0; 256]), &mut out).unwrap().bytes, 119);

        let mut e = SbcEncoder::new(&config(48000, 1, Quality::Mobile)).unwrap();
        assert!(matches!(
            e.encode(Pcm::I16(&[0; 256]), &mut out),
            Err(EncoderError::InputLength { expected: 128, got: 256 })
        ));
    }

    #[test]
    fn create_returns_sbc() {
        let e = crate::create(CodecKind::Sbc, &config(48000, 2, Quality::Standard)).unwrap();
        assert_eq!(e.kind(), CodecKind::Sbc);
        assert_eq!(e.max_output_bytes(), 83);
    }

    #[test]
    fn xq_frame_length_and_bitrate() {
        // (rate, channels, quality, bitpool, frame length, kbps, header byte 1)
        let cases = [
            (44100, 2, Quality::High, 47, 200, 551, 0b1011_0101),
            (44100, 2, Quality::Standard, 43, 184, 507, 0b1011_0101),
            (44100, 2, Quality::Mobile, 32, 140, 385, 0b1011_0101),
            (48000, 2, Quality::High, 43, 184, 552, 0b1111_0101),
            (48000, 2, Quality::Standard, 39, 168, 504, 0b1111_0101),
            (48000, 2, Quality::Mobile, 29, 128, 384, 0b1111_0101),
            (44100, 1, Quality::High, 47, 102, 281, 0b1011_0001),
            (48000, 1, Quality::High, 43, 94, 282, 0b1111_0001),
        ];
        for (rate, ch, quality, bitpool, len, kbps, hdr) in cases {
            let mut e = SbcEncoder::new_xq(&config(rate, ch, quality)).unwrap();
            assert_eq!(e.kind(), CodecKind::SbcXq);
            assert_eq!(e.bitpool(), bitpool, "{rate} {ch} {quality:?}");
            assert_eq!(e.frame_len(), len, "{rate} {ch} {quality:?}");
            assert_eq!(e.bitrate_kbps(), kbps, "{rate} {ch} {quality:?}");
            let mut out = vec![0u8; len];
            let pcm: Vec<i16> = (0..128 * ch as usize).map(|i| ((i * 7919) % 20000) as i16 - 10000).collect();
            for _ in 0..4 {
                assert_eq!(e.encode(Pcm::I16(&pcm), &mut out).unwrap(), Encoded { bytes: len, frames: 1 });
                assert_eq!((out[0], out[1], out[2]), (0x9C, hdr, bitpool));
            }
        }
        assert_eq!(SbcEncoder::new_xq(&config(44100, 2, Quality::MediumHigh)).unwrap().bitpool(), 47);
        assert_eq!((xq_max_bitpool(44100), xq_max_bitpool(48000)), (47, 43));
    }

    #[test]
    fn xq_fills_packets() {
        // High: three frames and the media payload header in one 2-DH5 (679
        // bytes less 4 L2CAP and 12 RTP), five in one 3-DH5 (1021 less 16)
        for rate in [44100, 48000] {
            let len = SbcEncoder::new_xq(&config(rate, 2, Quality::High)).unwrap().frame_len();
            let packet = |frames: usize| 1 + frames * len;
            assert!(packet(3) <= 679 - 16 && packet(4) > 679 - 16, "{rate}");
            assert!(packet(5) <= 1021 - 16 && packet(6) > 1021 - 16, "{rate}");
        }
    }

    #[test]
    fn sbc_bitpool_within_the_sink_range() {
        // (sink min, sink max, quality) -> bitpool
        for (min, max, quality, want) in [
            (0, 0, Quality::High, 53), // not known
            (2, 53, Quality::High, 53),
            (2, 35, Quality::High, 35),
            (2, 35, Quality::Standard, 35),
            (2, 35, Quality::Mobile, 19),
            (2, 250, Quality::High, 53),
            (25, 40, Quality::Mobile, 25),
        ] {
            let mut c = config(48000, 2, quality);
            c.peer.sbc_min_bitpool = min;
            c.peer.sbc_max_bitpool = max;
            let mut e = SbcEncoder::new(&c).unwrap();
            assert_eq!(e.bitpool(), want, "{min}-{max} {quality:?}");
            let mut out = vec![0u8; e.frame_len()];
            e.encode(Pcm::I16(&[0; 256]), &mut out).unwrap();
            assert_eq!(out[2], want);
        }
        // 35: 83-byte frames, as Standard
        let mut c = config(48000, 2, Quality::High);
        c.peer.sbc_max_bitpool = 35;
        assert_eq!(SbcEncoder::new(&c).unwrap().frame_len(), 83);
    }

    #[test]
    fn xq_bitpool_capped_by_the_sink() {
        let mut c = config(44100, 2, Quality::High);
        c.peer.sbc_max_bitpool = 45;
        let e = SbcEncoder::new_xq(&c).unwrap();
        assert_eq!((e.bitpool(), e.frame_len()), (45, 192));
        c.peer.sbc_max_bitpool = 53;
        assert_eq!(SbcEncoder::new_xq(&c).unwrap().bitpool(), 47);
        let e = crate::create(CodecKind::SbcXq, &config(48000, 2, Quality::Standard)).unwrap();
        assert_eq!((e.kind(), e.max_output_bytes()), (CodecKind::SbcXq, 168));
        assert!(matches!(
            SbcEncoder::new_xq(&config(32000, 2, Quality::High)),
            Err(EncoderError::Unsupported { codec: CodecKind::SbcXq, .. })
        ));
    }

    fn with_params(rate: u32, ch: u32, quality: Quality, sbc: SbcParams, bitpool: u8) -> EncoderConfig {
        EncoderConfig {
            peer: PeerParams { sbc, sbc_bitpool: bitpool, ..Default::default() },
            ..config(rate, ch, quality)
        }
    }

    #[test]
    fn frame_length_and_bitrate_of_other_setups() {
        // (rate, blocks, subbands, mode, bitpool, frame length, kbps): the
        // A2DP specification's formula (12.9)
        use ChannelMode::*;
        let cases = [
            (44100, 16, 8, JointStereo, 53, 119, 327),
            (44100, 16, 8, Stereo, 53, 118, 325),
            (44100, 16, 8, DualChannel, 32, 140, 385),
            (44100, 16, 8, Mono, 31, 70, 192),
            (44100, 8, 4, JointStereo, 35, 44, 485),
            (48000, 8, 4, Stereo, 35, 43, 516),
            (44100, 4, 8, JointStereo, 53, 40, 441),
            (48000, 12, 8, JointStereo, 53, 93, 372),
            (48000, 12, 4, DualChannel, 20, 68, 544),
            (44100, 4, 4, Mono, 30, 21, 463),
        ];
        for (rate, blocks, subbands, mode, bitpool, len, kbps) in cases {
            let c = SbcConfig {
                sample_rate: rate,
                channel_mode: mode,
                blocks,
                subbands,
                allocation: Allocation::Snr,
                bitpool,
            };
            assert_eq!(c.frame_len(), len, "{rate} {blocks}x{subbands} {mode:?} {bitpool}");
            assert_eq!(bitrate_kbps(len, rate, c.pcm_frames_per_frame()), kbps, "{rate} {blocks}x{subbands} {mode:?}");
            // The encoder agrees
            let p = SbcParams { blocks, subbands, allocation: None, channel_mode: Some(mode) };
            let e = SbcEncoder::new(&with_params(rate, 2, Quality::High, p, bitpool)).unwrap();
            assert_eq!((e.frame_len(), e.bitrate_kbps()), (len, kbps), "{rate} {blocks}x{subbands} {mode:?}");
        }
    }

    #[test]
    fn every_block_and_subband_count_and_allocation_encodes() {
        let mut seed = 7u32;
        for blocks in [4u8, 8, 12, 16] {
            for subbands in [4u8, 8] {
                for allocation in [Allocation::Loudness, Allocation::Snr] {
                    for (ch, mode) in [
                        (2, ChannelMode::JointStereo),
                        (2, ChannelMode::Stereo),
                        (2, ChannelMode::Mono),
                        (1, ChannelMode::Mono),
                    ] {
                        let p = SbcParams { blocks, subbands, allocation: Some(allocation), channel_mode: Some(mode) };
                        let mut e = SbcEncoder::new(&with_params(48000, ch, Quality::Standard, p, 0)).unwrap();
                        let what = format!("{blocks}x{subbands} {allocation:?} {ch} ch {mode:?}");
                        let frames = blocks as usize * subbands as usize;
                        assert_eq!(e.pcm_frames_per_encode(), frames, "{what}");
                        let len = e.frame_len();
                        let want = SbcConfig {
                            sample_rate: 48000,
                            channel_mode: mode,
                            blocks,
                            subbands,
                            allocation,
                            bitpool: 35,
                        }
                        .frame_len();
                        assert_eq!(len, want, "{what}");
                        let mut out = vec![0u8; len];
                        for _ in 0..8 {
                            let pcm: Vec<i16> = (0..frames * ch as usize)
                                .map(|_| {
                                    seed ^= seed << 13;
                                    seed ^= seed >> 17;
                                    seed ^= seed << 5;
                                    (seed >> 3) as i16
                                })
                                .collect();
                            let r = e.encode(Pcm::I16(&pcm), &mut out).unwrap();
                            assert_eq!(r, Encoded { bytes: len, frames: 1 }, "{what}");
                            // Header: 48 kHz, blocks, mode, allocation, subbands
                            let b =
                                (blocks / 4 - 1) << 4 | (mode as u8) << 2 | (allocation as u8) << 1 | (subbands / 8);
                            assert_eq!((out[0], out[1], out[2]), (0x9C, 0xC0 | b, 35), "{what}");
                        }
                        // The wrong input length is refused
                        assert!(matches!(
                            e.encode(Pcm::I16(&vec![0; frames * ch as usize + 2]), &mut out),
                            Err(EncoderError::InputLength { .. })
                        ));
                    }
                }
            }
        }
    }

    #[test]
    fn mono_from_stereo_mixes_down() {
        let p = SbcParams { channel_mode: Some(ChannelMode::Mono), ..Default::default() };
        let mut mixed = SbcEncoder::new(&with_params(44100, 2, Quality::High, p, 0)).unwrap();
        let mut mono = SbcEncoder::new(&config(44100, 1, Quality::High)).unwrap();
        assert_eq!((mixed.frame_len(), mixed.pcm_frames_per_encode()), (mono.frame_len(), 128));
        let (mut a, mut b) = (vec![0u8; mono.frame_len()], vec![0u8; mono.frame_len()]);
        for n in 0..4 {
            let m: Vec<i16> = (0..128).map(|i| ((i * 977 + n * 31) % 16000) as i16 - 8000).collect();
            let lr: Vec<i16> = m.iter().flat_map(|&s| [s, s]).collect();
            mixed.encode(Pcm::I16(&lr), &mut a).unwrap();
            mono.encode(Pcm::I16(&m), &mut b).unwrap();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn auto_bitpool_by_channel_mode() {
        // Dual channel halves the joint stereo bitpool per channel: about
        // the same bit rate
        let dual = SbcParams { channel_mode: Some(ChannelMode::DualChannel), ..Default::default() };
        let e = SbcEncoder::new(&with_params(44100, 2, Quality::High, dual, 0)).unwrap();
        assert_eq!((e.bitpool(), e.frame_len(), e.bitrate_kbps()), (27, 120, 330));
        assert_eq!(auto_bitpool(Quality::Mobile, ChannelMode::DualChannel), 10);
        assert_eq!(auto_bitpool(Quality::Standard, ChannelMode::Stereo), 35);
        // SBC XQ keeps its dual channel and bitpools whatever the mode asked
        let stereo = SbcParams { channel_mode: Some(ChannelMode::Stereo), ..Default::default() };
        let e = SbcEncoder::new_xq(&with_params(44100, 2, Quality::High, stereo, 0)).unwrap();
        assert_eq!((e.bitpool(), e.frame_len()), (47, 200));
        // ... but its frame setup
        let small = SbcParams { blocks: 8, ..Default::default() };
        let e = SbcEncoder::new_xq(&with_params(44100, 2, Quality::High, small, 0)).unwrap();
        assert_eq!((e.frame_len(), e.pcm_frames_per_encode()), (4 + 8 + 94, 64));
    }

    #[test]
    fn keys() {
        for m in ChannelMode::ALL {
            assert_eq!(ChannelMode::from_key(m.key()), Some(m));
        }
        assert_eq!(Allocation::from_key("snr"), Some(Allocation::Snr));
        assert_eq!(Allocation::from_key(""), None);
        assert_eq!(ChannelMode::from_key("auto"), None);
        let d = SbcParams::default();
        assert_eq!((d.blocks(), d.subbands(), d.allocation()), (16, 8, Allocation::Loudness));
        assert_eq!(d.channel_mode(CodecKind::Sbc, 2), ChannelMode::JointStereo);
        assert_eq!(d.channel_mode(CodecKind::SbcXq, 2), ChannelMode::DualChannel);
        assert_eq!(d.channel_mode(CodecKind::Sbc, 1), ChannelMode::Mono);
    }

    #[test]
    fn frame_config_validation() {
        let ok = SbcConfig {
            sample_rate: 16000,
            channel_mode: ChannelMode::DualChannel,
            blocks: 12,
            subbands: 4,
            allocation: Allocation::Snr,
            bitpool: 200,
        };
        let e = SbcFrameEncoder::new(ok).unwrap();
        assert_eq!(ok.effective_bitpool(), 64);
        assert_eq!(e.pcm_samples(), 96);
        assert!(SbcFrameEncoder::new(SbcConfig { blocks: 6, ..ok }).is_err());
        assert!(SbcFrameEncoder::new(SbcConfig { subbands: 6, ..ok }).is_err());
        assert!(SbcFrameEncoder::new(SbcConfig { sample_rate: 22050, ..ok }).is_err());
    }
}
