//! Band-limited 2:1 resampling around the spatial engine.
//!
//! The engine only takes 48 kHz stereo float32. For the 96 kHz transport the
//! capture keeps the device's 96 kHz, [`Downsample2`] halves it for the
//! engine and [`Upsample2`] doubles the engine's output back for the encoder.
//! Both are windowed-sinc FIR filters designed at 96 kHz: a Kaiser window
//! (β 7, about 72 dB of stop-band attenuation) with a 22 kHz cut-off, so
//! 20 kHz still passes while everything above 24 kHz (the 48 kHz Nyquist) is
//! rejected. The filters keep their state across calls and append to a
//! caller-provided buffer, so the running pipeline does not allocate here.
//!
//! Both have a constant group delay of about (taps - 1) / 2 samples (0.66 ms
//! at 96 kHz): the output is the input delayed, not a different waveform,
//! which is why the passband round trip only loses amplitude and nothing else.
//! The tail left in the filters when a stream stops is shorter than a
//! millisecond, so there is no flush; a restarted stream calls [`reset`].
//!
//! [`reset`]: Downsample2::reset

use std::f32::consts::PI;

/// The capture rate the 96 kHz transport runs at (twice the engine's)
pub const CAPTURE_RATE: u32 = 96_000;

/// Filter length in taps at 96 kHz
const TAPS: usize = 127;
/// Cut-off between the 20 kHz passband edge and the 24 kHz engine Nyquist
const CUTOFF: f32 = 22_000.0;
/// Kaiser β: about 72 dB of stop-band attenuation
const BETA: f32 = 7.0;

/// Resampling around the spatial engine: the capture keeps its rate (the
/// 96 kHz transport) and the engine still runs at 48 kHz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResampleConfig {
    /// Rate the capture runs at; the stage is built for [`CAPTURE_RATE`] only
    pub capture_rate: u32,
}

/// 96 kHz to 48 kHz: a low-pass and then every second frame.
pub struct Downsample2 {
    coeffs: Vec<f32>,
    /// One frame per slot, interleaved stereo, overwritten in a ring
    history: Vec<f32>,
    /// The next slot to write
    next: usize,
    /// Input frames since the last output (the decimation phase)
    phase: u8,
}

impl Downsample2 {
    pub fn new() -> Self {
        Downsample2 { coeffs: design(1.0), history: vec![0.0; TAPS * 2], next: 0, phase: 0 }
    }

    /// Appends the 48 kHz frames of `input` (interleaved stereo at 96 kHz) to
    /// `output`; the caller leaves room for half the frames. One input frame
    /// of an odd chunk stays in the phase until the next call, so the count is
    /// exactly two in for one out over time.
    pub fn process(&mut self, input: &[f32], output: &mut Vec<f32>) {
        let frames = input.len() / 2;
        for f in 0..frames {
            let slot = self.next * 2;
            self.history[slot] = input[f * 2];
            self.history[slot + 1] = input[f * 2 + 1];
            self.next = if self.next + 1 == TAPS { 0 } else { self.next + 1 };
            self.phase ^= 1;
            if self.phase == 0 {
                let (mut l, mut r) = (0.0f32, 0.0f32);
                // Newest sample first: the filter is symmetric, so the order
                // of the coefficients does not matter, only that both match
                let mut idx = self.next;
                for &c in &self.coeffs {
                    idx = if idx == 0 { TAPS - 1 } else { idx - 1 };
                    l += c * self.history[idx * 2];
                    r += c * self.history[idx * 2 + 1];
                }
                output.push(l);
                output.push(r);
            }
        }
    }

    /// Forgets the history (the stream restarted)
    pub fn reset(&mut self) {
        self.history.fill(0.0);
        self.next = 0;
        self.phase = 0;
    }
}

impl Default for Downsample2 {
    fn default() -> Self {
        Self::new()
    }
}

/// 48 kHz to 96 kHz: a polyphase windowed-sinc interpolator by two.
pub struct Upsample2 {
    /// The even taps h[2j], one per input frame of history
    even: Vec<f32>,
    /// The odd taps h[2j + 1]
    odd: Vec<f32>,
    /// One input frame per slot, interleaved stereo, overwritten in a ring
    history: Vec<f32>,
    /// The next slot to write
    next: usize,
}

