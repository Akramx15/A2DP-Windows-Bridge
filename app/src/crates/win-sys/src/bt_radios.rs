//! The Bluetooth radios Windows' own Bluetooth stack runs: the name Device
//! Manager shows, the address and the versions (its Advanced tab), and the
//! USB device each sits on
//!
//! Every radio registers GUID_BTHPORT_DEVICE_INTERFACE; IOCTL_BTH_GET_LOCAL_INFO
//! on it returns BTH_LOCAL_RADIO_INFO (bthioctl.h). A radio of a composite
//! device (MediaTek: `USB\VID_0E8D&PID_7925&MI_00\...`) is a child of the
//! USB device `usb_adapters` lists, so the device tree is walked up to it.

use windows::Win32::Devices::Bluetooth::GUID_BTHPORT_DEVICE_INTERFACE;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Device_IDW, CM_Get_Parent, CR_SUCCESS, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA,
    SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
};
use windows::Win32::Foundation::ERROR_NO_MORE_ITEMS;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::core::{HSTRING, PCWSTR};

use crate::usb_adapters::{DevInfoSet, OwnedHandle};

/// A radio of Windows' Bluetooth stack
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsRadio {
    /// The radio's device node and its parents up to the root
    /// (`USB\VID_0E8D&PID_7925&MI_00\...`, `USB\VID_0E8D&PID_7925\...`, ...)
    pub instance_ids: Vec<String>,
    /// Device Manager's name ("MediaTek Bluetooth Adapter")
    pub name: String,
    /// "C0:E3:50:2E:00:09"; None if the radio could not be asked
    pub address: Option<String>,
    /// HCI version (12 = Bluetooth 5.3)
    pub hci_version: Option<u8>,
    /// LMP version (the same numbering)
    pub lmp_version: Option<u8>,
}

impl WindowsRadio {
    /// The radio sits on this device (itself or one of its parents)
    pub fn on_device(&self, instance_id: &str) -> bool {
        self.instance_ids.iter().any(|i| i.eq_ignore_ascii_case(instance_id))
    }
}

/// BTH_DEVICE_INFO (bthdef.h)
#[repr(C)]
struct BthDeviceInfo {
    flags: u32,
    address: u64,
    class_of_device: u32,
    name: [u8; 248],
}

/// BTH_RADIO_INFO (bthdef.h)
#[repr(C)]
struct BthRadioInfo {
    lmp_supported_features: u64,
    mfg: u16,
    lmp_subversion: u16,
    lmp_version: u8,
}

/// BTH_LOCAL_RADIO_INFO (bthioctl.h)
#[repr(C)]
struct BthLocalRadioInfo {
    local_info: BthDeviceInfo,
    flags: u32,
    hci_revision: u16,
    hci_version: u8,
    radio_info: BthRadioInfo,
}

/// CTL_CODE(FILE_DEVICE_BLUETOOTH, BTH_IOCTL_BASE + 0, METHOD_BUFFERED,
/// FILE_ANY_ACCESS)
const IOCTL_BTH_GET_LOCAL_INFO: u32 = 0x0041_0000;

/// The present radios of Windows' Bluetooth (none if it runs none)
pub fn radios() -> Vec<WindowsRadio> {
    // SAFETY: interface class GUID and flags as documented; no window.
    let set = match unsafe {
        SetupDiGetClassDevsW(
            Some(&GUID_BTHPORT_DEVICE_INTERFACE),
            PCWSTR::null(),
            None,
            DIGCF_DEVICEINTERFACE | DIGCF_PRESENT,
        )
    } {
        Ok(s) => DevInfoSet(s),
        Err(e) => {
            tracing::debug!("bt_radios: SetupDiGetClassDevs failed: {e}");
            return Vec::new();
        }
    };
    let mut result = Vec::new();
    for index in 0.. {
        let mut intf =
            SP_DEVICE_INTERFACE_DATA { cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32, ..Default::default() };
        // SAFETY: valid set, GUID and out structure with its size set.
        let next =
            unsafe { SetupDiEnumDeviceInterfaces(set.0, None, &GUID_BTHPORT_DEVICE_INTERFACE, index, &mut intf) };
        if let Err(e) = next {
            if e.code() != ERROR_NO_MORE_ITEMS.to_hresult() {
                tracing::debug!("bt_radios: SetupDiEnumDeviceInterfaces stopped: {e}");
            }
            break;
        }
        let Some((path, data)) = set.interface_detail(&intf) else { continue };
        let name = set
            .registry_string(&data, SPDRP_FRIENDLYNAME)
            .or_else(|| set.registry_string(&data, SPDRP_DEVICEDESC))
            .unwrap_or_default();
        let info = local_info(&path);
        let radio = WindowsRadio {
            instance_ids: ancestors(data.DevInst),
            name,
            address: info.as_ref().map(|i| format_address(i.local_info.address)),
            hci_version: info.as_ref().map(|i| i.hci_version),
            lmp_version: info.as_ref().map(|i| i.radio_info.lmp_version),
        };
        tracing::debug!("bt_radios: {radio:?}");
        result.push(radio);
    }
    result
}

