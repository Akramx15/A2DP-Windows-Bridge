//! HCI over USB ("H2") on a controller bound to WinUSB, through `nusb`.
//!
//! Interface 0 carries HCI: commands on the default control pipe (class
//! request to the device), events on the interrupt IN pipe, ACL data on the
//! bulk pipes. SCO (interface 1) is not used by an A2DP source.
//!
//! Recovery kept from v1's modified WinUSB transport:
//! - the event and ACL pipes are reset (clear halt) when the adapter is
//!   opened, so a halted endpoint or data left from a previous session does
//!   not keep the controller from answering HCI Reset
//! - a failed event read is retried (clear halt + new read), up to
//!   EVENT_IN_MAX_ERRORS in a row, instead of ending event delivery
//! - HCI commands time out (COMMAND_TIMEOUT), so a controller that stops
//!   accepting them does not block the stack
//! - close sends HCI Reset (bounded) and cancels everything, leaving the
//!   controller idle for the next session
//! - a transfer that fails for good (device gone) is reported to BTstack's
//!   owner through [`Upstream::fail`]: BTstack itself cannot recover, it
//!   waits for the answer to a command that was lost
//!
//! BTstack treats this transport as asynchronous: `send` hands the packet to
//! the writer thread and returns; the writer reports each packet as sent.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use btstack_sys::consts::{HCI_ACL_DATA_PACKET, HCI_COMMAND_DATA_PACKET, HCI_EVENT_PACKET};
use btstack_sys::{HciBackend, Upstream};
use nusb::descriptors::TransferType;
use nusb::transfer::{Buffer, Bulk, ControlOut, ControlType, In, Interrupt, Out, Recipient, TransferError};
use nusb::{Device, DeviceInfo, Endpoint, Interface, MaybeFuture};

use crate::reassembly::Reassembler;

/// HCI command control transfer timeout. A working controller accepts
/// commands at once, firmware download chunks included.
const COMMAND_TIMEOUT: Duration = Duration::from_millis(2000);
/// HCI Reset sent on close
const CLOSE_RESET_TIMEOUT: Duration = Duration::from_millis(500);
/// Consecutive failed event reads before event delivery is given up
const EVENT_IN_MAX_ERRORS: u32 = 10;
/// How often the I/O threads look at the stop flag
const POLL: Duration = Duration::from_millis(100);
/// Transfers kept in flight per IN pipe
const IN_TRANSFERS: usize = 3;
const EVENT_TRANSFER_SIZE: usize = 512;
const ACL_TRANSFER_SIZE: usize = 4096;

/// Which adapter to open
#[derive(Debug, Clone, Default)]
pub struct UsbSelector {
    /// Windows device instance ID (exact adapter), if chosen
    pub instance_id: Option<String>,
    /// VID:PID, if chosen
    pub vid_pid: Option<(u16, u16)>,
    /// VID:PID of controllers that are Bluetooth adapters even without the
    /// Bluetooth class code (vendor class FF/01/01, e.g. Realtek tables)
    pub known: Vec<(u16, u16)>,
}

/// A USB device that could be a Bluetooth controller
#[derive(Debug, Clone)]
pub struct UsbCandidate {
    pub vid: u16,
    pub pid: u16,
    pub instance_id: String,
    /// Driver service ("WinUSB", "BTHUSB", ...)
    pub driver: Option<String>,
    pub product: Option<String>,
    /// Bluetooth class code (E0/01/01) on the device or interface 0
    pub bluetooth_class: bool,
    /// In the selector's list of known controllers
    pub known: bool,
}

impl UsbCandidate {
    pub fn winusb(&self) -> bool {
        self.driver.as_deref().is_some_and(|d| d.eq_ignore_ascii_case("winusb"))
    }
}

fn is_bluetooth_class(class: u8, subclass: u8, protocol: u8) -> bool {
    class == 0xE0 && subclass == 0x01 && protocol == 0x01
}

/// VMware's virtual Bluetooth adapter (v1 skipped it as well)
fn is_vmware(info: &DeviceInfo) -> bool {
    info.vendor_id() == 0x0E0F
}

fn candidate(info: &DeviceInfo, known: &[(u16, u16)]) -> UsbCandidate {
    let bluetooth_class = is_bluetooth_class(info.class(), info.subclass(), info.protocol())
        || info
            .interfaces()
            .any(|i| i.interface_number() == 0 && is_bluetooth_class(i.class(), i.subclass(), i.protocol()));
    UsbCandidate {
        vid: info.vendor_id(),
        pid: info.product_id(),
        instance_id: info.instance_id().to_string_lossy().into_owned(),
        driver: info.driver().map(str::to_owned),
        product: info.product_string().map(str::to_owned),
        bluetooth_class,
        known: known.contains(&(info.vendor_id(), info.product_id())),
    }
}

