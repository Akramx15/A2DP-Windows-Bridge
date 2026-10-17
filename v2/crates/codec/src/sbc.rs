//! Placeholder: being implemented

use crate::{CodecKind, Encoded, Encoder, EncoderConfig, EncoderError, Pcm, PcmFormat};

pub struct SbcEncoder;

impl SbcEncoder {
    pub fn new(config: &EncoderConfig) -> Result<SbcEncoder, EncoderError> {
        let _ = config;
        Err(EncoderError::NotAvailable(CodecKind::Sbc))
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
        128
    }
    fn max_output_bytes(&self) -> usize {
        0
    }
    fn encode(&mut self, _pcm: Pcm<'_>, _out: &mut [u8]) -> Result<Encoded, EncoderError> {
        Err(EncoderError::NotAvailable(CodecKind::Sbc))
    }
    fn bitrate_kbps(&self) -> u32 {
        0
    }
}