impl Upsample2 {
    pub fn new() -> Self {
        let coeffs = design(2.0);
        let mut even = Vec::with_capacity(TAPS.div_ceil(2));
        let mut odd = Vec::with_capacity(TAPS / 2);
        for (i, &c) in coeffs.iter().enumerate() {
            if i % 2 == 0 {
                even.push(c);
            } else {
                odd.push(c);
            }
        }
        let slots = even.len();
        Upsample2 { even, odd, history: vec![0.0; slots * 2], next: 0 }
    }

    /// Appends the 96 kHz frames of `input` (interleaved stereo at 48 kHz) to
    /// `output`; two output frames per input frame, no state left over.
    pub fn process(&mut self, input: &[f32], output: &mut Vec<f32>) {
        let frames = input.len() / 2;
        let slots = self.even.len();
        for f in 0..frames {
            let slot = self.next * 2;
            self.history[slot] = input[f * 2];
            self.history[slot + 1] = input[f * 2 + 1];
            self.next = if self.next + 1 == slots { 0 } else { self.next + 1 };
            // y[2n] takes the even taps, y[2n + 1] the odd ones (the zeroed
            // input between two frames never contributes)
            let (mut l0, mut r0, mut l1, mut r1) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            let mut idx = self.next;
            for &c in &self.even {
                idx = if idx == 0 { slots - 1 } else { idx - 1 };
                l0 += c * self.history[idx * 2];
                r0 += c * self.history[idx * 2 + 1];
            }
            idx = self.next;
            for &c in &self.odd {
                idx = if idx == 0 { slots - 1 } else { idx - 1 };
                l1 += c * self.history[idx * 2];
                r1 += c * self.history[idx * 2 + 1];
            }
            output.push(l0);
            output.push(r0);
            output.push(l1);
            output.push(r1);
        }
    }

    /// Forgets the history (the stream restarted)
    pub fn reset(&mut self) {
        self.history.fill(0.0);
        self.next = 0;
    }
}

impl Default for Upsample2 {
    fn default() -> Self {
        Self::new()
    }
}

/// A Kaiser-windowed sinc low-pass at 96 kHz, normalized to `gain` at DC.
/// An upsample filter needs a gain of two: the zeroed input halves the
/// average, the filter puts it back.
fn design(gain: f32) -> Vec<f32> {
    let middle = (TAPS - 1) as f32 / 2.0;
    let fc = CUTOFF / CAPTURE_RATE as f32;
    let denom = bessel_i0(BETA);
    let mut coeffs = Vec::with_capacity(TAPS);
    let mut sum = 0.0f32;
    for n in 0..TAPS {
        let x = n as f32 - middle;
        let z = 2.0 * fc * x;
        let sinc = if x.abs() < f32::EPSILON { 1.0 } else { (PI * z).sin() / (PI * z) };
        let t = x / middle;
        let window = bessel_i0(BETA * (1.0 - t * t).max(0.0).sqrt()) / denom;
        let c = 2.0 * fc * sinc * window;
        coeffs.push(c);
        sum += c;
    }
    let scale = gain / sum;
    for c in &mut coeffs {
        *c *= scale;
    }
    coeffs
}

