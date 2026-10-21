//! HCI capture in PacketLogger format (.pklg), for Wireshark and
//! tools/a2dp_decode.
//!
//! Can be started in the middle of a connection: the packets that set up the
//! current connections (Connection Complete, L2CAP signaling, AVDTP
//! signaling) are remembered all the time and written first, so Wireshark
//! still tells the AVDTP media channel from signaling.
//!
//! Flight recorder: the last packets are kept in memory all the time
//! ([`RECENT_BYTES`]; media, SCO and ISO payloads cut to
//! [`RECORDER_PAYLOAD`] bytes, so that it spans minutes of streaming) and
//! [`HciCapture::save_recent`] writes them to a file when something went
//! wrong, without a capture started beforehand.

use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use btstack_sys::consts::{
    HCI_ACL_DATA_PACKET, HCI_COMMAND_DATA_PACKET, HCI_EVENT_PACKET, HCI_ISO_DATA_PACKET, HCI_SCO_DATA_PACKET,
    LOG_MESSAGE_PACKET,
};

const PSM_AVDTP: u16 = 0x0019;
const AVDTP_SIGNAL_DELAYREPORT: u8 = 0x0D;
/// Upper bound on remembered setup packets (a few dozen per connection)
const MAX_SETUP_RECORDS: usize = 1000;
/// Flight recorder: bytes of records kept
pub const RECENT_BYTES: usize = 3 * 1024 * 1024;
/// Flight recorder: bytes kept of a media, SCO or ISO packet (the ACL,
/// L2CAP and RTP headers)
pub const RECORDER_PAYLOAD: usize = 32;
/// Flight recorder files kept in their folder
pub const KEEP_RECORDINGS: usize = 10;
/// Flight recorder: at most one file per this time ([`HciCapture::save_recent`]
/// on every failure of a retry loop writes no more)
const SAVE_INTERVAL: Duration = Duration::from_secs(30);

fn packetlogger_type(packet_type: u8, incoming: bool) -> Option<u8> {
    Some(match packet_type {
        HCI_COMMAND_DATA_PACKET => 0x00,
        HCI_EVENT_PACKET => 0x01,
        HCI_ACL_DATA_PACKET => {
            if incoming {
                0x03
            } else {
                0x02
            }
        }
        HCI_SCO_DATA_PACKET => {
            if incoming {
                0x09
            } else {
                0x08
            }
        }
        HCI_ISO_DATA_PACKET => {
            if incoming {
                0x0D
            } else {
                0x0C
            }
        }
        LOG_MESSAGE_PACKET => 0xFC,
        _ => return None,
    })
}

/// PacketLogger record: length (4, BE) = 9 + payload, seconds (4, BE),
/// microseconds (4, BE), type (1), payload
fn record(pl_type: u8, payload: &[u8]) -> Vec<u8> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let mut r = Vec::with_capacity(13 + payload.len());
    r.extend_from_slice(&(9 + payload.len() as u32).to_be_bytes());
    r.extend_from_slice(&(now.as_secs() as u32).to_be_bytes());
    r.extend_from_slice(&now.subsec_micros().to_be_bytes());
    r.push(pl_type);
    r.extend_from_slice(payload);
    r
}

/// The packet with its pairing keys zeroed, for the flight recorder (its
/// files go into diagnostic bundles that get posted): Link Key
/// Notification, Link Key Request Reply, LE Long Term Key Request Reply, LE
/// Enable Encryption. A manual capture keeps them.
fn without_keys(packet_type: u8, p: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    let key_at = match packet_type {
        // Event code, length, address (6), key
        HCI_EVENT_PACKET if p.first() == Some(&0x18) => Some(8),
        HCI_COMMAND_DATA_PACKET if p.len() >= 3 => match le16(p, 0) {
            // Opcode, length, address (6), key
            0x040B => Some(9),
            // Opcode, length, handle (2), LTK
            0x201A => Some(5),
            // Opcode, length, handle (2), random (8), EDIV (2), LTK
            0x2019 => Some(15),
            _ => None,
        },
        _ => None,
    };
    match key_at {
        Some(at) if p.len() >= at + 16 => {
            let mut v = p.to_vec();
            v[at..at + 16].fill(0);
            std::borrow::Cow::Owned(v)
        }
        _ => std::borrow::Cow::Borrowed(p),
    }
}

