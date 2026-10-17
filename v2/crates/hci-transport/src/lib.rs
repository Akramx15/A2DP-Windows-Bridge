//! HCI transports for BTstack ([`btstack_sys::HciBackend`]):
//!
//! - [`usb::UsbTransport`]: a USB Bluetooth controller bound to WinUSB,
//!   driven through `nusb` (HCI over USB, "H2")
//! - [`tcp::TcpH4`]: H4 over TCP to a virtual controller (tools/emu)
//!
//! Both deliver whole HCI packets: they reassemble events and ACL packets
//! that arrive split over several transfers or reads.

#![forbid(unsafe_code)]

pub mod reassembly;
pub mod tcp;
pub mod usb;

pub use tcp::TcpH4;
pub use usb::{UsbCandidate, UsbSelector, UsbTransport};
