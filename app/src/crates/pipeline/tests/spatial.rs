//! The spatial stage against a real engine, when one is pointed at.
//!
//! The engine path comes from the environment (the engine is not part of this
//! repository): `A2DPWB_SPATIAL_ENGINE` points at the executable, whose assets
//! sit next to it. Without it the test reports that it was skipped.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use codec::{CodecKind, Encoded, Encoder, EncoderError, Pcm, PcmFormat};
use pipeline::convert::SampleType;
use pipeline::spatial::{SpatialConfig, SpatialEngine};
use pipeline::{BufferPreset, InputFormat, Pipeline, PipelineConfig};

/// One engine block: 1024 frames of a 440 Hz tone in stereo, float32
fn tone_block() -> Vec<u8> {
    let mut out = Vec::with_capacity(1024 * 2 * 4);
    for n in 0..1024 {
        let s = (2.0 * std::f64::consts::PI * 440.0 * n as f64 / 48000.0).sin() as f32 * 0.25;
        out.extend_from_slice(&s.to_le_bytes());
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[test]
fn stage_passes_one_block_through_the_engine() {
    let Ok(exe) = std::env::var("A2DPWB_SPATIAL_ENGINE") else {
        eprintln!("skipped: A2DPWB_SPATIAL_ENGINE is not set");
        return;
    };
    let mut engine = SpatialEngine::start(&SpatialConfig {
        exe: PathBuf::from(&exe),
        program: PathBuf::from(&exe),
        args: vec!["-".into(), "stream".into(), "1".into(), "1024".into()],
        cwd: None,
    })
    .expect("engine starts");
    let block = tone_block();
    let out = engine.process(&block, SampleType::F32, 2).expect("the engine answers");
    assert_eq!(out.len(), block.len(), "the engine returns as many bytes as it got");
    assert_ne!(out, block, "the engine changes the audio");
    assert!(engine.running());
}

#[test]
fn a_fresh_engine_gives_the_same_first_block() {
    let Ok(exe) = std::env::var("A2DPWB_SPATIAL_ENGINE") else {
        eprintln!("skipped: A2DPWB_SPATIAL_ENGINE is not set");
        return;
    };
    let block = tone_block();
    let run = || {
        let mut engine = SpatialEngine::start(&SpatialConfig {
            exe: PathBuf::from(&exe),
            program: PathBuf::from(&exe),
            args: vec!["-".into(), "stream".into(), "1".into(), "1024".into()],
            cwd: None,
        })
        .expect("engine starts");
        engine.process(&block, SampleType::F32, 2).expect("the engine answers")
    };
    assert_eq!(run(), run(), "the same input gives the same output");
}

#[test]
fn a_partial_block_waits_for_the_rest() {
    let Ok(exe) = std::env::var("A2DPWB_SPATIAL_ENGINE") else {
        eprintln!("skipped: A2DPWB_SPATIAL_ENGINE is not set");
        return;
    };
    let mut engine = SpatialEngine::start(&SpatialConfig {
        exe: PathBuf::from(&exe),
        program: PathBuf::from(&exe),
        args: vec!["-".into(), "stream".into(), "1".into(), "1024".into()],
        cwd: None,
    })
    .expect("engine starts");
    let block = tone_block();
    let half = block.len() / 2;
    let out = engine.process(&block[..half], SampleType::F32, 2).expect("the engine answers");
    assert!(out.is_empty(), "half a block produces no audio yet");
    let out = engine.process(&block[half..], SampleType::F32, 2).expect("the engine answers");
    assert_eq!(out.len(), block.len());
}

/// An encoder that keeps what it was given
struct Recording(Arc<Mutex<Vec<i16>>>);

impl Encoder for Recording {
    fn kind(&self) -> CodecKind {
        CodecKind::Ldac
    }
    fn pcm_format(&self) -> PcmFormat {
        PcmFormat::I16
    }
    fn pcm_frames_per_encode(&self) -> usize {
        1024
    }
    fn max_output_bytes(&self) -> usize {
        16
    }
    fn encode(&mut self, pcm: Pcm<'_>, out: &mut [u8]) -> Result<Encoded, EncoderError> {
        if let Pcm::I16(samples) = pcm {
            self.0.lock().expect("recorder").extend_from_slice(samples);
        }
        out[0] = 0;
        Ok(Encoded { bytes: 1, frames: 1 })
    }
    fn bitrate_kbps(&self) -> u32 {
        0
    }
    fn abr_adjust(&mut self, _queue_depth: u32) {}
    fn abr_active(&self) -> bool {
        false
    }
    fn abr_reset(&mut self) {}
}

/// The whole path: capture ring -> spatial engine -> encoder
#[test]
fn the_encoder_gets_the_engine_output() {
    let Ok(exe) = std::env::var("A2DPWB_SPATIAL_ENGINE") else {
        eprintln!("skipped: A2DPWB_SPATIAL_ENGINE is not set");
        return;
    };
    let seen = Arc::new(Mutex::new(Vec::new()));
    let stats = Arc::new(bt_host::LinkStats::default());
    let (sink, _consumer, _streaming) = bt_host::media::detached_queue(stats.clone());
    let config = PipelineConfig {
        encoder: Box::new(Recording(seen.clone())),
        input: InputFormat { sample_rate: 48000, channels: 2, sample: SampleType::F32 },
        channels: 2,
        mtu: 600,
        abr: false,
        buffer: BufferPreset::Standard,
        spatial: Some(SpatialConfig {
            exe: PathBuf::from(&exe),
            program: PathBuf::from(&exe),
            args: vec!["-".into(), "stream".into(), "1".into(), "1024".into()],
            cwd: None,
        }),
        resample: None,
    };
    let (_pipeline, mut input) = Pipeline::start(config, sink, stats);
    let block = tone_block();
    input.push(&block);
    // The engine works in blocks of 1024 frames: give it a few
    for _ in 0..4 {
        input.push(&block);
    }
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while seen.lock().expect("recorder").is_empty() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let got = seen.lock().expect("recorder").clone();
    assert!(!got.is_empty(), "the encoder received audio");
    // What the encoder got is the engine's output, not the tone we pushed
    let tone_as_i16: Vec<i16> = tone_block().chunks(4).map(|s| i16::from_le_bytes([s[0], s[1]])).collect();
    assert_ne!(got, tone_as_i16, "the encoder did not get the raw tone");
}

/// A stage that is a program of its own (the Upmixer stage: a Python runtime
/// with a native library) is started with the command its stage names.
#[test]
fn a_stage_with_its_own_command_runs() {
    let Ok(program) = std::env::var("A2DPWB_SPATIAL_PROGRAM") else {
        eprintln!("skipped: A2DPWB_SPATIAL_PROGRAM is not set");
        return;
    };
    let args: Vec<String> = std::env::var("A2DPWB_SPATIAL_ARGS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let cwd = std::env::var("A2DPWB_SPATIAL_CWD").ok().map(std::path::PathBuf::from);
    let mut engine = SpatialEngine::start(&SpatialConfig {
        exe: std::path::PathBuf::from(&program),
        program: std::path::PathBuf::from(&program),
        args,
        cwd,
    })
    .expect("the stage starts");
    let block = tone_block();
    let out = engine.process(&block, SampleType::F32, 2).expect("the stage answers");
    assert_eq!(out.len(), block.len(), "the stage returns as many bytes as it got");
    assert_ne!(out, block, "the stage changes the audio");
}