/// Keeps the newest [`KEEP_RECORDINGS`] flight recorder files (their names
/// sort by time)
fn prune_recordings(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|n| n.starts_with("hci-") && n.ends_with(".pklg"))
        .collect();
    names.sort();
    for n in &names[..names.len().saturating_sub(KEEP_RECORDINGS)] {
        let _ = std::fs::remove_file(dir.join(n));
    }
}

fn le16(p: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([p[i], p[i + 1]])
}

#[derive(Clone, Copy)]
struct AvdtpChannel {
    local_cid: u16,
    remote_cid: u16,
}

#[derive(Default)]
struct State {
    file: Option<File>,
    path: PathBuf,
    /// Connection setup records per ACL handle, with their number
    setup: Vec<(u16, u64, Vec<u8>)>,
    /// Flight recorder: the last records with their number, oldest first
    recent: VecDeque<(u64, Vec<u8>)>,
    recent_bytes: usize,
    /// Records so far (numbers the flight recorder's and the setup records)
    seq: u64,
    /// When the flight recorder was last saved
    saved_at: Option<Instant>,
    /// AVDTP channels per handle ([0] = signaling)
    avdtp: HashMap<u16, Vec<AvdtpChannel>>,
    /// L2CAP connection requests waiting for their response: (handle,
    /// direction, identifier) -> (PSM, source CID)
    pending: HashMap<(u16, bool, u8), (u16, u16)>,
}

impl State {
    fn forget_handle(&mut self, handle: u16) {
        self.avdtp.remove(&handle);
        self.pending.retain(|k, _| k.0 != handle);
        self.setup.retain(|(h, _, _)| *h != handle);
    }

    /// An ACL packet on an AVDTP media channel (not signaling)
    fn is_media(&self, packet_type: u8, incoming: bool, p: &[u8]) -> bool {
        if packet_type != HCI_ACL_DATA_PACKET || p.len() < 8 {
            return false;
        }
        let handle = le16(p, 0) & 0x0FFF;
        let cid = le16(p, 6);
        self.avdtp.get(&handle).is_some_and(|chans| {
            chans.iter().skip(1).any(|ch| cid == if incoming { ch.local_cid } else { ch.remote_cid })
        })
    }

    fn remember(&mut self, rec: Vec<u8>) {
        self.recent_bytes += rec.len();
        self.recent.push_back((self.seq, rec));
        while self.recent_bytes > RECENT_BYTES
            && let Some((_, old)) = self.recent.pop_front()
        {
            self.recent_bytes -= old.len();
        }
    }

