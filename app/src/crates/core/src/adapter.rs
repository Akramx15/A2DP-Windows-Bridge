//! Which controller to open and how to bring it up: the Realtek chip for the
//! firmware download (setting, or the detected adapter as v1's "Auto"), the
//! Intel bootloader download and Broadcom PatchRAM (experimental), and the
//! AFH classification from the PC's Wi-Fi.

use std::path::Path;

use bt_host::afh;
use bt_host::chips::{self, ChipVendor};
use bt_host::{AfhClassification, AfhSource, HostConfig, TransportConfig};
use win_sys::usb_adapters::{self, UsbDevice};

use crate::settings::Settings;

/// A USB Bluetooth adapter as the UI and the stack setup see it
#[derive(Debug, Clone)]
pub struct Adapter {
    pub device: UsbDevice,
    /// Realtek PID for the firmware lookup (also for OEM adapters)
    pub realtek_pid: Option<u16>,
    pub vendor: ChipVendor,
    pub display_name: String,
}

/// Present USB devices that look like Bluetooth adapters (any driver).
/// `probe_winusb` opens WinUSB-bound ones for a moment (not while the stack
/// has the adapter open).
pub fn adapters(probe_winusb: bool) -> Vec<Adapter> {
    let ids = bt_host::realtek_usb_ids();
    let devices = match usb_adapters::enumerate(probe_winusb) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("cannot list USB devices: {e}");
            return Vec::new();
        }
    };
    devices
        .into_iter()
        .filter_map(|d| {
            let realtek_pid = chips::resolve_realtek_pid(d.vid, d.pid, &ids);
            let vendor = chips::vendor_for_usb(d.vid, d.pid, realtek_pid);
            let bluetooth =
                d.looks_like_bluetooth() || realtek_pid.is_some() || chips::is_bluetooth_vendor_vid(d.vid, d.pid);
            bluetooth.then(|| Adapter {
                display_name: chips::display_name(d.vid, d.pid, realtek_pid, vendor),
                device: d,
                realtek_pid,
                vendor,
            })
        })
        .collect()
}

/// Fills the chipset part of `config` from the settings and the present
/// adapters
pub fn configure(config: &mut HostConfig, settings: &Settings, _config_dir: &Path) {
    config.afh = Some(Box::new(WifiAfh));
    if matches!(config.transport, TransportConfig::Tcp(_)) {
        return; // a virtual controller: no chipset, no firmware
    }
    let auto = chips::is_auto_chip(settings.bt_chip_pid, &settings.bt_chip_fw_stem);
    let present = adapters(false);
    let winusb: Vec<&Adapter> =
        present.iter().filter(|a| a.device.driver_kind == win_sys::driver::DriverKind::WinUsb).collect();
    let pid =
        chips::resolve_chip_pid(settings.bt_chip_pid, &settings.bt_chip_fw_stem, winusb.iter().map(|a| a.realtek_pid));
    if pid != 0 {
        tracing::info!(
            "chip: {} (0x{pid:04X}){}",
            chips::realtek_chip_name(pid).unwrap_or("unknown"),
            if auto { " (auto)" } else { "" }
        );
        config.realtek_pid = pid;
    } else {
        tracing::info!(
            "no Realtek chip {}, skipping the Realtek chipset init",
            if auto { "detected" } else { "selected" }
        );
    }
    config.realtek_files = chips::firmware_files(pid, &settings.bt_chip_fw_stem).map(|(fw, cfg)| {
        let stem = |s: String| s.strip_suffix(".bin").map(str::to_owned).unwrap_or(s);
        (stem(fw), stem(cfg))
    });
    if pid == 0 {
        // Other vendors (experimental): Intel needs its bootloader
        // firmware, Broadcom may take a PatchRAM
        config.intel_loader = winusb.iter().any(|a| a.vendor == ChipVendor::Intel);
        config.broadcom_ids =
            winusb.iter().filter(|a| a.vendor == ChipVendor::Broadcom).map(|a| (a.device.vid, a.device.pid)).collect();
        for a in &winusb {
            if matches!(a.vendor, ChipVendor::MediaTek | ChipVendor::Qualcomm) {
                tracing::warn!(
                    "{} adapter {:04X}:{:04X}: firmware loading for this vendor is not implemented; it may not work",
                    a.vendor.name(),
                    a.device.vid,
                    a.device.pid
                );
            }
        }
    }
}

/// AFH host channel classification from the 2.4 GHz channel this PC's Wi-Fi
/// is connected on ("auto")
struct WifiAfh;

impl AfhSource for WifiAfh {
    fn classify(&mut self, policy: &str) -> Option<AfhClassification> {
        let c = afh::classify(policy, || match win_sys::wlan::survey() {
            Ok(s) => s.connected_channels_24ghz(),
            Err(e) => {
                tracing::debug!("Wi-Fi survey failed: {e}");
                Vec::new()
            }
        });
        c.passed.then(|| AfhClassification {
            map: c.map,
            usable: c.usable as u32,
            avoided: afh::describe(&c.avoided_wifi),
        })
    }
}