/// Bluetooth controllers on USB (any driver), Bluetooth class or known VID:PID
pub fn list_candidates(known: &[(u16, u16)]) -> Vec<UsbCandidate> {
    let Ok(devices) = nusb::list_devices().wait() else { return Vec::new() };
    devices.filter(|d| !is_vmware(d)).map(|d| candidate(&d, known)).filter(|c| c.bluetooth_class || c.known).collect()
}

struct Opened {
    /// Kept for the control pipe (commands) and the endpoints' lifetime
    interface: Interface,
    _device: Device,
    stop: Arc<AtomicBool>,
    writer_tx: mpsc::Sender<(u8, Vec<u8>)>,
    /// Buffers of written packets come back here for reuse
    recycle_rx: mpsc::Receiver<Vec<u8>>,
    threads: Vec<JoinHandle<()>>,
    /// A packet of this type is with the writer (BTstack: one at a time each)
    in_flight: Arc<[AtomicBool; 2]>,
}

pub struct UsbTransport {
    selector: UsbSelector,
    opened: Option<Opened>,
}

impl UsbTransport {
    pub fn new(selector: UsbSelector) -> UsbTransport {
        UsbTransport { selector, opened: None }
    }

    fn find(&self) -> Result<DeviceInfo, String> {
        let devices: Vec<DeviceInfo> = nusb::list_devices()
            .wait()
            .map_err(|e| format!("cannot list USB devices: {e}"))?
            .filter(|d| !is_vmware(d))
            .collect();
        let mut wrong_driver = None;
        for info in devices {
            let c = candidate(&info, &self.selector.known);
            if let Some(id) = &self.selector.instance_id {
                if !c.instance_id.eq_ignore_ascii_case(id) {
                    continue;
                }
            } else if let Some(vp) = self.selector.vid_pid {
                if (c.vid, c.pid) != vp {
                    continue;
                }
            } else if !(c.bluetooth_class || c.known) {
                continue;
            }
            if c.winusb() {
                return Ok(info);
            }
            wrong_driver.get_or_insert(c);
        }
        Err(match wrong_driver {
            Some(c) => format!(
                "adapter {:04X}:{:04X} uses driver {} instead of WinUSB",
                c.vid,
                c.pid,
                c.driver.as_deref().unwrap_or("(none)")
            ),
            None => "no Bluetooth adapter with the WinUSB driver found".into(),
        })
    }
}

/// Endpoint addresses of the HCI pipes on interface 0
fn hci_endpoints(interface: &Interface) -> Option<(u8, u8, u8)> {
    let desc = interface.descriptor()?;
    let (mut event_in, mut acl_in, mut acl_out) = (None, None, None);
    for ep in desc.endpoints() {
        let addr = ep.address();
        match (ep.transfer_type(), addr & 0x80 != 0) {
            (TransferType::Interrupt, true) => event_in = event_in.or(Some(addr)),
            (TransferType::Bulk, true) => acl_in = acl_in.or(Some(addr)),
            (TransferType::Bulk, false) => acl_out = acl_out.or(Some(addr)),
            _ => {}
        }
    }
    Some((event_in?, acl_in?, acl_out?))
}

fn send_command(interface: &Interface, command: &[u8], timeout: Duration) -> Result<(), TransferError> {
    interface
        .control_out(
            ControlOut {
                control_type: ControlType::Class,
                recipient: Recipient::Device,
                request: 0,
                value: 0,
                index: 0,
                data: command,
            },
            timeout,
        )
        .wait()
}

/// Errors after which the device handle no longer reaches the adapter
fn device_gone(e: &TransferError) -> bool {
    matches!(e, TransferError::Disconnected)
}

impl HciBackend for UsbTransport {
    fn is_async(&self) -> bool {
        true
    }

