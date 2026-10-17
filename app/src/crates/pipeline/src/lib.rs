//! The audio path from capture to the Bluetooth thread:
//!
//! capture thread --[`CaptureInput`] (SPSC byte ring)--> encode thread
//! --[`bt_host::MediaSink`] (SPSC packet ring)--> BTstack thread
//!
//! The encode thread converts the captured format to what the encoder takes,
//! downmixes, encodes in whole encoder blocks (keeping the rest for the next
//! round), packs codec frames into media payloads and queues them. It works
//! in rounds of about 10 ms of audio so packets go out evenly instead of in
//! bursts. No allocation and no lock on this path once a stream runs.

#![forbid(unsafe_code)]

pub mod convert;
pub mod le;
pub mod packetizer;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{JoinHandle, Thread};
use std::time::{Duration, Instant};

use bt_host::{LinkStats, MediaSink};
use codec::{CodecKind, Encoder, Pcm, PcmFormat};

pub use convert::SampleType;
use packetizer::Packetizer;

/// Format of the captured audio as it arrives
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample: SampleType,
}

impl InputFormat {
    pub fn frame_bytes(&self) -> usize {
        self.channels as usize * self.sample.bytes()
    }
}

/// Depth of the capture ring (how long the encoder may fall behind before
/// audio is dropped). Latency is set by how full it runs, which stays low
/// while the encoder keeps up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BufferPreset {
    LowLatency,
    #[default]
    Standard,
    Stable,
}

impl BufferPreset {
    fn ring_bytes(self, input: &InputFormat) -> usize {
        let per_second = input.sample_rate as usize * input.frame_bytes();
        match self {
            BufferPreset::LowLatency => per_second / 5,
            // v1's fixed 256 KiB, or 200 ms if that is more
            BufferPreset::Standard => (per_second / 5).max(256 * 1024),
            BufferPreset::Stable => per_second,
        }
    }
}

pub struct PipelineConfig {
    pub encoder: Box<dyn Encoder>,
    pub input: InputFormat,
    /// Channels sent (1 or 2); input channels beyond are dropped
    pub channels: u32,
    /// Media payload size (RTP header excluded)
    pub mtu: u16,
    pub abr: bool,
    pub buffer: BufferPreset,
}

/// The capture thread's end: copies captured audio into the ring
pub struct CaptureInput {
    producer: rtrb::Producer<u8>,
    encode_thread: Thread,
    stats: Arc<LinkStats>,
    frame_bytes: usize,
}

impl CaptureInput {
    /// Captured audio (whole frames of the pipeline's input format). Dropped
    /// and counted when the encoder fell behind and the ring is full.
    pub fn push(&mut self, data: &[u8]) {
        let frames = (data.len() / self.frame_bytes) as u64;
        self.stats.capture_frames.fetch_add(frames, Ordering::Relaxed);
        if !self.write(data) {
            self.stats.capture_dropped_frames.fetch_add(frames, Ordering::Relaxed);
        }
    }

    /// Silence the capture inserted while nothing played: encoded like
    /// audio, but not counted as audio (AVRCP playback status)
    pub fn push_silence(&mut self, frames: usize) {
        let mut left = frames * self.frame_bytes;
        self.stats.silence_frames.fetch_add(frames as u64, Ordering::Relaxed);
        while left > 0 {
            let n = left.min(self.producer.slots());
            if n == 0 {
                break;
            }
            if let Ok(mut chunk) = self.producer.write_chunk(n) {
                let (a, b) = chunk.as_mut_slices();
                a.fill(0);
                b.fill(0);
                chunk.commit_all();
            }
            left -= n;
        }
        self.encode_thread.unpark();
    }

    fn write(&mut self, data: &[u8]) -> bool {
        let Ok(mut chunk) = self.producer.write_chunk(data.len()) else { return false };
        let (a, b) = chunk.as_mut_slices();
        let first = a.len();
        a.copy_from_slice(&data[..first]);
        b.copy_from_slice(&data[first..]);
        chunk.commit_all();
        self.encode_thread.unpark();
        true
    }

