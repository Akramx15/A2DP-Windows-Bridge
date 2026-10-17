//! The media packet queue between the encode thread (producer) and the
//! BTstack thread (consumer): a lock-free SPSC ring of fixed-size slots, so
//! neither side allocates or waits.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use btstack_sys::Remote;

use crate::stats::{LinkStats, MEDIA_QUEUE_PACKETS};

/// RTP media header size (BTstack AVDTP_MEDIA_PAYLOAD_HEADER_SIZE)
pub const RTP_HEADER_SIZE: usize = 12;
/// Largest L2CAP payload with HCI_ACL_PAYLOAD_SIZE (1691 + 4-byte L2CAP header)
pub const MAX_MEDIA_PACKET_SIZE: usize = 1691;
/// Media packet size limit (advanced setting): libldac needs at least 679
/// (one 2-DH5); 1679 is the largest payload the queue can send with an RTP
/// header; 1023 fits one 3-DH5 baseband packet (larger packets span several,
/// so a lost baseband packet loses more audio, for 1-2 % less overhead)
pub const MEDIA_PAYLOAD_LIMIT_MIN: u16 = 679;
pub const MEDIA_PAYLOAD_LIMIT_MAX: u16 = 1679;
pub const MEDIA_PAYLOAD_LIMIT_DEFAULT: u16 = 1023;

pub fn clamp_media_payload_limit(limit: u32) -> u16 {
    limit.clamp(MEDIA_PAYLOAD_LIMIT_MIN as u32, MEDIA_PAYLOAD_LIMIT_MAX as u32) as u16
}

/// One media packet: the media payload (media payload header + codec
/// frames); BTstack adds the RTP header unless `rtp` is false
#[derive(Clone, Copy)]
pub struct MediaPacket {
    pub data: [u8; MAX_MEDIA_PACKET_SIZE],
    pub len: u16,
    pub timestamp: u32,
    pub rtp: bool,
}

impl Default for MediaPacket {
    fn default() -> Self {
        MediaPacket { data: [0; MAX_MEDIA_PACKET_SIZE], len: 0, timestamp: 0, rtp: true }
    }
}

impl MediaPacket {
    pub fn payload(&self) -> &[u8] {
        &self.data[..self.len as usize]
    }
}

pub(crate) fn queue(stats: Arc<LinkStats>, remote: Remote) -> (MediaSink, rtrb::Consumer<MediaPacket>) {
    let (producer, consumer) = rtrb::RingBuffer::new(MEDIA_QUEUE_PACKETS);
    (MediaSink { producer, remote, stats }, consumer)
}

/// The encode thread's end of the queue
pub struct MediaSink {
    producer: rtrb::Producer<MediaPacket>,
    remote: Remote,
    stats: Arc<LinkStats>,
}

impl MediaSink {
    /// Queues a packet and wakes the BTstack thread. False (and counted as a
    /// queue drop) when the queue is full: the radio is not keeping up.
    pub fn send(&mut self, packet: &MediaPacket) -> bool {
        match self.producer.push(*packet) {
            Ok(()) => {
                let depth = MEDIA_QUEUE_PACKETS - self.producer.slots();
                self.stats.queue_depth.store(depth as u32, Ordering::Relaxed);
                self.remote.wake();
                true
            }
            Err(_) => {
                self.stats.queue_drops.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    /// Packets waiting (ABR input)
    pub fn depth(&self) -> u32 {
        (MEDIA_QUEUE_PACKETS - self.producer.slots()) as u32
    }

    pub fn stats(&self) -> &Arc<LinkStats> {
        &self.stats
    }
}
