//! The HCI transport as BTstack sees it, implemented by a Rust backend
//! (crate hci-transport: WinUSB via nusb, H4 over TCP).
//!
//! BTstack calls open / close / send on its thread. The backend's I/O threads
//! hand received packets to an [`Upstream`], which queues them and wakes the
//! run loop; they reach BTstack there, with its pre-buffer in front.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::{GENERATION, WAKE_APP, WAKE_FLAGS, post_wake};

/// An HCI transport for BTstack
pub trait HciBackend: Send {
    /// Asynchronous backends return from [`send`](Self::send) before the
    /// packet is out and report it with [`Upstream::packet_sent`]; BTstack
    /// then asks [`can_send`](Self::can_send) before each packet.
    /// Synchronous ones (TCP) are done when `send` returns.
    fn is_async(&self) -> bool;
    /// Opens the device and starts receiving; packets go to `upstream`
    fn open(&mut self, upstream: Upstream) -> Result<(), String>;
    /// Stops receiving and releases the device. May be called more than
    /// once, and without a successful open.
    fn close(&mut self);
    /// Asynchronous backends: whether a packet of this type can be sent now
    fn can_send(&mut self, _packet_type: u8) -> bool {
        true
    }
    /// Sends one packet (H4 packet type, packet without type byte)
    fn send(&mut self, packet_type: u8, packet: &[u8]) -> bool;
}

/// The way back from a backend's I/O threads into BTstack. Belongs to one
/// run: after the stack it was opened for has stopped, it does nothing.
#[derive(Clone, Debug)]
pub struct Upstream {
    generation: u64,
}

struct RxQueue {
    packets: VecDeque<(u8, Vec<u8>)>,
    /// Buffers of delivered packets, reused
    free: Vec<Vec<u8>>,
}

static RX: Mutex<RxQueue> = Mutex::new(RxQueue { packets: VecDeque::new(), free: Vec::new() });
static SENT: AtomicU32 = AtomicU32::new(0);
static FAILED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static BACKEND: RefCell<Option<Box<dyn HciBackend>>> = const { RefCell::new(None) };
    static OPEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl Upstream {
    fn current(&self) -> bool {
        self.generation == GENERATION.load(Ordering::Acquire)
    }

    /// A packet from the controller (H4 packet type, packet without it)
    pub fn deliver(&self, packet_type: u8, packet: &[u8]) {
        if !self.current() {
            return;
        }
        {
            let mut q = RX.lock().unwrap_or_else(|e| e.into_inner());
            let mut buf = q.free.pop().unwrap_or_default();
            buf.clear();
            buf.extend_from_slice(packet);
            q.packets.push_back((packet_type, buf));
        }
        post_wake();
    }

    /// Asynchronous backends: the packet of the last `send` is out
    pub fn packet_sent(&self) {
        if !self.current() {
            return;
        }
        SENT.fetch_add(1, Ordering::AcqRel);
        post_wake();
    }

    /// The device stopped working for good (unplugged, I/O errors); see
    /// [`crate::Stack::transport_failed`]. The app is woken up.
    pub fn fail(&self, why: &str) {
        if !self.current() {
            return;
        }
        if !FAILED.swap(true, Ordering::AcqRel) {
            tracing::error!(target: "hci", "HCI transport failed: {why}");
        }
        WAKE_FLAGS.fetch_or(WAKE_APP, Ordering::AcqRel);
        post_wake();
    }
}

/// Clears what a previous run left
pub(crate) fn reset() {
    let mut q = RX.lock().unwrap_or_else(|e| e.into_inner());
    q.packets.clear();
    SENT.store(0, Ordering::Release);
    FAILED.store(false, Ordering::Release);
}

pub(crate) fn failed() -> bool {
    FAILED.load(Ordering::Acquire)
}

pub(crate) fn install(backend: Box<dyn HciBackend>) -> bool {
    let is_async = backend.is_async();
    BACKEND.with(|b| *b.borrow_mut() = Some(backend));
    is_async
}

pub(crate) fn is_async() -> bool {
    BACKEND.with(|b| b.borrow().as_ref().is_some_and(|b| b.is_async()))
}

pub(crate) fn open() -> bool {
    let upstream = Upstream { generation: GENERATION.load(Ordering::Acquire) };
    let result = BACKEND.with(|b| match b.borrow_mut().as_mut() {
        Some(backend) => backend.open(upstream),
        None => Err("no HCI backend installed".into()),
    });
    match result {
        Ok(()) => {
            OPEN.with(|o| o.set(true));
            true
        }
        Err(e) => {
            tracing::error!(target: "hci", "cannot open the HCI transport: {e}");
            false
        }
    }
}

pub(crate) fn close() {
    OPEN.with(|o| o.set(false));
    BACKEND.with(|b| {
        if let Some(backend) = b.borrow_mut().as_mut() {
            backend.close();
        }
    });
}

/// Closes and drops the backend (end of a run)
pub(crate) fn uninstall() {
    close();
    BACKEND.with(|b| b.borrow_mut().take());
}

pub(crate) fn can_send(packet_type: u8) -> bool {
    BACKEND.with(|b| b.borrow_mut().as_mut().is_some_and(|b| b.can_send(packet_type)))
}

pub(crate) fn send(packet_type: u8, packet: &[u8]) -> bool {
    BACKEND.with(|b| b.borrow_mut().as_mut().is_some_and(|b| b.send(packet_type, packet)))
}

/// On the run loop: hands queued completions and packets to BTstack
pub(crate) fn pump() {
    let sent = SENT.swap(0, Ordering::AcqRel);
    for _ in 0..sent {
        // SAFETY: on the run loop thread; the shim checks for a handler.
        unsafe { crate::ffi::a2dpwb_transport_packet_sent() };
    }
    loop {
        let next = RX.lock().unwrap_or_else(|e| e.into_inner()).packets.pop_front();
        let Some((packet_type, buf)) = next else { break };
        if buf.len() <= u16::MAX as usize {
            // SAFETY: on the run loop thread; buf is valid for its length;
            // the shim copies it and checks the size.
            let ok = unsafe { crate::ffi::a2dpwb_transport_deliver(packet_type, buf.as_ptr(), buf.len() as u16) };
            if ok == 0 {
                tracing::warn!(target: "hci", "dropped an incoming HCI packet of {} bytes", buf.len());
            }
        }
        let mut q = RX.lock().unwrap_or_else(|e| e.into_inner());
        if q.free.len() < 64 {
            q.free.push(buf);
        }
    }
}
