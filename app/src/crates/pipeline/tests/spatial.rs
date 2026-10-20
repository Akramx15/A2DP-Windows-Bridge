//! The spatial stage against a real engine, when one is pointed at.
//!
//! The engine path comes from the environment (the engine is not part of this
//! repository): `A2DPWB_SPATIAL_ENGINE` points at the executable, whose assets
//! sit next to it. Without it the test reports that it was skipped.

use std::path::PathBuf;

use pipeline::convert::SampleType;
use pipeline::spatial::{SpatialConfig, SpatialEngine};

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
    let mut engine = SpatialEngine::start(&SpatialConfig { exe: PathBuf::from(exe) }).expect("engine starts");
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
        let mut engine = SpatialEngine::start(&SpatialConfig { exe: PathBuf::from(&exe) }).expect("engine starts");
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
    let mut engine = SpatialEngine::start(&SpatialConfig { exe: PathBuf::from(exe) }).expect("engine starts");
    let block = tone_block();
    let half = block.len() / 2;
    let out = engine.process(&block[..half], SampleType::F32, 2).expect("the engine answers");
    assert!(out.is_empty(), "half a block produces no audio yet");
    let out = engine.process(&block[half..], SampleType::F32, 2).expect("the engine answers");
    assert_eq!(out.len(), block.len());
}
