//! Placeholder: being implemented

use crate::{CodecKind, Encoded, Encoder, EncoderConfig, EncoderError, Pcm, PcmFormat};

pub struct AptxEncoder(CodecKind);

impl AptxEncoder {
    pub fn new(kind: CodecKind, config: &EncoderConfig) -> Result<AptxEncoder, EncoderError> {
        let _ = config;
        Err(EncoderError::NotAvailable(kind))
    }
}

impl Encoder for AptxEncoder {
    fn kind(&self) -> CodecKind {
        self.0
    }
    fn pcm_format(&self) -> PcmFormat {
        PcmFormat::I16
    }
    fn pcm_frames_per_encode(&self) -> usize {
        4
    }
    fn max_output_bytes(&self) -> usize {
        0
    }
    fn encode(&mut self, _pcm: Pcm<'_>, _out: &mut [u8]) -> Result<Encoded, EncoderError> {
        Err(EncoderError::NotAvailable(self.0))
    }
    fn bitrate_kbps(&self) -> u32 {
        0
    }
}
