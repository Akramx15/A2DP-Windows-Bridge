//! Link statistics: cumulative counters written by the audio threads and the
//! BTstack thread without locks, read a few times a second by core (which
//! works with deltas, so nothing is ever reset).

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Capacity of the media send queue, in packets (as v1)
pub const MEDIA_QUEUE_PACKETS: usize = 64;

#[derive(Debug, Default)]
pub struct LinkStats {
    /// Media packets handed to L2CAP and their L2CAP payload bytes (RTP
    /// header, media payload header and codec data)
    pub packets_sent: AtomicU64,
    pub bytes_sent: AtomicU64,
    /// Packets lost before the radio: send queue full / L2CAP refused
    pub queue_drops: AtomicU64,
    pub send_errors: AtomicU64,
    /// PCM frames captured from Windows (real audio, not silence fill), and
    /// those dropped because the encoder fell behind
    pub capture_frames: AtomicU64,
    pub capture_dropped_frames: AtomicU64,
    /// Silence frames inserted while nothing played
    pub silence_frames: AtomicU64,
    /// Instantaneous send queue fill (packets)
    pub queue_depth: AtomicU32,
    /// The counters above when the current stream started; stream_starts
    /// changes with every stream (written after them)
    pub start_packets: AtomicU64,
    pub start_bytes: AtomicU64,
    pub start_dropped: AtomicU64,
    pub start_capture_frames: AtomicU64,
    pub start_capture_dropped: AtomicU64,
    pub stream_starts: AtomicU32,
    /// Radio readings of the link (BTstack thread, every few seconds)
    pub radio: Mutex<LinkRadio>,
}

impl LinkStats {
    pub(crate) fn mark_stream_start(&self) {
        let r = Ordering::Relaxed;
        self.start_packets.store(self.packets_sent.load(r), r);
        self.start_bytes.store(self.bytes_sent.load(r), r);
        self.start_dropped.store(self.queue_drops.load(r) + self.send_errors.load(r), r);
        self.start_capture_frames.store(self.capture_frames.load(r), r);
        self.start_capture_dropped.store(self.capture_dropped_frames.load(r), r);
        self.stream_starts.fetch_add(1, Ordering::Release);
    }

    pub fn radio(&self) -> LinkRadio {
        self.radio.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub(crate) fn with_radio(&self, f: impl FnOnce(&mut LinkRadio)) {
        f(&mut self.radio.lock().unwrap_or_else(|e| e.into_inner()));
    }
}

/// Radio state of the current link
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkRadio {
    /// BR/EDR RSSI: relative to the golden receive range, not dBm
    pub rssi: Option<i8>,
    /// Channels the host classification leaves usable (None = none passed)
    pub afh_usable: Option<u32>,
    /// Wi-Fi channels passed as bad, "6, 11"
    pub afh_avoided: String,
    /// Channels the link hops over (Read AFH Channel Map)
    pub afh_in_use: Option<u32>,
}