/// Zeroth-order modified Bessel function of the first kind (the Kaiser window)
fn bessel_i0(x: f32) -> f32 {
    let mut sum = 1.0f32;
    let mut term = 1.0f32;
    let mut k = 1u32;
    loop {
        let kf = k as f32;
        term *= (x * x / 4.0) / (kf * kf);
        sum += term;
        if term <= sum * 1e-9 {
            return sum;
        }
        k += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 96_000.0;

    fn sine(rate: f64, hz: f64, frames: usize, amp: f32) -> Vec<f32> {
        let mut v = Vec::with_capacity(frames * 2);
        for n in 0..frames {
            let s = amp * (2.0 * std::f64::consts::PI * hz * n as f64 / rate).sin() as f32;
            // The same signal on both channels
            v.push(s);
            v.push(s);
        }
        v
    }

    /// RMS of the left channel against an input amplitude of one
    fn rms(samples: &[f32]) -> f64 {
        let n = samples.len() / 2;
        let sum: f64 = samples.iter().step_by(2).map(|&s| (s as f64) * (s as f64)).sum();
        (sum / n as f64).sqrt()
    }

    /// Amplitude of the left channel at `hz`; the window must hold whole
    /// periods so the quadrature has no leakage
    fn amplitude(samples: &[f32], rate: f64, hz: f64) -> f64 {
        let n = samples.len() / 2;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for i in 0..n {
            let w = 2.0 * std::f64::consts::PI * hz * i as f64 / rate;
            let s = samples[i * 2] as f64;
            re += s * w.cos();
            im += s * w.sin();
        }
        2.0 * (re * re + im * im).sqrt() / n as f64
    }

    fn round_trip(input: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let mut down = Downsample2::new();
        let mut mid = Vec::with_capacity(input.len() / 2);
        down.process(input, &mut mid);
        let mut up = Upsample2::new();
        let mut out = Vec::with_capacity(mid.len() * 2);
        up.process(&mid, &mut out);
        (mid, out)
    }

    #[test]
    fn thirty_khz_is_rejected_by_more_than_sixty_db() {
        let input = sine(RATE, 30_000.0, 96_000, 1.0);
        let mut down = Downsample2::new();
        let mut out = Vec::with_capacity(input.len() / 2);
        down.process(&input, &mut out);
        // Skip the start-up, where the filter still fills from silence
        let db = 20.0 * rms(&out[2 * 2_000..]).log10();
        assert!(db < -60.0, "30 kHz came through at {db:.1} dB");
    }

    #[test]
    fn one_khz_survives_the_round_trip_in_amplitude() {
        let input = sine(RATE, 1_000.0, 96_000, 0.5);
        let (mid, out) = round_trip(&input);
        assert_eq!(mid.len(), input.len() / 2);
        assert_eq!(out.len(), input.len());
        // 900 whole periods of 1 kHz at 96 kHz after the start-up
        let skip = 2_000;
        let window = 96 * 900;
        let amp = amplitude(&out[2 * skip..2 * (skip + window)], RATE, 1_000.0);
        let error = (amp / 0.5 - 1.0).abs();
        assert!(error < 0.005, "1 kHz came back {:.3} % off in amplitude", error * 100.0);
    }

    #[test]
    fn twelve_khz_upsamples_within_a_tenth_of_a_db() {
        let input = sine(48_000.0, 12_000.0, 48_000, 0.5);
        let mut up = Upsample2::new();
        let mut out = Vec::with_capacity(input.len() * 2);
        up.process(&input, &mut out);
        assert_eq!(out.len(), input.len() * 2);
        let skip = 2_000;
        let window = 96 * 900;
        let amp = amplitude(&out[2 * skip..2 * (skip + window)], RATE, 12_000.0);
        let db = 20.0 * (amp / 0.5).log10();
        assert!(db.abs() < 0.1, "12 kHz changed by {db:.3} dB");
    }

    #[test]
    fn silence_stays_exactly_silent() {
        let input = vec![0.0f32; 2 * 4_096];
        let (_, out) = round_trip(&input);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn odd_chunks_keep_one_frame_for_the_next_call() {
        let input = [0.25f32; 2 * 5];
        let mut down = Downsample2::new();
        let mut out = Vec::new();
        down.process(&input[..2 * 3], &mut out);
        assert_eq!(out.len() / 2, 1);
        down.process(&input[2 * 3..], &mut out);
        assert_eq!(out.len() / 2, 2);
    }

    #[test]
    fn splitting_the_input_does_not_change_the_output() {
        let input = sine(RATE, 3_000.0, 4_800, 0.5);
        let mut down = Downsample2::new();
        let mut whole = Vec::new();
        down.process(&input, &mut whole);
        let mut split_down = Downsample2::new();
        let mut split = Vec::new();
        for chunk in input.chunks(2 * 37) {
            split_down.process(chunk, &mut split);
        }
        assert_eq!(whole, split);

        let mut up = Upsample2::new();
        let mut one = Vec::new();
        up.process(&whole, &mut one);
        let mut split_up = Upsample2::new();
        let mut two = Vec::new();
        for chunk in whole.chunks(2 * 37) {
            split_up.process(chunk, &mut two);
        }
        assert_eq!(one, two);
    }
}
