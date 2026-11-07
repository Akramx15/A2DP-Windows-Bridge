//! Keeps a queue between two clocks (the Bluetooth controller's and a Windows
//! audio device's) near its target level. The level is sampled at every pull;
//! its minimum over a window (the trough, just before the next burst of
//! input arrives) is compared with the target, and when it is off by more
//! than the tolerance a few samples are dropped or repeated.

/// Window over which the trough is taken, in milliseconds of pulled audio
const WINDOW_MS: usize = 200;

/// What to do with the next pull
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Adjust {
    None,
    /// Discard this many queued samples first (the queue is too full)
    Drop(usize),
    /// Repeat a sample this many times (the queue is running low)
    Duplicate(usize),
}

#[derive(Debug)]
pub(crate) struct Drift {
    target: usize,
    tolerance: usize,
    window: usize,
    /// Most samples corrected per window (0.25 % of the rate)
    max_step: usize,
    pulled: usize,
    trough: usize,
}

impl Drift {
    /// `target` and `tolerance` in samples at `rate`
    pub fn new(rate: u32, target: usize, tolerance: usize) -> Drift {
        let rate = rate as usize;
        Drift {
            target,
            tolerance,
            window: (rate * WINDOW_MS / 1000).max(1),
            max_step: (rate / 2000).max(1),
            pulled: 0,
            trough: usize::MAX,
        }
    }

    /// Starts a new window (after a re-prime: the old trough says nothing)
    pub fn reset(&mut self) {
        self.pulled = 0;
        self.trough = usize::MAX;
    }

    /// Before a pull of `n` samples with `level` samples queued
    pub fn observe(&mut self, level: usize, n: usize) -> Adjust {
        self.trough = self.trough.min(level);
        self.pulled += n;
        if self.pulled < self.window {
            return Adjust::None;
        }
        let trough = self.trough;
        self.reset();
        if trough > self.target + self.tolerance {
            let excess = trough - self.target - self.tolerance;
            Adjust::Drop((excess / 2).clamp(1, self.max_step))
        } else if trough + self.tolerance < self.target {
            let short = self.target - self.tolerance - trough;
            Adjust::Duplicate((short / 2).clamp(1, self.max_step))
        } else {
            Adjust::None
        }
    }
}

/// What one [`Primed::pull`] did
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Pulled {
    /// Samples taken from the queue into the output
    pub got: usize,
    /// Zero samples in the output (priming or underrun)
    pub zeros: usize,
    /// The queue ran dry after it had been primed
    pub underrun: bool,
    pub dropped: usize,
    pub duplicated: usize,
}

/// The consumer side of a jittery queue: silence until `prefill` samples
/// are queued (at the start and after an underrun), then the queue's
/// samples, with [`Drift`] keeping the level near the prefill
#[derive(Debug)]
pub(crate) struct Primed {
    prefill: usize,
    primed: bool,
    drift: Drift,
    last: i16,
}

impl Primed {
    pub fn new(rate: u32, prefill: usize, tolerance: usize) -> Primed {
        Primed { prefill, primed: false, drift: Drift::new(rate, prefill, tolerance), last: 0 }
    }

    /// Fills all of `out`
    pub fn pull(&mut self, queue: &mut rtrb::Consumer<i16>, out: &mut [i16]) -> Pulled {
        let mut r = Pulled::default();
        let level = queue.slots();
        if !self.primed {
            if level < self.prefill.max(out.len()).max(1) {
                out.fill(0);
                r.zeros = out.len();
                return r;
            }
            self.primed = true;
            self.drift.reset();
        }
        let mut want = out.len();
        match self.drift.observe(level, out.len()) {
            Adjust::None => {}
            Adjust::Drop(n) => {
                let n = n.min(level.saturating_sub(want));
                if n > 0
                    && let Ok(chunk) = queue.read_chunk(n)
                {
                    chunk.commit_all();
                }
                r.dropped = n;
            }
            Adjust::Duplicate(n) => {
                r.duplicated = n.min(want);
                want -= r.duplicated;
            }
        }
        r.got = pop_into(queue, &mut out[..want]);
        if let Some(&l) = out[..r.got].last() {
            self.last = l;
        }
        if r.got < want {
            out[r.got..].fill(0);
            r.zeros = out.len() - r.got;
            r.duplicated = 0;
            r.underrun = true;
            self.primed = false;
        } else if r.duplicated > 0 {
            out[want..].fill(self.last);
        }
        r
    }
}

/// Moves up to `out.len()` samples from `queue`; returns how many
pub(crate) fn pop_into(queue: &mut rtrb::Consumer<i16>, out: &mut [i16]) -> usize {
    let n = queue.slots().min(out.len());
    if n == 0 {
        return 0;
    }
    let Ok(chunk) = queue.read_chunk(n) else { return 0 };
    let (a, b) = chunk.as_slices();
    out[..a.len()].copy_from_slice(a);
    out[a.len()..a.len() + b.len()].copy_from_slice(b);
    chunk.commit_all();
    n
}

/// Pushes as much of `samples` as fits; returns how many
pub(crate) fn push_from(queue: &mut rtrb::Producer<i16>, samples: &[i16]) -> usize {
    let n = queue.slots().min(samples.len());
    if n > 0
        && let Ok(chunk) = queue.write_chunk_uninit(n)
    {
        return chunk.fill_from_iter(samples.iter().copied());
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_the_tolerance_nothing_happens() {
        let mut d = Drift::new(16_000, 640, 160);
        for i in 0..10_000 {
            assert_eq!(d.observe(640 + (i % 160), 120), Adjust::None);
        }
    }

    #[test]
    fn a_full_queue_drops_and_a_low_one_repeats() {
        let mut d = Drift::new(16_000, 640, 160);
        let adj: Vec<_> = (0..100).map(|_| d.observe(1200, 120)).filter(|a| *a != Adjust::None).collect();
        assert!(!adj.is_empty() && adj.iter().all(|a| matches!(a, Adjust::Drop(1..=8))), "{adj:?}");
        let mut d = Drift::new(16_000, 640, 160);
        let adj: Vec<_> = (0..100).map(|_| d.observe(100, 120)).filter(|a| *a != Adjust::None).collect();
        assert!(!adj.is_empty() && adj.iter().all(|a| matches!(a, Adjust::Duplicate(1..=8))), "{adj:?}");
    }

    #[test]
    fn the_trough_decides_not_the_peaks() {
        // Bursts lift the level far above the target, but it falls back to
        // it before each burst
        let mut d = Drift::new(16_000, 640, 160);
        for i in 0..10_000 {
            let level = if i % 4 == 0 { 650 } else { 1400 };
            assert_eq!(d.observe(level, 120), Adjust::None);
        }
    }
}