    pub fn frame_bytes(&self) -> usize {
        self.frame_bytes
    }
}

struct Control {
    stop: AtomicBool,
    /// Drop buffered audio and restart timestamps (after a reconnect)
    reset: AtomicBool,
}

/// A running encode thread. Stopping it returns the media sink.
pub struct Pipeline {
    control: Arc<Control>,
    thread: Option<JoinHandle<MediaSink>>,
    kind: CodecKind,
    bitrate_kbps: u32,
}

impl Pipeline {
    pub fn start(config: PipelineConfig, sink: MediaSink, stats: Arc<LinkStats>) -> (Pipeline, CaptureInput) {
        let ring = config.buffer.ring_bytes(&config.input);
        let (producer, consumer) = rtrb::RingBuffer::new(ring);
        let control = Arc::new(Control { stop: AtomicBool::new(false), reset: AtomicBool::new(false) });
        let kind = config.encoder.kind();
        let bitrate_kbps = config.encoder.bitrate_kbps();
        let frame_bytes = config.input.frame_bytes();
        let thread = {
            let control = control.clone();
            std::thread::Builder::new()
                .name("encode".into())
                .spawn(move || encode_thread(config, consumer, sink, &control))
                .expect("spawning the encode thread")
        };
        let input = CaptureInput { producer, encode_thread: thread.thread().clone(), stats, frame_bytes };
        (Pipeline { control, thread: Some(thread), kind, bitrate_kbps }, input)
    }

    pub fn codec(&self) -> CodecKind {
        self.kind
    }

    pub fn bitrate_kbps(&self) -> u32 {
        self.bitrate_kbps
    }

    /// Drops audio buffered for the old stream and restarts the timestamps
    pub fn reset(&self) {
        self.control.reset.store(true, Ordering::Release);
        if let Some(t) = &self.thread {
            t.thread().unpark();
        }
    }

    /// Stops and joins the encode thread
    pub fn stop(mut self) -> Option<MediaSink> {
        self.stop_thread()
    }

    fn stop_thread(&mut self) -> Option<MediaSink> {
        let t = self.thread.take()?;
        self.control.stop.store(true, Ordering::Release);
        t.thread().unpark();
        t.join().ok()
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.stop_thread();
    }
}

/// How often ABR looks at the queue: LDAC every 100 ms (v1); LHDC V5 once
/// per packet sent, with the queue depth seen before the round (Android)
fn abr_per_packet(kind: CodecKind) -> bool {
    kind == CodecKind::LhdcV5
}

const ABR_INTERVAL: Duration = Duration::from_millis(100);

