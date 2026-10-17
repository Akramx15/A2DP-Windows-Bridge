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
//!   bitpool by [`Quality`].

mod analysis;
mod bit_alloc;
mod frame;
mod packing;

pub use frame::{Allocation, ChannelMode, SbcConfig, SbcFrameEncoder};

use crate::{CodecKind, Encoded, Encoder, EncoderConfig, EncoderError, Pcm, PcmFormat, Quality};

const BLOCKS: u8 = 16;
const SUBBANDS: u8 = 8;

/// SBC bitpool for a quality mode (joint stereo; mono takes the same)
pub fn quality_bitpool(quality: Quality) -> u8 {
    match quality {
        Quality::High | Quality::MediumHigh => 53,
        Quality::Standard => 35,
        Quality::Mobile => 19,
    }
}

/// The A2DP SBC encoder: one SBC frame (128 PCM frames) per `encode`
pub struct SbcEncoder {
    frame: SbcFrameEncoder,
    frame_len: usize,
    sample_rate: u32,
}

impl SbcEncoder {
    pub fn new(config: &EncoderConfig) -> Result<SbcEncoder, EncoderError> {
        let unsupported = |what: String| EncoderError::Unsupported { codec: CodecKind::Sbc, what };
        if config.sample_rate != 44100 && config.sample_rate != 48000 {
            return Err(unsupported(format!("sample rate {} Hz (44100 or 48000)", config.sample_rate)));
        }
        let channel_mode = match config.channels {
            1 => ChannelMode::Mono,
            2 => ChannelMode::JointStereo,
            n => return Err(unsupported(format!("{n} channels"))),
        };
        let frame = SbcFrameEncoder::new(SbcConfig {
            sample_rate: config.sample_rate,
            channel_mode,
            blocks: BLOCKS,
            subbands: SUBBANDS,
            allocation: Allocation::Loudness,
            bitpool: quality_bitpool(config.quality),
        })?;
        // Bitpool is never 0 here, so every frame has exactly this length
        let frame_len = frame.max_frame_len();
        Ok(SbcEncoder { frame, frame_len, sample_rate: config.sample_rate })
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
        CodecKind::Sbc
    }

    fn pcm_format(&self) -> PcmFormat {
        PcmFormat::I16
    }

    fn pcm_frames_per_encode(&self) -> usize {
        BLOCKS as usize * SUBBANDS as usize
    }

    fn max_output_bytes(&self) -> usize {
        self.frame_len
    }

    fn encode(&mut self, pcm: Pcm<'_>, out: &mut [u8]) -> Result<Encoded, EncoderError> {
        let Pcm::I16(pcm) = pcm else {
            return Err(EncoderError::Encode { codec: CodecKind::Sbc, what: "SBC takes 16-bit PCM".into() });
        };
        let bytes = self.frame.encode(pcm, out)?;
        Ok(Encoded { bytes, frames: 1 })
    }

    /// As v1: 8 * frame length * sample rate / (subbands * blocks), in kbps,
    /// rounded down
    fn bitrate_kbps(&self) -> u32 {
        (8 * self.frame_len as u32 * self.sample_rate) / (SUBBANDS as u32 * BLOCKS as u32) / 1000
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
