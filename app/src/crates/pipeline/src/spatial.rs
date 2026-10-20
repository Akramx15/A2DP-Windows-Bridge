//! An optional PCM processing stage between capture and the encoder.
//!
//! The spatial engine (the xm6 360 chain) is a separate program that reads
//! raw 48 kHz stereo float32 PCM from stdin and writes the processed audio
//! to stdout in blocks of 1024 frames. It runs as a child process, so the
//! engine's own assets (next to its executable) and its exactness stay the
//! engine's business.
//!
//! The stage converts whatever the capture delivered (float, 16-bit or
//! 24-in-32, any channel count) to stereo float32, hands whole engine
//! blocks to the child and returns the processed stereo float32. While the
//! first block is still filling the encoder gets less than the capture
//! produced, which is a constant one-block delay, not a growing one.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::convert::SampleType;

/// Frames the engine processes at once
pub const ENGINE_FRAMES: usize = 1024;
/// Stereo float32 bytes of one engine block
const ENGINE_BLOCK: usize = ENGINE_FRAMES * 2 * 4;

/// What the stage needs to run the engine
#[derive(Debug, Clone)]
pub struct SpatialConfig {
    /// The engine executable; its assets sit next to it
    pub exe: PathBuf,
    /// The program to start (the engine, or the interpreter of a stage that
    /// is a program itself)
    pub program: PathBuf,
    /// Its arguments
    pub args: Vec<String>,
    /// The folder it runs in
    pub cwd: Option<PathBuf>,
}

/// The engine as a child process
pub struct SpatialEngine {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    /// Captured stereo float32 not yet a full engine block
    pending: Vec<u8>,
    /// Processed stereo float32 not yet handed to the encoder
    ready: Vec<u8>,
    /// The engine died: the audio passes through unchanged
    failed: bool,
}

impl SpatialEngine {
    /// Starts the engine. The child reads the assets next to its executable
    /// ("-"), streams 1024-frame blocks and reports on stderr.
    pub fn start(cfg: &SpatialConfig) -> std::io::Result<Self> {
        let mut child = Command::new(&cfg.program)
            .args(&cfg.args)
            .current_dir(cfg.cwd.as_deref().unwrap_or_else(|| cfg.exe.parent().unwrap_or(Path::new("."))))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().map(BufReader::new);
        if let Some(err) = child.stderr.take() {
            // The engine writes its identity and result lines to stderr; drain
            // them in the background so its stderr pipe never fills up.
            std::thread::spawn(move || {
                let mut err = BufReader::new(err);
                let mut line = String::new();
                while err.read_line(&mut line).unwrap_or(0) > 0 {
                    tracing::info!("spatial engine: {}", line.trim_end());
                    line.clear();
                }
            });
        }
        Ok(SpatialEngine { child, stdin, stdout, pending: Vec::new(), ready: Vec::new(), failed: false })
    }

    /// Runs captured audio through the engine and returns the processed
    /// stereo float32 bytes (empty while the first block is still filling).
    /// None when the engine is not running: the caller then passes the
    /// audio through unchanged.
    pub fn process(&mut self, raw: &[u8], sample: SampleType, in_ch: usize) -> Option<Vec<u8>> {
        if self.failed {
            return None;
        }
        let in_ch = in_ch.max(1);
        let frames = raw.len() / (in_ch * sample.bytes());
        if frames == 0 {
            return Some(Vec::new());
        }
        self.pending.reserve(frames * 8);
        for f in 0..frames {
            for c in 0..2 {
                // Mono input reaches both channels, as the encoder's
                // downmix does; extra channels are dropped.
                let i = f * in_ch + c.min(in_ch - 1);
                let s = to_f32(raw, i, sample);
                self.pending.extend_from_slice(&s.to_le_bytes());
            }
        }
        if !self.forward() {
            return None;
        }
        Some(std::mem::take(&mut self.ready))
    }

