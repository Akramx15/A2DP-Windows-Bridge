//! Placeholder: being implemented

use crate::{CodecKind, Encoded, Encoder, EncoderConfig, EncoderError, Pcm, PcmFormat};

pub struct LhdcV5Encoder;

impl LhdcV5Encoder {
    pub fn new(config: &EncoderConfig) -> Result<LhdcV5Encoder, EncoderError> {
        let _ = config;
        Err(EncoderError::NotAvailable(CodecKind::LhdcV5))
    }
}

impl Encoder for LhdcV5Encoder {
    fn kind(&self) -> CodecKind {
        CodecKind::LhdcV5
    }
    fn pcm_format(&self) -> PcmFormat {
        PcmFormat::I16
    }
    fn pcm_frames_per_encode(&self) -> usize {
        128
    }
    fn max_output_bytes(&self) -> usize {
        0
    }
    fn encode(&mut self, _pcm: Pcm<'_>, _out: &mut [u8]) -> Result<Encoded, EncoderError> {
        Err(EncoderError::NotAvailable(CodecKind::LhdcV5))
    }
    fn bitrate_kbps(&self) -> u32 {
        0
    }
}