fn encode_thread(
    config: PipelineConfig,
    mut ring: rtrb::Consumer<u8>,
    mut sink: MediaSink,
    control: &Control,
) -> MediaSink {
    let PipelineConfig { mut encoder, input, channels, mtu, abr, .. } = config;
    let _priority = thread_priority();
    let kind = encoder.kind();
    let fmt = encoder.pcm_format();
    let out_ch = channels.clamp(1, 2) as usize;
    let in_ch = input.channels as usize;
    let frame_bytes = input.frame_bytes();
    let per_encode = encoder.pcm_frames_per_encode();
    let per_codec_frame = encoder.pcm_frames_per_codec_frame() as u32;
    // ~10 ms rounds, at least 128 frames (v1)
    let round_frames = (input.sample_rate as usize / 100).max(128);

    // Preallocated: one round read and converted, plus the rest of the last
    let mut read_buf = vec![0u8; round_frames * frame_bytes];
    let cap = (round_frames + per_encode) * out_ch;
    let mut pcm16: Vec<i16> = if fmt == PcmFormat::I16 { Vec::with_capacity(cap) } else { Vec::new() };
    let mut pcm32: Vec<i32> = if fmt == PcmFormat::I32 { Vec::with_capacity(cap) } else { Vec::new() };
    let mut encoded = vec![0u8; encoder.max_output_bytes().max(4096)];
    let mut packetizer = Packetizer::new(kind, mtu, input.sample_rate);
    let mut timestamp: u32 = 0;
    let mut abr_last = Instant::now();
    let mut encode_errors = 0u64;

    while !control.stop.load(Ordering::Acquire) {
        if control.reset.swap(false, Ordering::AcqRel) {
            let n = ring.slots();
            if let Ok(chunk) = ring.read_chunk(n) {
                chunk.commit_all();
            }
            pcm16.clear();
            pcm32.clear();
            timestamp = 0;
            packetizer.reset();
        }
        let available = ring.slots() / frame_bytes;
        if available == 0 {
            std::thread::park_timeout(Duration::from_millis(10));
            continue;
        }
        let frames = available.min(round_frames);
        let bytes = frames * frame_bytes;
        if let Ok(chunk) = ring.read_chunk(bytes) {
            let (a, b) = chunk.as_slices();
            read_buf[..a.len()].copy_from_slice(a);
            read_buf[a.len()..bytes].copy_from_slice(b);
            chunk.commit_all();
        }
        let raw = &read_buf[..bytes];
        match fmt {
            PcmFormat::I16 => convert::append_i16(raw, input.sample, in_ch, out_ch, &mut pcm16),
            PcmFormat::I32 => convert::append_i32(raw, input.sample, in_ch, out_ch, &mut pcm32),
        }

        let queue_before = sink.depth();
        let mut sent = 0u32;
        let block = per_encode * out_ch;
        let total = if fmt == PcmFormat::I16 { pcm16.len() } else { pcm32.len() };
        let mut offset = 0;
        while offset + block <= total {
            let pcm = match fmt {
                PcmFormat::I16 => Pcm::I16(&pcm16[offset..offset + block]),
                PcmFormat::I32 => Pcm::I32(&pcm32[offset..offset + block]),
            };
            match encoder.encode(pcm, &mut encoded) {
                Ok(e) if e.bytes > 0 => {
                    sent += packetizer.push(&encoded[..e.bytes], e.frames, timestamp, &mut |p| {
                        sink.send(p);
                    });
                    timestamp = timestamp.wrapping_add(e.frames * per_codec_frame);
                }
                Ok(_) => {}
                Err(e) => {
                    encode_errors += 1;
                    if encode_errors == 1 || encode_errors.is_multiple_of(1000) {
                        tracing::error!("encode thread: {e} ({encode_errors} errors)");
                    }
                }
            }
            offset += block;
        }
        sent += packetizer.end_round(&mut |p| {
            sink.send(p);
        });
        // Keep the rest (less than one encoder block) for the next round
        match fmt {
            PcmFormat::I16 => {
                pcm16.copy_within(offset.., 0);
                pcm16.truncate(total - offset);
            }
            PcmFormat::I32 => {
                pcm32.copy_within(offset.., 0);
                pcm32.truncate(total - offset);
            }
        }

        if abr {
            if abr_per_packet(kind) {
                for _ in 0..sent {
                    encoder.abr_adjust(queue_before);
                }
            } else if abr_last.elapsed() >= ABR_INTERVAL {
                abr_last = Instant::now();
                encoder.abr_adjust(sink.depth());
            }
        }
    }
    sink
}

/// The encode thread raises its priority (MMCSS "Pro Audio" on Windows, as
/// v1) through a hook core installs, so this crate stays free of Win32.
/// What the hook returns is dropped when the thread ends (e.g. a guard that
/// reverts the registration).
pub type ThreadPriorityHook = fn() -> Box<dyn std::any::Any>;

static PRIORITY_HOOK: std::sync::OnceLock<ThreadPriorityHook> = std::sync::OnceLock::new();

pub fn set_thread_priority_hook(hook: ThreadPriorityHook) {
    let _ = PRIORITY_HOOK.set(hook);
}

fn thread_priority() -> Option<Box<dyn std::any::Any>> {
    PRIORITY_HOOK.get().map(|h| h())
}
