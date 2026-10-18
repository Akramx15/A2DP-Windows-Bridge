//! HCI over USB ("H2") on a controller bound to WinUSB, through `nusb`.
//!
//! Interface 0 carries HCI: commands on the default control pipe (class
//! request to the device), events on the interrupt IN pipe, ACL data on the
//! bulk pipes. SCO (interface 1) is not used by an A2DP source. HCI ISO data
//! (LE Audio) shares the bulk pipes with ACL, as in v1 and Linux btusb: out
//! through the ACL slot; in, the ACL reader delivers it as ACL and BTstack
//! tells the two apart by the connection handle (a CIS handle).
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
//! - a failed ACL read is retried the same way (ACL_IN_MAX_ERRORS)
//! - ACL / ISO writes have no timeout, and a failed one only drops the packet
//! - a transfer that fails for good (device gone, a lost command) is
//!   reported to BTstack's owner through [`Upstream::fail`]: BTstack itself
//!   cannot recover, it waits for the answer to a command that was lost
//! - an adapter that cannot be opened (another program holds it) is skipped
//!   for the next one the selector takes
//!
//! BTstack treats this transport as asynchronous: `send` hands the packet to
//! a writer thread (one for commands, one for ACL / ISO) and returns; the
//! writer reports each packet as sent.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use btstack_sys::consts::{HCI_ACL_DATA_PACKET, HCI_COMMAND_DATA_PACKET, HCI_EVENT_PACKET, HCI_ISO_DATA_PACKET};
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
/// Consecutive failed ACL reads before ACL delivery is given up
const ACL_IN_MAX_ERRORS: u32 = 10;
/// How often the I/O threads look at the stop flag
const POLL: Duration = Duration::from_millis(100);
/// Transfers kept in flight per IN pipe
const IN_TRANSFERS: usize = 3;
const EVENT_TRANSFER_SIZE: usize = 512;
const ACL_TRANSFER_SIZE: usize = 4096;

/// The writer slot of an outgoing packet: 0 commands (control pipe), 1 ACL
/// and ISO data (bulk OUT pipe). BTstack sends one packet per slot at a time
/// (one outgoing buffer for ACL and ISO).
fn out_slot(packet_type: u8) -> Option<usize> {
    match packet_type {
        HCI_COMMAND_DATA_PACKET => Some(0),
        HCI_ACL_DATA_PACKET | HCI_ISO_DATA_PACKET => Some(1),
        _ => None,
    }
}

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

impl UsbSelector {
    /// The adapter named on the command line (`-u`): a Windows device
    /// instance ID (`USB\VID_0BDA&PID_8771\00E04C000001`), a device
    /// interface path as v1 and SetupAPI list them
    /// (`\\?\usb#vid_0bda&pid_8771#00e04c000001#{a5dcbf10-...}`), or a
    /// VID:PID (`0bda:8771`, the first such adapter)
    pub fn from_arg(arg: &str) -> Result<UsbSelector, String> {
        let arg = arg.trim();
        let bad = || {
            format!(
                "'{arg}' is not a USB device: use its instance ID (USB\\VID_xxxx&PID_xxxx\\...), \
                 device path (\\\\?\\usb#vid_xxxx&pid_xxxx#...) or VID:PID"
            )
        };
        if let Some(vp) = parse_vid_pid_pair(arg) {
            return Ok(UsbSelector { vid_pid: Some(vp), ..Default::default() });
        }
        let id = if arg.starts_with(r"\\?\") || arg.starts_with(r"\\.\") {
            instance_id_from_device_path(arg).ok_or_else(bad)?
        } else {
            arg.to_string()
        };
        let mut parts = id.split('\\');
        let (Some(bus), Some(hw), Some(serial), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
            return Err(bad());
        };
        if !bus.eq_ignore_ascii_case("usb") || !hw.to_ascii_uppercase().starts_with("VID_") || serial.is_empty() {
            return Err(bad());
        }
        Ok(UsbSelector { instance_id: Some(id), ..Default::default() })
    }

    /// The selector picks this device: the named adapter, else any
    /// Bluetooth-class or known one
    fn takes(&self, c: &UsbCandidate) -> bool {
        if let Some(id) = &self.instance_id {
            c.instance_id.eq_ignore_ascii_case(id)
        } else if let Some(vp) = self.vid_pid {
            (c.vid, c.pid) == vp
        } else {
            c.bluetooth_class || c.known
        }
    }
}