    fn open(&mut self, upstream: Upstream) -> Result<(), String> {
        self.close();
        let info = self.find()?;
        let (vid, pid) = (info.vendor_id(), info.product_id());
        let device = info.open().wait().map_err(|e| format!("cannot open {vid:04X}:{pid:04X}: {e}"))?;
        let interface = device.claim_interface(0).wait().map_err(|e| format!("cannot claim interface 0: {e}"))?;
        let (event_addr, acl_in_addr, acl_out_addr) =
            hci_endpoints(&interface).ok_or("the adapter lacks the HCI event / ACL endpoints")?;
        let mut event_in = interface.endpoint::<Interrupt, In>(event_addr).map_err(|e| e.to_string())?;
        let mut acl_in = interface.endpoint::<Bulk, In>(acl_in_addr).map_err(|e| e.to_string())?;
        let mut acl_out = interface.endpoint::<Bulk, Out>(acl_out_addr).map_err(|e| e.to_string())?;
        // A halted endpoint or a data toggle left from an earlier session
        // keeps the controller from answering HCI Reset
        for (name, r) in [
            ("event", event_in.clear_halt().wait()),
            ("ACL in", acl_in.clear_halt().wait()),
            ("ACL out", acl_out.clear_halt().wait()),
        ] {
            if let Err(e) = r {
                tracing::warn!(target: "hci", "USB: clearing the {name} pipe failed: {e}");
            }
        }
        tracing::info!(
            target: "hci",
            "USB: opened {vid:04X}:{pid:04X} {} (event 0x{event_addr:02x}, ACL 0x{acl_in_addr:02x}/0x{acl_out_addr:02x})",
            info.instance_id().to_string_lossy()
        );

        let stop = Arc::new(AtomicBool::new(false));
        let in_flight = Arc::new([AtomicBool::new(false), AtomicBool::new(false)]);
        let (writer_tx, writer_rx) = mpsc::channel::<(u8, Vec<u8>)>();
        let (recycle_tx, recycle_rx) = mpsc::channel::<Vec<u8>>();
        let mut threads = Vec::new();
        let spawn = |name: &str, f: Box<dyn FnOnce() + Send>| {
            std::thread::Builder::new().name(name.into()).spawn(f).map_err(|e| e.to_string())
        };
        {
            let (stop, upstream) = (stop.clone(), upstream.clone());
            threads.push(spawn("usb-event-in", Box::new(move || event_loop(&mut event_in, &stop, &upstream)))?);
        }
        {
            let (stop, upstream) = (stop.clone(), upstream.clone());
            threads.push(spawn("usb-acl-in", Box::new(move || acl_in_loop(&mut acl_in, &stop, &upstream)))?);
        }
        {
            let (stop, interface, in_flight) = (stop.clone(), interface.clone(), in_flight.clone());
            threads.push(spawn(
                "usb-out",
                Box::new(move || {
                    writer_loop(&interface, &mut acl_out, writer_rx, recycle_tx, &in_flight, &stop, &upstream)
                }),
            )?);
        }
        self.opened = Some(Opened { interface, _device: device, stop, writer_tx, recycle_rx, threads, in_flight });
        Ok(())
    }

    fn close(&mut self) {
        let Some(opened) = self.opened.take() else { return };
        opened.stop.store(true, Ordering::Release);
        // The writer finishes the packet it has (bounded by the timeouts)
        drop(opened.writer_tx);
        for t in opened.threads {
            let _ = t.join();
        }
        // Leave the controller idle (no links, no scans, no queued events)
        // for the next session, as Linux does with HCI_QUIRK_RESET_ON_CLOSE
        match send_command(&opened.interface, &[0x03, 0x0C, 0x00], CLOSE_RESET_TIMEOUT) {
            Ok(()) => tracing::info!(target: "hci", "USB: HCI Reset sent on close"),
            Err(e) => tracing::warn!(target: "hci", "USB: HCI Reset on close failed: {e}"),
        }
        tracing::info!(target: "hci", "USB: closed");
    }

    fn can_send(&mut self, packet_type: u8) -> bool {
        let Some(o) = &self.opened else { return false };
        match packet_type {
            HCI_COMMAND_DATA_PACKET => !o.in_flight[0].load(Ordering::Acquire),
            HCI_ACL_DATA_PACKET => !o.in_flight[1].load(Ordering::Acquire),
            _ => false,
        }
    }

    fn send(&mut self, packet_type: u8, packet: &[u8]) -> bool {
        let Some(o) = &self.opened else { return false };
        let slot = match packet_type {
            HCI_COMMAND_DATA_PACKET => 0,
            HCI_ACL_DATA_PACKET => 1,
            _ => return false,
        };
        let mut buf = o.recycle_rx.try_recv().unwrap_or_default();
        buf.clear();
        buf.extend_from_slice(packet);
        o.in_flight[slot].store(true, Ordering::Release);
        if o.writer_tx.send((packet_type, buf)).is_err() {
            o.in_flight[slot].store(false, Ordering::Release);
            return false;
        }
        true
    }
}

impl Drop for UsbTransport {
    fn drop(&mut self) {
        self.close();
    }
}

fn submit_in<T: nusb::transfer::BulkOrInterrupt>(ep: &mut Endpoint<T, In>, size: usize) {
    // IN transfers must be a multiple of the max packet size
    let mps = ep.max_packet_size().max(1);
    let len = size.div_ceil(mps) * mps;
    ep.submit(Buffer::new(len));
}

fn drain<T: nusb::transfer::BulkOrInterrupt>(ep: &mut Endpoint<T, In>) {
    ep.cancel_all();
    while ep.pending() > 0 {
        if ep.wait_next_complete(Duration::from_secs(1)).is_none() {
            tracing::warn!(target: "hci", "USB: a cancelled IN transfer did not return");
            break;
        }
    }
}

