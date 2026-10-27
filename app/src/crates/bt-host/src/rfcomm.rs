//! RFCOMM client channels to a device the stack has an ACL link with, for
//! IPC clients that talk to the device's own service over RFCOMM (an
//! earbud app's control channel: battery, noise control, EQ). The stack
//! never connects or pairs for them: without a link there is no channel.
//!
//! An open resolves the service UUID to its RFCOMM server channel with an
//! SDP query, then opens the channel; one open runs at a time, the others
//! wait. Data a channel receives goes to its [`RfcommSink`] on the BTstack
//! thread; data to send is queued per channel and sent frame by frame as
//! RFCOMM's credits allow.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;

use btstack_sys::consts::*;
use btstack_sys::{AclState, Stack};

use crate::events::{addr_str, event_type, le16, u8_at};

/// Channels open or opening at a time (all clients together)
const MAX_CHANNELS: usize = 8;
/// Bytes a channel queues for sending before a send is refused
const MAX_QUEUED_BYTES: usize = 64 * 1024;

/// An open channel: its id (unique in this process, also across restarts
/// of the stack) and the largest frame it sends and receives
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RfcommOpened {
    pub id: u32,
    pub mtu: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfcommError {
    /// The stack has no link to the device (or is not running)
    NotConnected,
    /// The device's SDP lists no RFCOMM channel for the UUID
    ServiceNotFound,
    /// BTstack or the device refused (BTstack's status)
    Refused(u8),
    /// No open channel with this id (closed, or the stack restarted)
    NoChannel,
    /// Too many channels, or the channel's send queue is full
    Busy,
    /// No answer in time
    NoAnswer,
}

/// Why a channel closed (not for a close of its own client)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfcommCloseReason {
    /// The device closed it, or the link to the device went down
    Remote,
    /// The A2DP connection to the device ended (the stream was stopped),
    /// together with the link
    Disconnected,
    /// The Bluetooth stack stopped (restart, adapter removed, core exit)
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RfcommEvent {
    Data(Vec<u8>),
    /// The last event of the channel
    Closed(RfcommCloseReason),
}

/// Receives a channel's events with its id, on the BTstack thread: it must
/// not block. The first may come before [`crate::BtHost::rfcomm_open`]
/// has returned.
pub type RfcommSink = Box<dyn FnMut(u32, RfcommEvent) + Send>;

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

pub(crate) type OpenReply = mpsc::SyncSender<Result<RfcommOpened, RfcommError>>;

/// The outcome of [`crate::BtHost::rfcomm_send`] / `rfcomm_close`, which
/// return before the BTstack thread has taken them
pub struct RfcommReply(pub(crate) mpsc::Receiver<Result<(), RfcommError>>);

impl RfcommReply {
    pub fn wait(self) -> Result<(), RfcommError> {
        match self.0.recv_timeout(crate::JOB_TIMEOUT) {
            Ok(r) => r,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(RfcommError::NoAnswer),
            // The stack stopped: its channels are gone
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(RfcommError::NoChannel),
        }
    }
}

enum Stage {
    /// Behind the open that runs
    Waiting,
    /// SDP query sent; the first server channel it lists
    Sdp(Option<u8>),
    /// rfcomm_create_channel sent
    Connecting(u16),
}

struct Open {
    id: u32,
    addr: [u8; 6],
    uuid: [u8; 16],
    sink: RfcommSink,
    /// Its receiver is gone when the caller stopped waiting: the channel is
    /// then closed as soon as it opens
    reply: OpenReply,
    stage: Stage,
}

struct Channel {
    id: u32,
    cid: u16,
    addr: [u8; 6],
    mtu: u16,
    sink: RfcommSink,
    queue: VecDeque<Vec<u8>>,
    queued_bytes: usize,
    can_send_asked: bool,
    /// Its client closed it: it sends what is queued, then disconnects, and
    /// its sink gets nothing more
    closing: bool,
}

/// The channels and opens (BTstack thread only)
#[derive(Default)]
pub(crate) struct Rfcomm {
    opens: VecDeque<Open>,
    channels: Vec<Channel>,
}

impl Rfcomm {
    pub fn open(&mut self, stack: &Stack, addr: [u8; 6], uuid: [u8; 16], sink: RfcommSink, reply: OpenReply) {
        if !matches!(stack.acl_state(&addr), AclState::Open(_)) {
            tracing::info!("RFCOMM: no link to {}, not opening", addr_str(&addr));
            let _ = reply.send(Err(RfcommError::NotConnected));
            return;
        }
        if self.opens.len() + self.channels.len() >= MAX_CHANNELS {
            tracing::info!("RFCOMM: {MAX_CHANNELS} channels open or opening, not opening another");
            let _ = reply.send(Err(RfcommError::Busy));
            return;
        }
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        tracing::info!("RFCOMM: opening #{id} to {} for service {}", addr_str(&addr), uuid_str(&uuid));
        self.opens.push_back(Open { id, addr, uuid, sink, reply, stage: Stage::Waiting });
        self.next(stack);
    }

    /// Starts the first open if none runs
    fn next(&mut self, stack: &Stack) {
        while let Some(open) = self.opens.front_mut() {
            if !matches!(open.stage, Stage::Waiting) {
                return;
            }
            stack.rfcomm_init();
            let status = stack.sdp_query_rfcomm(&open.addr, &open.uuid);
            if status == ERROR_CODE_SUCCESS {
                open.stage = Stage::Sdp(None);
                return;
            }
            self.fail_first(RfcommError::Refused(status));
        }
    }

    fn fail_first(&mut self, error: RfcommError) {
        if let Some(open) = self.opens.pop_front() {
            tracing::info!("RFCOMM: #{} not opened: {error:?}", open.id);
            let _ = open.reply.send(Err(error));
        }
    }

    /// Source::SdpClient
    pub fn on_sdp(&mut self, stack: &Stack, p: &[u8]) {
        let Some(open) = self.opens.front_mut() else { return };
        let Stage::Sdp(found) = &mut open.stage else { return };
        match event_type(p) {
            Some(SDP_EVENT_QUERY_RFCOMM_SERVICE) => {
                let channel = u8_at(p, 2);
                tracing::debug!("RFCOMM: #{} SDP lists server channel {channel:?}", open.id);
                if found.is_none() {
                    *found = channel;
                }
            }
            Some(SDP_EVENT_QUERY_COMPLETE) => {
                let status = u8_at(p, 2).unwrap_or(ERROR_CODE_UNSPECIFIED_ERROR);
                let found = *found;
                tracing::debug!("RFCOMM: #{} SDP query complete (0x{status:02X}), server channel {found:?}", open.id);
                let result = match (status, found) {
                    (ERROR_CODE_SUCCESS, Some(server_channel)) => stack
                        .rfcomm_create_channel(&open.addr, server_channel)
                        .map(|cid| open.stage = Stage::Connecting(cid))
                        .map_err(RfcommError::Refused),
                    (ERROR_CODE_SUCCESS, None) => Err(RfcommError::ServiceNotFound),
                    _ if !matches!(stack.acl_state(&open.addr), AclState::Open(_)) => Err(RfcommError::NotConnected),
                    _ => Err(RfcommError::Refused(status)),
                };
                if let Err(e) = result {
                    self.fail_first(e);
                    self.next(stack);
                }
            }
            _ => {}
        }
    }

    /// Source::Rfcomm: channel events, and data (packet type
    /// RFCOMM_DATA_PACKET, `channel` its RFCOMM cid)
    pub fn on_packet(&mut self, stack: &Stack, packet_type: u8, channel: u16, p: &[u8]) {
        if packet_type == RFCOMM_DATA_PACKET {
            match self.channels.iter_mut().find(|c| c.cid == channel) {
                Some(c) if c.closing => {}
                Some(c) => {
                    tracing::trace!("RFCOMM: #{} received {} bytes", c.id, p.len());
                    (c.sink)(c.id, RfcommEvent::Data(p.to_vec()));
                }
                None => tracing::debug!("RFCOMM: {} bytes for unknown cid 0x{channel:04X}", p.len()),
            }
            return;
        }
        if packet_type != HCI_EVENT_PACKET {
            return;
        }
        match event_type(p) {
            Some(RFCOMM_EVENT_CHANNEL_OPENED) => self.on_opened(stack, p),
            Some(RFCOMM_EVENT_CHANNEL_CLOSED) => {
                let Some(cid) = le16(p, 2) else { return };
                if let Some(i) = self.channels.iter().position(|c| c.cid == cid) {
                    let mut c = self.channels.remove(i);
                    if c.closing {
                        tracing::info!("RFCOMM: #{} closed before its queue was sent", c.id);
                    } else {
                        tracing::info!("RFCOMM: #{} closed by the device or the link", c.id);
                        (c.sink)(c.id, RfcommEvent::Closed(RfcommCloseReason::Remote));
                    }
                }
            }
            Some(RFCOMM_EVENT_CAN_SEND_NOW) => {
                let Some(cid) = le16(p, 2) else { return };
                if let Some(i) = self.channels.iter().position(|c| c.cid == cid) {
                    let c = &mut self.channels[i];
                    c.can_send_asked = false;
                    c.send_next(stack);
                    if c.closing && c.queue.is_empty() {
                        let c = self.channels.remove(i);
                        tracing::info!("RFCOMM: #{} sent its queue, closing", c.id);
                        stack.rfcomm_disconnect(c.cid);
                    }
                }
            }
            _ => {}
        }
    }

    fn on_opened(&mut self, stack: &Stack, p: &[u8]) {
        let (Some(status), Some(cid), Some(mtu)) = (u8_at(p, 2), le16(p, 12), le16(p, 14)) else { return };
        let first = self.opens.front().is_some_and(|o| matches!(o.stage, Stage::Connecting(c) if c == cid));
        if !first {
            tracing::debug!("RFCOMM: channel opened event for cid 0x{cid:04X} of no open (0x{status:02X})");
            return;
        }
        let Some(open) = self.opens.pop_front() else { return };
        if status != ERROR_CODE_SUCCESS {
            tracing::info!("RFCOMM: #{} not opened: the device refused (0x{status:02X})", open.id);
            let _ = open.reply.send(Err(RfcommError::Refused(status)));
        } else if open.reply.send(Ok(RfcommOpened { id: open.id, mtu })).is_err() {
            tracing::info!("RFCOMM: #{} opened after its client stopped waiting, closing it", open.id);
            stack.rfcomm_disconnect(cid);
        } else {
            tracing::info!("RFCOMM: #{} open (cid 0x{cid:04X}, frames up to {mtu} bytes)", open.id);
            self.channels.push(Channel {
                id: open.id,
                cid,
                addr: open.addr,
                mtu: mtu.max(1),
                sink: open.sink,
                queue: VecDeque::new(),
                queued_bytes: 0,
                can_send_asked: false,
                closing: false,
            });
        }
        self.next(stack);
    }

    pub fn send(&mut self, stack: &Stack, id: u32, data: &[u8]) -> Result<(), RfcommError> {
        let c = self.channels.iter_mut().find(|c| c.id == id && !c.closing).ok_or(RfcommError::NoChannel)?;
        if c.queued_bytes + data.len() > MAX_QUEUED_BYTES {
            tracing::info!("RFCOMM: #{id} send queue full ({} bytes), {} bytes refused", c.queued_bytes, data.len());
            return Err(RfcommError::Busy);
        }
        for frame in data.chunks(c.mtu as usize) {
            c.queue.push_back(frame.to_vec());
            c.queued_bytes += frame.len();
        }
        c.send_next(stack);
        Ok(())
    }

    /// A close of the channel's own client, once what it queued is sent:
    /// no event follows
    pub fn close(&mut self, stack: &Stack, id: u32) -> Result<(), RfcommError> {
        let i = self.channels.iter().position(|c| c.id == id && !c.closing).ok_or(RfcommError::NoChannel)?;
        if self.channels[i].queue.is_empty() {
            let c = self.channels.remove(i);
            tracing::info!("RFCOMM: #{id} closed by its client");
            stack.rfcomm_disconnect(c.cid);
        } else {
            let c = &mut self.channels[i];
            tracing::info!("RFCOMM: #{id} closed by its client, after its {} queued bytes", c.queued_bytes);
            c.closing = true;
        }
        Ok(())
    }

    /// The A2DP connection to `addr` ended: its channels go with it, so that
    /// the link goes down as it does without them
    pub fn close_device(&mut self, stack: &Stack, addr: [u8; 6]) {
        let (gone, kept) = std::mem::take(&mut self.channels).into_iter().partition(|c| c.addr == addr);
        self.channels = kept;
        for mut c in gone {
            tracing::info!("RFCOMM: #{} closed with the A2DP connection", c.id);
            stack.rfcomm_disconnect(c.cid);
            if !c.closing {
                (c.sink)(c.id, RfcommEvent::Closed(RfcommCloseReason::Disconnected));
            }
        }
    }

    /// The run loop exits: every channel and open ends
    pub fn stop(&mut self) {
        for open in self.opens.drain(..) {
            let _ = open.reply.send(Err(RfcommError::NotConnected));
        }
        for mut c in self.channels.drain(..) {
            tracing::info!("RFCOMM: #{} closed, the stack stops", c.id);
            if !c.closing {
                (c.sink)(c.id, RfcommEvent::Closed(RfcommCloseReason::Stopped));
            }
        }
    }
}

impl Channel {
    /// Sends the next queued frame, or asks for the moment it can be sent
    fn send_next(&mut self, stack: &Stack) {
        if self.can_send_asked {
            return;
        }
        if let Some(frame) = self.queue.front() {
            let status = stack.rfcomm_send(self.cid, frame);
            if status == ERROR_CODE_SUCCESS {
                self.queued_bytes -= frame.len();
                self.queue.pop_front();
            } else {
                // No credits or buffers now: once it can
                tracing::trace!("RFCOMM: #{} send deferred (0x{status:02X})", self.id);
            }
        }
        if !self.queue.is_empty() {
            self.can_send_asked = stack.rfcomm_request_can_send_now(self.cid) == ERROR_CODE_SUCCESS;
        }
    }
}

/// "0000xxxx-0000-1000-8000-00805F9B34FB"
fn uuid_str(u: &[u8; 16]) -> String {
    let h: String = u.iter().map(|b| format!("{b:02X}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// Parses a service UUID: 128-bit ("00001101-0000-1000-8000-00805F9B34FB",
/// with or without dashes or braces), or 16 / 32-bit ("1101", "0x1101") on
/// the Bluetooth base UUID. Big endian, as printed.
pub fn parse_uuid(s: &str) -> Option<[u8; 16]> {
    let s = s.trim().trim_start_matches('{').trim_end_matches('}');
    let hex: String = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s).replace('-', "");
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut u = [0u8; 16];
    match hex.len() {
        4 | 8 => {
            u = [0, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0x80, 0x5F, 0x9B, 0x34, 0xFB];
            let short = u32::from_str_radix(&hex, 16).ok()?;
            u[..4].copy_from_slice(&short.to_be_bytes());
        }
        32 => {
            for (i, b) in u.iter_mut().enumerate() {
                *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
            }
        }
        _ => return None,
    }
    Some(u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids() {
        let spp = [0, 0, 0x11, 0x01, 0, 0, 0x10, 0, 0x80, 0, 0, 0x80, 0x5F, 0x9B, 0x34, 0xFB];
        assert_eq!(parse_uuid("1101"), Some(spp));
        assert_eq!(parse_uuid("0x1101"), Some(spp));
        assert_eq!(parse_uuid("00001101"), Some(spp));
        assert_eq!(parse_uuid("00001101-0000-1000-8000-00805f9b34fb"), Some(spp));
        assert_eq!(parse_uuid("{00001101-0000-1000-8000-00805F9B34FB}"), Some(spp));
        assert_eq!(parse_uuid("000011010000100080000080 5F9B34FB"), None);
        assert_eq!(parse_uuid("110"), None);
        assert_eq!(parse_uuid("g101"), None);
        assert_eq!(uuid_str(&spp), "00001101-0000-1000-8000-00805F9B34FB");
        let vendor = parse_uuid("0000079a-d102-11e1-9b23-00025b00a5a5").unwrap();
        assert_eq!(uuid_str(&vendor), "0000079A-D102-11E1-9B23-00025B00A5A5");
    }
}