/// `0bda:8771` (hex VID:PID)
fn parse_vid_pid_pair(s: &str) -> Option<(u16, u16)> {
    let (v, p) = s.split_once(':')?;
    if v.len() != 4 || p.len() != 4 {
        return None;
    }
    Some((u16::from_str_radix(v, 16).ok()?, u16::from_str_radix(p, 16).ok()?))
}

/// The device instance ID in a device interface path:
/// `\\?\usb#vid_0bda&pid_8771#00e04c000001#{guid}` is the interface of
/// `usb\vid_0bda&pid_8771\00e04c000001` (instance IDs compare without case)
pub fn instance_id_from_device_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\").or_else(|| path.strip_prefix(r"\\.\"))?;
    // The interface class GUID (and an optional reference string) follows
    // the last '#'
    let (id, guid) = rest.rsplit_once('#')?;
    if !guid.starts_with('{') || id.split('#').count() != 3 {
        return None;
    }
    Some(id.replace('#', "\\"))
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
    /// The writer of each slot (see [`out_slot`])
    writer_tx: [mpsc::Sender<(u8, Vec<u8>)>; 2],
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

    /// The WinUSB adapters the selector takes, in enumeration order
    fn find(&self) -> Result<Vec<DeviceInfo>, String> {
        let devices: Vec<DeviceInfo> = nusb::list_devices()
            .wait()
            .map_err(|e| format!("cannot list USB devices: {e}"))?
            .filter(|d| !is_vmware(d))
            .collect();
        let mut found = Vec::new();
        let mut wrong_driver = None;
        for info in devices {
            let c = candidate(&info, &self.selector.known);
            if !self.selector.takes(&c) {
                continue;
            }
            if c.winusb() {
                found.push(info);
            } else {
                wrong_driver.get_or_insert(c);
            }
        }
        if !found.is_empty() {
            return Ok(found);
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

/// Errors after which the device handle no longer reaches the adapter, as
/// v1's usb_device_gone: ERROR_DEVICE_NOT_CONNECTED, ERROR_NO_SUCH_DEVICE
/// and ERROR_FILE_NOT_FOUND (nusb's Disconnected), ERROR_BAD_COMMAND (22)
/// and ERROR_INVALID_HANDLE (6)
fn device_gone(e: &TransferError) -> bool {
    matches!(e, TransferError::Disconnected | TransferError::Unknown(22) | TransferError::Unknown(6))
}

impl HciBackend for UsbTransport {
    fn is_async(&self) -> bool {
        true
    }

    fn open(&mut self, upstream: Upstream) -> Result<(), String> {
        self.close();
        // The first adapter that opens: one another program holds is
        // skipped, as v1's usb_open tried every device
        let mut last_error = String::new();
        let mut opened = None;
        for info in self.find()? {
            let (vid, pid) = (info.vendor_id(), info.product_id());
            let attempt =
                info.open().wait().map_err(|e| format!("cannot open {vid:04X}:{pid:04X}: {e}")).and_then(|device| {
                    let interface = device
                        .claim_interface(0)
                        .wait()
                        .map_err(|e| format!("cannot claim interface 0 of {vid:04X}:{pid:04X}: {e}"))?;
                    let endpoints = hci_endpoints(&interface)
                        .ok_or_else(|| format!("{vid:04X}:{pid:04X} lacks the HCI event / ACL endpoints"))?;
                    Ok((device, interface, endpoints))
                });
            match attempt {
                Ok(o) => {
                    opened = Some((info, o));
                    break;
                }
                Err(e) => {
                    tracing::warn!(target: "hci", "USB: {e}; trying the next adapter, if any");
                    last_error = e;
                }
            }
        }
        let Some((info, (device, interface, (event_addr, acl_in_addr, acl_out_addr)))) = opened else {
            return Err(last_error);
        };
        let (vid, pid) = (info.vendor_id(), info.product_id());
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
        let (command_tx, command_rx) = mpsc::channel::<(u8, Vec<u8>)>();
        let (data_tx, data_rx) = mpsc::channel::<(u8, Vec<u8>)>();
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
            let (upstream, recycle) = (upstream.clone(), recycle_tx.clone());
            threads.push(spawn(
                "usb-cmd-out",
                Box::new(move || {
                    let write = |_: u8, p: &[u8]| send_command(&interface, p, COMMAND_TIMEOUT);
                    writer_loop(write, command_rx, recycle, &in_flight[0], &stop, &upstream)
                }),
            )?);
        }
        {
            let (stop, in_flight) = (stop.clone(), in_flight.clone());
            threads.push(spawn(
                "usb-acl-out",
                Box::new(move || {
                    let write_stop = stop.clone();
                    let write = |_: u8, p: &[u8]| {
                        let r = bulk_write(&mut acl_out, p, &write_stop);
                        if matches!(r, Err(TransferError::Stall)) {
                            // Or every later write fails as well
                            if let Err(e) = acl_out.clear_halt().wait() {
                                tracing::warn!(target: "hci", "USB: clearing the ACL out pipe failed: {e}");
                            }
                        }
                        r
                    };
                    writer_loop(write, data_rx, recycle_tx, &in_flight[1], &stop, &upstream)
                }),
            )?);
        }
        self.opened = Some(Opened {
            interface,
            _device: device,
            stop,
            writer_tx: [command_tx, data_tx],
            recycle_rx,
            threads,
            in_flight,
        });
        Ok(())
    }

    fn close(&mut self) {
        let Some(opened) = self.opened.take() else { return };
        opened.stop.store(true, Ordering::Release);
        // The writers finish the packet they have (a command bounded by its
        // timeout, a data write cancelled on the stop flag)
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
        out_slot(packet_type).is_some_and(|slot| !o.in_flight[slot].load(Ordering::Acquire))
    }

    fn send(&mut self, packet_type: u8, packet: &[u8]) -> bool {
        let Some(o) = &self.opened else { return false };
        let Some(slot) = out_slot(packet_type) else { return false };
        let mut buf = o.recycle_rx.try_recv().unwrap_or_default();
        buf.clear();
        buf.extend_from_slice(packet);
        o.in_flight[slot].store(true, Ordering::Release);
        if o.writer_tx[slot].send((packet_type, buf)).is_err() {
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
    let mut errors = 0u32;
    for _ in 0..IN_TRANSFERS {
        submit_in(ep, ACL_TRANSFER_SIZE);
    }
    while !stop.load(Ordering::Acquire) {
        let Some(c) = ep.wait_next_complete(POLL) else { continue };
        match c.status {
            Ok(()) => {
                errors = 0;
                acl.push(&c.buffer[..c.actual_len], |p| upstream.deliver(HCI_ACL_DATA_PACKET, p));
            }
            Err(TransferError::Cancelled) if stop.load(Ordering::Acquire) => break,
            Err(e) if device_gone(&e) => {
                upstream.fail(&format!("ACL read: {e}"));
                break;
            }
            Err(e) => {
                // As v1: reset the pipe and read on (a stall, a bad
                // transfer); only a run of failures gives up
                errors += 1;
                acl.reset();
                if errors > ACL_IN_MAX_ERRORS {
                    upstream.fail(&format!("ACL read failed {errors} times in a row: {e}"));
                    break;
                }
                tracing::warn!(target: "hci", "USB: ACL read failed ({e}), resetting the pipe ({errors}/{ACL_IN_MAX_ERRORS})");
                drain(ep);
                if let Err(e) = ep.clear_halt().wait() {
                    tracing::warn!(target: "hci", "USB: clearing the ACL in pipe failed: {e}");
                }
                for _ in 0..IN_TRANSFERS {
                    submit_in(ep, ACL_TRANSFER_SIZE);
                }
                continue;
            }
        }
        submit_in(ep, ACL_TRANSFER_SIZE);
    }
    drain(ep);
}

/// What becomes of a failed write
#[derive(Debug, PartialEq, Eq)]
enum WriteFailure {
    /// The stack has to be restarted
    Fatal,
    /// Logged; the packet is lost and the stack goes on
    Dropped,
}

/// A lost command leaves BTstack waiting for its answer for good, so any
/// command failure is fatal. A lost ACL / ISO packet is not (L2CAP and the
/// media stream go on, as in v1): only a gone adapter is.
fn write_failure(packet_type: u8, e: &TransferError) -> WriteFailure {
    if packet_type == HCI_COMMAND_DATA_PACKET || device_gone(e) { WriteFailure::Fatal } else { WriteFailure::Dropped }
}

/// One bulk OUT transfer with no timeout, as v1's ACL writes: a controller
/// may hold data for a while (a link going down, the air busy), which is not
/// a failure. Only the stop flag cancels it.
fn bulk_write(ep: &mut Endpoint<Bulk, Out>, packet: &[u8], stop: &AtomicBool) -> Result<(), TransferError> {
    let mut buf = Buffer::new(packet.len());
    buf.extend_from_slice(packet);
    ep.submit(buf);
    loop {
        if let Some(c) = ep.wait_next_complete(POLL) {
            return c.status;
        }
        if stop.load(Ordering::Acquire) {
            ep.cancel_all();
            return match ep.wait_next_complete(Duration::from_secs(1)) {
                Some(c) => c.status,
                None => {
                    tracing::warn!(target: "hci", "USB: a cancelled ACL write did not return");
                    Err(TransferError::Cancelled)
                }
            };
        }
    }
}

/// Sends the packets of one writer slot in the order BTstack hands them over
/// and reports each as sent. Commands (control pipe) and ACL / ISO data
/// (bulk OUT) have a thread each, so data the controller holds back does not
/// delay commands, as in v1 where both were separate overlapped transfers.
fn writer_loop(
    mut write: impl FnMut(u8, &[u8]) -> Result<(), TransferError>,
    rx: mpsc::Receiver<(u8, Vec<u8>)>,
    recycle: mpsc::Sender<Vec<u8>>,
    in_flight: &AtomicBool,
    stop: &AtomicBool,
    upstream: &Upstream,
) {
    loop {
        let (packet_type, packet) = match rx.recv_timeout(POLL) {
            Ok(p) => p,
            Err(mpsc::RecvTimeoutError::Timeout) if !stop.load(Ordering::Acquire) => continue,
            Err(_) => break,
        };
        let result = write(packet_type, &packet);
        in_flight.store(false, Ordering::Release);
        let _ = recycle.send(packet);
        match result {
            Ok(()) => upstream.packet_sent(),
            Err(e) => {
                let what = match packet_type {
                    HCI_COMMAND_DATA_PACKET => "HCI command",
                    HCI_ISO_DATA_PACKET => "ISO write",
                    _ => "ACL write",
                };
                if stop.load(Ordering::Acquire) {
                    break;
                }
                match write_failure(packet_type, &e) {
                    WriteFailure::Fatal => {
                        upstream.fail(&format!("{what}: {e}"));
                        if device_gone(&e) {
                            break;
                        }
                    }
                    WriteFailure::Dropped => {
                        tracing::warn!(target: "hci", "USB: {what} failed ({e}), packet dropped");
                    }
                }
                // Free BTstack's packet buffer anyway
                upstream.packet_sent();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_goes_through_the_acl_slot() {
        assert_eq!(out_slot(HCI_COMMAND_DATA_PACKET), Some(0));
        assert_eq!(out_slot(HCI_ACL_DATA_PACKET), Some(1));
        assert_eq!(out_slot(HCI_ISO_DATA_PACKET), Some(1));
        // SCO and events are not sent on this transport
        assert_eq!(out_slot(btstack_sys::consts::HCI_SCO_DATA_PACKET), None);
        assert_eq!(out_slot(HCI_EVENT_PACKET), None);
    }

    #[test]
    fn only_a_gone_adapter_or_a_lost_command_is_fatal() {
        use TransferError::*;
        for data in [HCI_ACL_DATA_PACKET, HCI_ISO_DATA_PACKET] {
            for e in [Cancelled, Stall, Fault, InvalidArgument, Unknown(31)] {
                assert_eq!(write_failure(data, &e), WriteFailure::Dropped, "{data} {e:?}");
            }
            for e in [Disconnected, Unknown(22), Unknown(6)] {
                assert_eq!(write_failure(data, &e), WriteFailure::Fatal, "{data} {e:?}");
            }
        }
        for e in [Cancelled, Stall, Disconnected] {
            assert_eq!(write_failure(HCI_COMMAND_DATA_PACKET, &e), WriteFailure::Fatal);
        }
        assert!(device_gone(&Disconnected));
        assert!(!device_gone(&Stall));
        assert!(!device_gone(&Cancelled));
    }

    fn usb_candidate(vid: u16, pid: u16, bluetooth_class: bool, known: bool) -> UsbCandidate {
        UsbCandidate {
            vid,
            pid,
            instance_id: format!(r"USB\VID_{vid:04X}&PID_{pid:04X}\1"),
            driver: Some("WinUSB".into()),
            product: None,
            bluetooth_class,
            known,
        }
    }

    #[test]
    fn selector_takes() {
        let any = UsbSelector::default();
        assert!(any.takes(&usb_candidate(0x0BDA, 0x8771, true, false)));
        // A vendor-class adapter (ASUS BT400) once its VID:PID is known
        assert!(any.takes(&usb_candidate(0x0B05, 0x17CB, false, true)));
        assert!(!any.takes(&usb_candidate(0x0B05, 0x17CB, false, false)));
        let vp = UsbSelector { vid_pid: Some((0x0B05, 0x17CB)), ..Default::default() };
        assert!(vp.takes(&usb_candidate(0x0B05, 0x17CB, false, false)));
        assert!(!vp.takes(&usb_candidate(0x0BDA, 0x8771, true, false)));
        let id = UsbSelector::from_arg(r"usb\vid_0bda&pid_8771\1").unwrap();
        assert!(id.takes(&usb_candidate(0x0BDA, 0x8771, true, false)));
        assert!(!id.takes(&usb_candidate(0x0BDA, 0x8772, true, false)));
    }

    #[test]
    fn selector_from_instance_id() {
        let s = UsbSelector::from_arg(r"USB\VID_0BDA&PID_8771\00E04C000001").unwrap();
        assert_eq!(s.instance_id.as_deref(), Some(r"USB\VID_0BDA&PID_8771\00E04C000001"));
        assert!(s.vid_pid.is_none());
        let s = UsbSelector::from_arg(r" usb\vid_0bda&pid_8771\5&1a2b3c&0&4 ").unwrap();
        assert_eq!(s.instance_id.as_deref(), Some(r"usb\vid_0bda&pid_8771\5&1a2b3c&0&4"));
    }

    #[test]
    fn selector_from_device_path() {
        let s = UsbSelector::from_arg(r"\\?\usb#vid_0bda&pid_8771#00e04c000001#{a5dcbf10-6530-11d2-901f-00c04fb951ed}")
            .unwrap();
        assert_eq!(s.instance_id.as_deref(), Some(r"usb\vid_0bda&pid_8771\00e04c000001"));
        assert!(
            s.instance_id.unwrap().eq_ignore_ascii_case(r"USB\VID_0BDA&PID_8771\00E04C000001"),
            "matches the instance ID nusb reports"
        );
        assert_eq!(
            instance_id_from_device_path(
                r"\\.\USB#VID_0A12&PID_0001#6&2C3D&0&1#{dee824ef-729b-4a0e-9c14-b7117d33a817}"
            )
            .as_deref(),
            Some(r"USB\VID_0A12&PID_0001\6&2C3D&0&1")
        );
        assert!(instance_id_from_device_path(r"\\?\usb#vid_0bda&pid_8771#00e04c000001").is_none());
        assert!(instance_id_from_device_path(r"usb#vid_0bda&pid_8771#x#{guid}").is_none());
    }

    #[test]
    fn selector_from_vid_pid() {
        let s = UsbSelector::from_arg("0bda:8771").unwrap();
        assert_eq!(s.vid_pid, Some((0x0BDA, 0x8771)));
        assert!(s.instance_id.is_none());
    }

    #[test]
    fn bad_selectors() {
        for bad in [
            "",
            "hello",
            "0bda:87",
            r"PCI\VEN_8086&DEV_A0F0\3&11583659&0&A3",
            r"USB\VID_0BDA&PID_8771",
            r"\\?\hid#x#{g}",
        ] {
            assert!(UsbSelector::from_arg(bad).is_err(), "{bad}");
        }
    }
}