    /// Whether this packet belongs to a connection's setup (and its handle)
    fn track(&mut self, packet_type: u8, incoming: bool, p: &[u8]) -> Option<u16> {
        if packet_type == HCI_EVENT_PACKET {
            // Disconnection Complete: status, handle, reason
            if p.len() >= 6 && p[0] == 0x05 && p[2] == 0x00 {
                self.forget_handle(le16(p, 3) & 0x0FFF);
            }
            // Connection Complete: Wireshark tracks L2CAP channels only
            // within a connection it has seen established
            if p.len() >= 13 && p[0] == 0x03 && p[2] == 0x00 {
                return Some(le16(p, 3) & 0x0FFF);
            }
            return None;
        }
        if packet_type != HCI_ACL_DATA_PACKET || p.len() < 8 {
            return None;
        }
        let handle = le16(p, 0) & 0x0FFF;
        let pb = (p[1] >> 4) & 0x03;
        let acl_len = le16(p, 2) as usize;
        if pb == 0x01 || 4 + acl_len > p.len() {
            return None; // continuation fragment
        }
        let (l2_len, cid) = (le16(p, 4) as usize, le16(p, 6));
        if 8 + l2_len > p.len() {
            return None; // fragmented L2CAP frame
        }
        let d = &p[8..8 + l2_len];
        if cid == 0x0001 {
            let mut off = 0;
            while off + 4 <= d.len() {
                let (code, id, clen) = (d[off], d[off + 1], le16(d, off + 2) as usize);
                if off + 4 + clen > d.len() {
                    break;
                }
                let c = &d[off + 4..off + 4 + clen];
                match code {
                    // Connection Request: PSM, SCID
                    0x02 if clen >= 4 => {
                        self.pending.insert((handle, incoming, id), (le16(c, 0), le16(c, 2)));
                    }
                    // Connection Response: DCID, SCID, result
                    0x03 if clen >= 8 => {
                        let result = le16(c, 4);
                        if result != 1
                            && let Some((psm, _)) = self.pending.remove(&(handle, !incoming, id))
                            && result == 0
                            && psm == PSM_AVDTP
                        {
                            // The request went out from us if the
                            // response comes in
                            let (dcid, scid) = (le16(c, 0), le16(c, 2));
                            let ch = if incoming {
                                AvdtpChannel { local_cid: scid, remote_cid: dcid }
                            } else {
                                AvdtpChannel { local_cid: dcid, remote_cid: scid }
                            };
                            self.avdtp.entry(handle).or_default().push(ch);
                        }
                    }
                    // Disconnection Request: DCID, SCID
                    0x06 if clen >= 4 => {
                        let local = if incoming { le16(c, 0) } else { le16(c, 2) };
                        if let Some(chans) = self.avdtp.get_mut(&handle)
                            && let Some(i) = chans.iter().position(|ch| ch.local_cid == local)
                        {
                            if i == 0 {
                                // Signaling closed: a media channel left
                                // would be taken for it, and every media
                                // packet recorded as setup
                                chans.clear();
                            } else {
                                chans.remove(i);
                            }
                        }
                    }
                    _ => {}
                }
                off += 4 + clen;
            }
            return Some(handle);
        }
        let sig = *self.avdtp.get(&handle)?.first()?;
        if cid != if incoming { sig.local_cid } else { sig.remote_cid } {
            return None;
        }
        // AVDTP signaling, without the periodic DELAYREPORT
        if d.len() >= 2 && (d[0] >> 2) & 0x03 == 0 && d[1] & 0x3F == AVDTP_SIGNAL_DELAYREPORT {
            return None;
        }
        Some(handle)
    }
}

/// The capture; cheap to clone (shared)
#[derive(Clone, Default)]
pub struct HciCapture {
    state: Arc<Mutex<State>>,
    bytes: Arc<AtomicU64>,
}

