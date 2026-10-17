//! liblc3 (Google, Apache-2.0), built from
//! extern/btstack/3rd-party/lc3-google.
//!
//! `ffi` (to come with the build, WP3) has hand-written declarations
//! matching include/lc3.h (only what A2DPWB uses: lc3_frame_samples,
//! lc3_encoder_size, lc3_setup_encoder, lc3_encode). [`Lc3Encoder`] owns the
//! encoder memory and passes slice lengths along, so that callers need no
//! `unsafe`. One encoder per LC3 channel (v1 Stream: one
//! btstack_lc3_encoder_google_t per output channel).

// Step 0 skeleton: remove once the bodies are written (WP3)
#![allow(unused_variables, dead_code)]

use std::fmt;

/// Why an LC3 call failed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Frame duration / sample rate liblc3 does not support
    BadParameters,
    /// The PCM slice is shorter than one frame (with the stride), or the
    /// output length is outside 20..=400 octets
    BadLength,
    /// lc3_encode returned an error
    Encode(i32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadParameters => f.write_str("unsupported LC3 frame duration or sample rate"),
            Error::BadLength => f.write_str("bad LC3 frame length"),
            Error::Encode(code) => write!(f, "LC3 encoding failed ({code})"),
        }
    }
}

impl std::error::Error for Error {}

/// An LC3 encoder of one channel
pub struct Lc3Encoder {
    frame_us: u32,
    sample_rate: u32,
}

impl Lc3Encoder {
    /// `frame_us`: 7500 or 10000; `sample_rate`: 8000, 16000, 24000, 32000
    /// or 48000 Hz (the LC3 rate; the PCM must already be at it)
    pub fn new(frame_us: u32, sample_rate: u32) -> Result<Lc3Encoder, Error> {
        todo!("Step 0 skeleton")
    }

    pub fn frame_us(&self) -> u32 {
        self.frame_us
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// PCM samples of one frame (e.g. 480 at 48 kHz, 10 ms)
    pub fn frame_samples(&self) -> usize {
        todo!("Step 0 skeleton")
    }

    /// Encodes one frame of 16-bit PCM into `out` (its length is the octets
    /// per frame of the preset). `pcm` holds the channel's samples every
    /// `stride` samples (1 for mono data, 2 for one channel of interleaved
    /// stereo, starting at that channel's first sample).
    pub fn encode(&mut self, pcm: &[i16], stride: usize, out: &mut [u8]) -> Result<(), Error> {
        todo!("Step 0 skeleton")
    }
}

impl fmt::Debug for Lc3Encoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lc3Encoder").field("frame_us", &self.frame_us).field("sample_rate", &self.sample_rate).finish()
    }
}