    /// Runs interleaved stereo float32 at 48 kHz (the resampling stage feeds
    /// it) through the engine and appends the processed frames to `out`.
    /// False when the engine is not running: the caller then passes the audio
    /// through unchanged.
    pub fn process_f32(&mut self, samples: &[f32], out: &mut Vec<f32>) -> bool {
        if self.failed {
            return false;
        }
        if samples.is_empty() {
            return true;
        }
        self.pending.reserve(samples.len() * 4);
        for &s in samples {
            self.pending.extend_from_slice(&s.to_le_bytes());
        }
        if !self.forward() {
            return false;
        }
        out.reserve(self.ready.len() / 4);
        for b in self.ready.as_chunks::<4>().0 {
            out.push(f32::from_le_bytes(*b));
        }
        self.ready.clear();
        true
    }

    /// Hands whole engine blocks over and takes their processed output
    fn forward(&mut self) -> bool {
        while self.pending.len() >= ENGINE_BLOCK {
            let block: Vec<u8> = self.pending.drain(..ENGINE_BLOCK).collect();
            if !self.feed(&block) {
                return false;
            }
        }
        true
    }

    /// Drops what is in flight (the stream restarted)
    pub fn reset(&mut self) {
        self.pending.clear();
        self.ready.clear();
    }

    /// True while the engine runs
    pub fn running(&self) -> bool {
        !self.failed
    }

    fn feed(&mut self, block: &[u8]) -> bool {
        let Some(stdin) = self.stdin.as_mut() else {
            self.failed = true;
            return false;
        };
        if stdin.write_all(block).is_err() || stdin.flush().is_err() {
            tracing::error!("spatial engine: the engine stopped accepting audio");
            self.failed = true;
            return false;
        }
        let Some(stdout) = self.stdout.as_mut() else {
            self.failed = true;
            return false;
        };
        // The engine answers with the block it was given; a stage that ends
        // early (the last partial block) answers with less.
        let mut out = vec![0u8; ENGINE_BLOCK];
        let mut got = 0usize;
        while got < out.len() {
            match stdout.read(&mut out[got..]) {
                Ok(0) => break,
                Ok(n) => got += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    tracing::error!("spatial engine: {e}");
                    self.failed = true;
                    return false;
                }
            }
        }
        if got == 0 {
            tracing::info!("spatial engine: the stage ended");
            self.failed = true;
            return false;
        }
        out.truncate(got);
        self.ready.extend_from_slice(&out);
        true
    }
}

impl Drop for SpatialEngine {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One sample of the captured audio as float32
fn to_f32(raw: &[u8], i: usize, t: SampleType) -> f32 {
    let b = t.bytes();
    let s = &raw[i * b..i * b + b];
    match t {
        SampleType::F32 => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
        SampleType::I16 => i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0,
        // 24-bit audio MSB-aligned in 32 bits
        SampleType::I32 => (i32::from_le_bytes([s[0], s[1], s[2], s[3]]) >> 16) as f32 / 32768.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f32_bytes(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[test]
    fn sixteen_bit_becomes_float() {
        let raw: Vec<u8> = [0i16, 16384].iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(to_f32(&raw, 0, SampleType::I16), 0.0);
        assert!((to_f32(&raw, 1, SampleType::I16) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn twenty_four_in_thirty_two_becomes_float() {
        // 24-bit audio sits MSB-aligned in 32 bits: full scale is 0x80000000
        // (negative) and 0x7FFFFF00 (positive, the 24-bit maximum)
        let raw: Vec<u8> = [i32::MIN, 0x7FFF_FF00i32].iter().flat_map(|s| s.to_le_bytes()).collect();
        assert!((to_f32(&raw, 0, SampleType::I32) + 1.0).abs() < 1e-6);
        assert!((to_f32(&raw, 1, SampleType::I32) - 0.999_969_5).abs() < 1e-6);
    }

    #[test]
    fn float_passes_through() {
        let raw = f32_bytes(&[0.25, -0.5]);
        assert_eq!(to_f32(&raw, 0, SampleType::F32), 0.25);
        assert_eq!(to_f32(&raw, 1, SampleType::F32), -0.5);
    }
}