impl HciCapture {
    pub fn new() -> HciCapture {
        HciCapture::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Every HCI packet (from the BTstack thread)
    pub fn log_packet(&self, packet_type: u8, incoming: bool, packet: &[u8]) {
        let Some(pl_type) = packetlogger_type(packet_type, incoming) else { return };
        let mut s = self.lock();
        s.seq += 1;
        let seq = s.seq;
        let setup = s.track(packet_type, incoming, packet);
        let bulky = matches!(packet_type, HCI_SCO_DATA_PACKET | HCI_ISO_DATA_PACKET)
            || s.is_media(packet_type, incoming, packet);
        let kept = if bulky {
            record(pl_type, &packet[..packet.len().min(RECORDER_PAYLOAD)])
        } else {
            record(pl_type, &without_keys(packet_type, packet))
        };
        s.remember(kept);
        if setup.is_none() && s.file.is_none() {
            return;
        }
        let rec = record(pl_type, packet);
        if let Some(f) = s.file.as_mut()
            && f.write_all(&rec).is_ok()
        {
            self.bytes.fetch_add(rec.len() as u64, Ordering::Relaxed);
        }
        if let Some(handle) = setup
            && s.setup.len() < MAX_SETUP_RECORDS
        {
            s.setup.push((handle, seq, rec));
        }
    }

    /// Writes the flight recorder to `dir\hci-<stamp>-<what>.pklg` (`stamp`
    /// from the caller: local time), keeping the newest
    /// [`KEEP_RECORDINGS`]. None if it is empty, or was saved less than 30
    /// s ago (`force` saves anyway: a diagnostic bundle).
    pub fn save_recent(&self, dir: &Path, stamp: &str, what: &str, force: bool) -> Option<PathBuf> {
        let what: String =
            what.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
        let path = dir.join(format!("hci-{stamp}-{what}.pklg"));
        {
            let mut s = self.lock();
            if s.recent.is_empty() || (!force && s.saved_at.is_some_and(|t| t.elapsed() < SAVE_INTERVAL)) {
                return None;
            }
            s.saved_at = Some(Instant::now());
        }
        let result = std::fs::create_dir_all(dir).and_then(|()| self.write_recent(&path, &what));
        match result {
            Ok(bytes) => {
                tracing::info!("HCI flight recorder saved ({bytes} bytes, {what}) -> {}", path.display());
                prune_recordings(dir);
                Some(path)
            }
            Err(e) => {
                tracing::warn!("HCI flight recorder: cannot write {}: {e}", path.display());
                None
            }
        }
    }

    /// The flight recorder as a .pklg file: the setup packets of the open
    /// connections it no longer holds, then what it holds -> bytes written
    pub fn write_recent(&self, path: &Path, why: &str) -> std::io::Result<u64> {
        let (setup, recent) = {
            let s = self.lock();
            let first = s.recent.front().map_or(u64::MAX, |r| r.0);
            let setup: Vec<Vec<u8>> = s.setup.iter().filter(|r| r.1 < first).map(|r| r.2.clone()).collect();
            let recent: Vec<Vec<u8>> = s.recent.iter().map(|r| r.1.clone()).collect();
            (setup, recent)
        };
        let mut out = std::io::BufWriter::new(File::create(path)?);
        let mut written = 0u64;
        let mut put = |out: &mut std::io::BufWriter<File>, r: &[u8]| -> std::io::Result<()> {
            out.write_all(r)?;
            written += r.len() as u64;
            Ok(())
        };
        let head = format!(
            "A2DPWB flight recorder ({why}): the last {} packets; media, SCO and ISO payloads cut to {RECORDER_PAYLOAD} bytes",
            recent.len()
        );
        put(&mut out, &record(0xFC, head.as_bytes()))?;
        if !setup.is_empty() {
            let note = format!("A2DPWB: {} connection setup packets recorded earlier follow", setup.len());
            put(&mut out, &record(0xFC, note.as_bytes()))?;
            for r in &setup {
                put(&mut out, r)?;
            }
            put(&mut out, &record(0xFC, b"A2DPWB: the recorded packets follow"))?;
        }
        for r in &recent {
            put(&mut out, r)?;
        }
        out.flush()?;
        Ok(written)
    }

    /// A note in the capture (shown by Wireshark as a log message)
    pub fn note(&self, text: &str) {
        let mut s = self.lock();
        if let Some(f) = s.file.as_mut() {
            let rec = record(0xFC, text.as_bytes());
            if f.write_all(&rec).is_ok() {
                self.bytes.fetch_add(rec.len() as u64, Ordering::Relaxed);
            }
        }
    }

    /// Forgets the setup records (a new BTstack run)
    pub fn reset_tracking(&self) {
        let mut s = self.lock();
        s.setup.clear();
        s.avdtp.clear();
        s.pending.clear();
    }

    pub fn start(&self, path: &Path) -> std::io::Result<()> {
        let mut s = self.lock();
        if s.file.is_some() {
            return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "a capture is running"));
        }
        let mut f = File::create(path)?;
        let mut written = 0u64;
        if !s.setup.is_empty() {
            let note = format!(
                "A2DPWB: capture started during a connection; {} connection setup packets recorded earlier follow",
                s.setup.len()
            );
            let rec = record(0xFC, note.as_bytes());
            f.write_all(&rec)?;
            written += rec.len() as u64;
            for (_, _, r) in &s.setup {
                f.write_all(r)?;
                written += r.len() as u64;
            }
        }
        let rec = record(0xFC, b"A2DPWB: live capture starts");
        f.write_all(&rec)?;
        written += rec.len() as u64;
        self.bytes.store(written, Ordering::Relaxed);
        s.file = Some(f);
        s.path = path.to_path_buf();
        tracing::info!("HCI capture: started -> {}", path.display());
        Ok(())
    }

    pub fn stop(&self) {
        let mut s = self.lock();
        let Some(mut f) = s.file.take() else { return };
        let _ = f.write_all(&record(0xFC, b"A2DPWB: capture stopped"));
        let _ = f.flush();
        tracing::info!("HCI capture: stopped ({} bytes) -> {}", self.bytes.load(Ordering::Relaxed), s.path.display());
    }

    pub fn active(&self) -> bool {
        self.lock().file.is_some()
    }

    pub fn path(&self) -> Option<PathBuf> {
        let s = self.lock();
        s.file.as_ref().map(|_| s.path.clone())
    }

    pub fn bytes_written(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_records_come_first() {
        let cap = HciCapture::new();
        // Connection Complete for handle 0x000B, before the capture starts
        cap.log_packet(HCI_EVENT_PACKET, true, &[0x03, 11, 0, 0x0B, 0x00, 1, 2, 3, 4, 5, 6, 1, 0]);
        // Some unrelated event: not remembered
        cap.log_packet(HCI_EVENT_PACKET, true, &[0x0E, 4, 1, 0x03, 0x0C, 0]);
        let path = std::env::temp_dir().join(format!("a2dpwb-cap-{}.pklg", std::process::id()));
        cap.start(&path).unwrap();
        cap.log_packet(HCI_COMMAND_DATA_PACKET, false, &[0x03, 0x0C, 0x00]);
        cap.stop();
        let data = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        // note, Connection Complete, note, command, note
        let mut types = Vec::new();
        let mut off = 0;
        while off + 13 <= data.len() {
            let len = u32::from_be_bytes(data[off..off + 4].try_into().unwrap()) as usize;
            types.push(data[off + 12]);
            off += 4 + len;
        }
        assert_eq!(types, vec![0xFC, 0x01, 0xFC, 0x00, 0xFC]);
        assert_eq!(off, data.len());
    }

    /// An L2CAP packet on handle 1
    fn acl(cid: u16, payload: &[u8]) -> Vec<u8> {
        let l2 = payload.len() as u16;
        let mut p = vec![0x01, 0x20];
        p.extend_from_slice(&(l2 + 4).to_le_bytes());
        p.extend_from_slice(&l2.to_le_bytes());
        p.extend_from_slice(&cid.to_le_bytes());
        p.extend_from_slice(payload);
        p
    }

    /// The records of a .pklg file: (type, payload length)
    fn records(data: &[u8]) -> Vec<(u8, usize)> {
        let mut out = Vec::new();
        let mut off = 0;
        while off + 13 <= data.len() {
            let len = u32::from_be_bytes(data[off..off + 4].try_into().unwrap()) as usize;
            out.push((data[off + 12], len - 9));
            off += 4 + len;
        }
        assert_eq!(off, data.len());
        out
    }

    #[test]
    fn flight_recorder_keeps_recent_packets_and_cuts_media() {
        let cap = HciCapture::new();
        cap.log_packet(HCI_EVENT_PACKET, true, &[0x03, 11, 0, 0x01, 0x00, 1, 2, 3, 4, 5, 6, 1, 0]);
        for (id, local, remote) in [(1u8, 0x40u8, 0x50u8), (2, 0x41, 0x51)] {
            cap.log_packet(HCI_ACL_DATA_PACKET, false, &acl(1, &[0x02, id, 4, 0, 0x19, 0, local, 0]));
            cap.log_packet(HCI_ACL_DATA_PACKET, true, &acl(1, &[0x03, id, 8, 0, remote, 0, local, 0, 0, 0, 0, 0]));
        }
        // A media packet of 600 bytes, then an unrelated event
        cap.log_packet(HCI_ACL_DATA_PACKET, false, &acl(0x51, &[0u8; 600]));
        cap.log_packet(HCI_EVENT_PACKET, true, &[0x0E, 4, 1, 0x03, 0x0C, 0]);
        let dir = std::env::temp_dir().join(format!("a2dpwb-rec-{}", std::process::id()));
        let path = cap.save_recent(&dir, "20261021_120000", "Stream error", false).unwrap();
        assert!(path.ends_with("hci-20261021_120000-stream_error.pklg"), "{}", path.display());
        // Within 30 s only when forced
        assert!(cap.save_recent(&dir, "20261021_120001", "x", false).is_none());
        let data = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let recs = records(&data);
        // note, Connection Complete, 4 L2CAP signaling, media (cut), event
        assert_eq!(recs.len(), 8, "{recs:?}");
        assert_eq!(recs[0].0, 0xFC);
        assert_eq!(recs[6], (0x02, RECORDER_PAYLOAD));
        assert_eq!(recs[7], (0x01, 6));
    }

    #[test]
    fn flight_recorder_is_bounded_and_writes_setup_it_lost() {
        let cap = HciCapture::new();
        cap.log_packet(HCI_EVENT_PACKET, true, &[0x03, 11, 0, 0x01, 0x00, 1, 2, 3, 4, 5, 6, 1, 0]);
        let big = [0x0Eu8; 250];
        for _ in 0..(RECENT_BYTES / 200) {
            cap.log_packet(HCI_EVENT_PACKET, true, &big);
        }
        {
            let s = cap.lock();
            assert!(s.recent_bytes <= RECENT_BYTES);
            assert!(s.recent.front().unwrap().0 > 1, "the Connection Complete fell out");
        }
        let path = std::env::temp_dir().join(format!("a2dpwb-rec2-{}.pklg", std::process::id()));
        cap.write_recent(&path, "test").unwrap();
        let data = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let recs = records(&data);
        // note, note, the Connection Complete again, note, ...
        assert_eq!((recs[1].0, recs[2], recs[3].0), (0xFC, (0x01, 13), 0xFC));
    }

    #[test]
    fn flight_recorder_drops_pairing_keys() {
        let cap = HciCapture::new();
        // Link Key Notification: address, key 0x11.., type
        let mut notify = vec![0x18, 23, 1, 2, 3, 4, 5, 6];
        notify.extend([0x11; 16]);
        notify.push(4);
        cap.log_packet(HCI_EVENT_PACKET, true, &notify);
        // Link Key Request Reply: opcode 0x040B, length, address, key
        let mut reply = vec![0x0B, 0x04, 22, 1, 2, 3, 4, 5, 6];
        reply.extend([0x22; 16]);
        cap.log_packet(HCI_COMMAND_DATA_PACKET, false, &reply);
        let s = cap.lock();
        for (_, r) in &s.recent {
            assert!(!r.windows(16).any(|w| w == [0x11; 16] || w == [0x22; 16]), "{r:02X?}");
        }
        // The address stays
        assert_eq!(&s.recent[0].1[13 + 2..13 + 8], &[1, 2, 3, 4, 5, 6]);
        assert_eq!(without_keys(HCI_EVENT_PACKET, &[0x0E, 1, 0]).len(), 3);
    }

    #[test]
    fn keeps_the_newest_recordings() {
        let dir = std::env::temp_dir().join(format!("a2dpwb-prune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..KEEP_RECORDINGS + 3 {
            std::fs::write(dir.join(format!("hci-20261021_1200{i:02}-x.pklg")), "").unwrap();
        }
        prune_recordings(&dir);
        let mut names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(names.len(), KEEP_RECORDINGS);
        assert_eq!(names[0], "hci-20261021_120003-x.pklg");
    }

    #[test]
    fn media_is_not_taken_for_signaling_once_signaling_closed() {
        let cap = HciCapture::new();
        cap.log_packet(HCI_EVENT_PACKET, true, &[0x03, 11, 0, 0x01, 0x00, 1, 2, 3, 4, 5, 6, 1, 0]);
        // AVDTP signaling (local 0x40, remote 0x50), then media (0x41, 0x51)
        for (id, local, remote) in [(1u8, 0x40u8, 0x50u8), (2, 0x41, 0x51)] {
            cap.log_packet(HCI_ACL_DATA_PACKET, false, &acl(1, &[0x02, id, 4, 0, 0x19, 0, local, 0]));
            cap.log_packet(HCI_ACL_DATA_PACKET, true, &acl(1, &[0x03, id, 8, 0, remote, 0, local, 0, 0, 0, 0, 0]));
        }
        // Signaling closed first (Disconnection Request: DCID, SCID)
        cap.log_packet(HCI_ACL_DATA_PACKET, false, &acl(1, &[0x06, 3, 4, 0, 0x50, 0, 0x40, 0]));
        let before = cap.lock().setup.len();
        // A media packet is no setup packet
        cap.log_packet(HCI_ACL_DATA_PACKET, false, &acl(0x51, &[0x80, 0x60, 0, 1]));
        assert_eq!(cap.lock().setup.len(), before);
    }
}
