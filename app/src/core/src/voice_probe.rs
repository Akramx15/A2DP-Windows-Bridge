//! --voice-probe (developer CLI, emulator): opens the HFP audio connection
//! once the A2DP stream and the HFP service level connection are up, sends
//! a 1 kHz 16-bit PCM tone (meaningful only with CVSD) and counts what comes
//! back. A probe of bt-host's voice API, not the headset microphone: the
//! session does not take part.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bt_host::{BtHost, HostError, VoiceRx, VoiceTx};

/// How long the probe waits for the service level connection
const SLC_WAIT: Duration = Duration::from_secs(15);

/// 1 kHz at 8 kHz, 16-bit little endian (CVSD's HCI format)
struct ToneTx {
    phase: u32,
}

impl VoiceTx for ToneTx {
    fn fill(&mut self, out: &mut [u8]) {
        for pair in out.as_chunks_mut::<2>().0 {
            let v = (std::f64::consts::TAU * self.phase as f64 / 8.0).sin() * 8000.0;
            *pair = (v as i16).to_le_bytes();
            self.phase = (self.phase + 1) % 8;
        }
    }
}

#[derive(Default)]
struct Counts {
    packets: AtomicU64,
    bytes: AtomicU64,
    status: [AtomicU64; 4],
}

struct CountRx(Arc<Counts>);

impl VoiceRx for CountRx {
    fn packet(&mut self, payload: &[u8], status: u8) {
        self.0.packets.fetch_add(1, Ordering::Relaxed);
        self.0.bytes.fetch_add(payload.len() as u64, Ordering::Relaxed);
        self.0.status[(status & 3) as usize].fetch_add(1, Ordering::Relaxed);
    }
}

/// Starts the probe on a thread of its own: retries until the service
/// level connection is up, then prints a summary every second
pub fn start(host: Arc<BtHost>) {
    let _ = std::thread::Builder::new().name("voice-probe".into()).spawn(move || {
        let counts = Arc::new(Counts::default());
        let since = Instant::now();
        loop {
            let tx = Box::new(ToneTx { phase: 0 });
            let rx = Box::new(CountRx(counts.clone()));
            match host.voice_start(tx, rx) {
                Ok(()) => {
                    println!("voice probe: audio connection requested");
                    break;
                }
                Err(HostError::NotConnected) if since.elapsed() < SLC_WAIT => {
                    std::thread::sleep(Duration::from_millis(300));
                }
                Err(e) => {
                    println!("voice probe: cannot start: {e:?}");
                    return;
                }
            }
        }
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let s = |i: usize| counts.status[i].load(Ordering::Relaxed);
            println!(
                "voice probe: {} packets, {} bytes, status counts ok {} invalid {} no data {} lost {}",
                counts.packets.load(Ordering::Relaxed),
                counts.bytes.load(Ordering::Relaxed),
                s(0),
                s(1),
                s(2),
                s(3)
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_is_1_khz_at_8_khz() {
        let mut tx = ToneTx { phase: 0 };
        let mut out = [0u8; 32];
        tx.fill(&mut out);
        let s: Vec<i16> = out.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect();
        // One period is 8 samples
        assert_eq!(s[0], 0);
        assert_eq!(s[2], 8000);
        assert_eq!(s[6], -8000);
        assert_eq!(&s[..8], &s[8..]);
    }
}
