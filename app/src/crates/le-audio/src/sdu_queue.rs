//! The jitter buffer of SDU sets and the send guard (v1 Stream in
//! le_audio_stream.cpp).
//!
//! One SDU set is one SDU per CIS for the same frame interval (10 or 7.5
//! ms), in plan order. The encode thread (pipeline) pushes sets; the
//! BTstack thread (bt-host iso_send) pops one per round on
//! HCI_EVENT_CIS_CAN_SEND_NOW. bt-host shares the queue between the two
//! behind a mutex (`LeSduSink`).
//!
//! Buffering: before audio starts and after an underrun the queue holds
//! sets back until it has `prefill` of them: queue depth + 3 at first, one
//! more after each underrun up to queue depth + 7. A full queue
//! ([`CAPACITY`]) drops the oldest set. While nothing can be popped the
//! sender sends the encoded silence set ([`SduQueue::silence`]).
//!
//! The guard keeps the sent sets at most queue depth + 2 intervals ahead
//! of real time (the controller queue takes queue-depth sets at once).

// Step 0 skeleton: remove once the bodies are written (WP3)
#![allow(unused_variables, dead_code)]

use std::collections::VecDeque;

/// One SDU per CIS, in plan order
pub type SduSet = Vec<Vec<u8>>;

/// Sets the queue holds at most (the oldest is dropped beyond)
pub const CAPACITY: usize = 16;

/// Counters of the queue (part of bt-host's LeStreamStats)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueueStats {
    /// Sets pushed (from captured audio)
    pub sets_pushed: u64,
    /// Sets popped (audio, not silence)
    pub sets_popped: u64,
    /// Pops answered with silence (buffer empty or still prefilling)
    pub silence_sets: u64,
    /// Times the buffer ran empty while audio flowed
    pub underruns: u64,
    /// Sets dropped because the buffer was full
    pub overflows: u64,
}

/// The jitter buffer
#[derive(Debug, Clone, Default)]
pub struct SduQueue {
    sets: VecDeque<SduSet>,
    silence: SduSet,
    buffering: bool,
    prefill: usize,
    prefill_max: usize,
    stats: QueueStats,
}

impl SduQueue {
    pub fn new() -> SduQueue {
        todo!("Step 0 skeleton")
    }

    /// For a new stream: empties the queue, resets the counters, sets the
    /// silence set (one encoded silent SDU per CIS) and the prefill for the
    /// controller queue depth (SDUs queued per CIS)
    pub fn configure(&mut self, queue_depth: u8, silence: SduSet) {
        todo!("Step 0 skeleton")
    }

    /// Encode thread: a new set (drops the oldest when full)
    pub fn push(&mut self, set: SduSet) {
        todo!("Step 0 skeleton")
    }

    /// BTstack thread: the next set, or None while buffering / empty (an
    /// underrun when audio was flowing: the prefill grows by one up to its
    /// maximum). The caller sends [`SduQueue::silence`] for None.
    pub fn pop(&mut self) -> Option<SduSet> {
        todo!("Step 0 skeleton")
    }

    pub fn silence(&self) -> &SduSet {
        &self.silence
    }

    /// Sets waiting
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// Sets held back before audio (re)starts
    pub fn prefill(&self) -> usize {
        self.prefill
    }

    pub fn clear(&mut self) {
        todo!("Step 0 skeleton")
    }

    pub fn stats(&self) -> QueueStats {
        self.stats
    }
}

/// Keeps the sending at most `lead` intervals ahead of real time (v1
/// Stream's rounds_ / lead_ / t0_ms_)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SendGuard {
    frame_us: u32,
    lead: u64,
    rounds: u64,
}

impl SendGuard {
    /// `lead` = queue depth + 2
    pub fn new(frame_us: u32, lead: u64) -> SendGuard {
        todo!("Step 0 skeleton")
    }

    /// May the next round start `elapsed_us` after the first one?
    pub fn may_send(&self, elapsed_us: u64) -> bool {
        todo!("Step 0 skeleton")
    }

    /// When the next round may start (microseconds after the first one)
    pub fn next_due_us(&self) -> u64 {
        todo!("Step 0 skeleton")
    }

    /// A round (one set to every CIS still up) was started
    pub fn round_started(&mut self) {
        todo!("Step 0 skeleton")
    }

    /// Rounds started
    pub fn rounds(&self) -> u64 {
        self.rounds
    }
}