fn event_loop(ep: &mut Endpoint<Interrupt, In>, stop: &AtomicBool, upstream: &Upstream) {
    let mut events = Reassembler::new(HCI_EVENT_PACKET);
    let mut errors = 0u32;
    for _ in 0..IN_TRANSFERS {
        submit_in(ep, EVENT_TRANSFER_SIZE);
    }
    while !stop.load(Ordering::Acquire) {
        let Some(c) = ep.wait_next_complete(POLL) else { continue };
        match c.status {
            Ok(()) => {
                errors = 0;
                events.push(&c.buffer[..c.actual_len], |p| upstream.deliver(HCI_EVENT_PACKET, p));
            }
            Err(TransferError::Cancelled) if stop.load(Ordering::Acquire) => break,
            Err(e) if device_gone(&e) => {
                upstream.fail(&format!("HCI event read: {e}"));
                break;
            }
            Err(e) => {
                // One bad read must not end event delivery for the session
                errors += 1;
                events.reset();
                if errors > EVENT_IN_MAX_ERRORS {
                    upstream.fail(&format!("HCI event read failed {errors} times in a row: {e}"));
                    break;
                }
                tracing::warn!(target: "hci", "USB: HCI event read failed ({e}), resetting the pipe ({errors}/{EVENT_IN_MAX_ERRORS})");
                drain(ep);
                if let Err(e) = ep.clear_halt().wait() {
                    tracing::warn!(target: "hci", "USB: clearing the event pipe failed: {e}");
                }
                for _ in 0..IN_TRANSFERS {
                    submit_in(ep, EVENT_TRANSFER_SIZE);
                }
                continue;
            }
        }
        submit_in(ep, EVENT_TRANSFER_SIZE);
    }
    drain(ep);
}

fn acl_in_loop(ep: &mut Endpoint<Bulk, In>, stop: &AtomicBool, upstream: &Upstream) {
    let mut acl = Reassembler::new(HCI_ACL_DATA_PACKET);
    for _ in 0..IN_TRANSFERS {
        submit_in(ep, ACL_TRANSFER_SIZE);
    }
    while !stop.load(Ordering::Acquire) {
        let Some(c) = ep.wait_next_complete(POLL) else { continue };
        match c.status {
            Ok(()) => acl.push(&c.buffer[..c.actual_len], |p| upstream.deliver(HCI_ACL_DATA_PACKET, p)),
            Err(TransferError::Cancelled) if stop.load(Ordering::Acquire) => break,
            Err(TransferError::Stall) => {
                acl.reset();
                tracing::warn!(target: "hci", "USB: ACL read stalled, resetting the pipe");
                drain(ep);
                let _ = ep.clear_halt().wait();
                for _ in 0..IN_TRANSFERS {
                    submit_in(ep, ACL_TRANSFER_SIZE);
                }
                continue;
            }
            Err(e) => {
                upstream.fail(&format!("ACL read: {e}"));
                break;
            }
        }
        submit_in(ep, ACL_TRANSFER_SIZE);
    }
    drain(ep);
}

/// Sends commands and ACL packets in the order BTstack hands them over, and
/// reports each as sent
fn writer_loop(
    interface: &Interface,
    acl_out: &mut Endpoint<Bulk, Out>,
    rx: mpsc::Receiver<(u8, Vec<u8>)>,
    recycle: mpsc::Sender<Vec<u8>>,
    in_flight: &[AtomicBool; 2],
    stop: &AtomicBool,
    upstream: &Upstream,
) {
    loop {
        let (packet_type, packet) = match rx.recv_timeout(POLL) {
            Ok(p) => p,
            Err(mpsc::RecvTimeoutError::Timeout) if !stop.load(Ordering::Acquire) => continue,
            Err(_) => break,
        };
        let result = if packet_type == HCI_COMMAND_DATA_PACKET {
            send_command(interface, &packet, COMMAND_TIMEOUT)
        } else {
            let mut buf = Buffer::new(packet.len());
            buf.extend_from_slice(&packet);
            let c = acl_out.transfer_blocking(buf, COMMAND_TIMEOUT);
            c.status
        };
        in_flight[if packet_type == HCI_COMMAND_DATA_PACKET { 0 } else { 1 }].store(false, Ordering::Release);
        let _ = recycle.send(packet);
        match result {
            Ok(()) => upstream.packet_sent(),
            Err(e) => {
                let what = if packet_type == HCI_COMMAND_DATA_PACKET { "HCI command" } else { "ACL write" };
                // A lost command leaves BTstack waiting for its answer for
                // good: the stack has to be restarted
                upstream.fail(&format!("{what}: {e}"));
                if device_gone(&e) || stop.load(Ordering::Acquire) {
                    break;
                }
                // Free BTstack's packet buffer anyway
                upstream.packet_sent();
            }
        }
    }
}