fn local_info(path: &str) -> Option<BthLocalRadioInfo> {
    // SAFETY: NUL-terminated path; no access rights asked (the IOCTL is
    // FILE_ANY_ACCESS), shared with Windows' stack.
    let handle = unsafe {
        CreateFileW(
            &HSTRING::from(path),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };
    let handle = match handle {
        Ok(h) => OwnedHandle(h),
        Err(e) => {
            tracing::debug!("bt_radios: cannot open {path}: {e}");
            return None;
        }
    };
    // SAFETY: plain-old-data structure, all zero is valid.
    let mut info: BthLocalRadioInfo = unsafe { std::mem::zeroed() };
    let mut returned = 0u32;
    // SAFETY: valid handle; output buffer is the structure with its size.
    let r = unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_BTH_GET_LOCAL_INFO,
            None,
            0,
            Some(std::ptr::from_mut(&mut info).cast()),
            size_of::<BthLocalRadioInfo>() as u32,
            Some(&mut returned),
            None,
        )
    };
    if let Err(e) = r {
        tracing::debug!("bt_radios: IOCTL_BTH_GET_LOCAL_INFO on {path} failed: {e}");
        return None;
    }
    (returned as usize >= size_of::<BthLocalRadioInfo>()).then_some(info)
}

/// The device node's instance ID and its parents' up to the root
fn ancestors(devinst: u32) -> Vec<String> {
    let mut ids = Vec::new();
    let mut dev = devinst;
    // A few levels reach the USB device; the bound guards a broken tree
    for _ in 0..8 {
        let mut buf = [0u16; 512];
        // SAFETY: valid devinst; the buffer length is passed.
        if unsafe { CM_Get_Device_IDW(dev, &mut buf, 0) } != CR_SUCCESS {
            break;
        }
        ids.push(crate::autostart::utf16_string(&buf));
        let mut parent = 0u32;
        // SAFETY: valid devinst and out pointer.
        if unsafe { CM_Get_Parent(&mut parent, dev, 0) } != CR_SUCCESS {
            break;
        }
        dev = parent;
    }
    ids
}

/// "C0:E3:50:2E:00:09" from BTH_ADDR (the address in its low 48 bits)
pub fn format_address(addr: u64) -> String {
    (0..6).rev().map(|i| format!("{:02X}", (addr >> (i * 8)) & 0xFF)).collect::<Vec<_>>().join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_from_bth_addr() {
        assert_eq!(format_address(0xC0E3_502E_0009), "C0:E3:50:2E:00:09");
        assert_eq!(format_address(0x0000_0000_0001), "00:00:00:00:00:01");
    }

    #[test]
    fn local_radio_info_layout_as_bthioctl_h() {
        // x64 and arm64: BTH_DEVICE_INFO 272 bytes, the whole 296
        assert_eq!(size_of::<BthDeviceInfo>(), 272);
        assert_eq!(std::mem::offset_of!(BthLocalRadioInfo, hci_version), 278);
        assert_eq!(std::mem::offset_of!(BthLocalRadioInfo, radio_info), 280);
        assert_eq!(size_of::<BthLocalRadioInfo>(), 296);
    }

    /// This PC's radios (cargo test -p win-sys list_radios -- --ignored
    /// --nocapture)
    #[test]
    #[ignore = "reads this PC's Bluetooth radios"]
    fn list_radios() {
        for r in radios() {
            println!("{r:?}");
        }
    }

    #[test]
    fn radio_on_its_usb_device() {
        let r = WindowsRadio {
            instance_ids: vec![r"USB\VID_0E8D&PID_7925&MI_00\7&1".into(), r"USB\VID_0E8D&PID_7925\000000000".into()],
            name: "MediaTek Bluetooth Adapter".into(),
            address: None,
            hci_version: None,
            lmp_version: None,
        };
        assert!(r.on_device(r"usb\vid_0e8d&pid_7925\000000000"));
        assert!(!r.on_device(r"USB\VID_0BDA&PID_A729\00E04C000001"));
    }
}
