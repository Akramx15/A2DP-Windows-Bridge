//! The Wi-Fi around this PC, for AFH (the WLAN API part of v1 afh.cpp)
//!
//! Lists the access points Windows' own background scans found (v1
//! afh_survey_wifi), and which of them each connected interface is
//! associated with: the access point this PC talks to is the one most worth
//! avoiding. No scan is requested: a scan would put more traffic on the
//! very band being measured. Deciding which Bluetooth channels to avoid is
//! bt-host's afh module; this returns plain data.

use std::ffi::c_void;

use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
use windows::Win32::NetworkManagement::WiFi::{
    WLAN_BSS_ENTRY, WLAN_BSS_LIST, WLAN_CONNECTION_ATTRIBUTES, WLAN_INTERFACE_INFO, WLAN_INTERFACE_INFO_LIST,
    WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory, WlanGetNetworkBssList, WlanOpenHandle, WlanQueryInterface,
    dot11_BSS_type_any, wlan_interface_state_connected, wlan_intf_opcode_current_connection,
};

/// One access point (BSS) a Wi-Fi interface heard
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiBss {
    pub ssid: String,
    pub bssid: [u8; 6],
    pub rssi_dbm: i32,
    /// Center frequency of its channel
    pub frequency_mhz: u32,
    /// The 2.4 GHz channel number (1-14), None for 5 / 6 GHz
    pub channel_24ghz: Option<u8>,
    /// This PC's interface is associated with it
    pub connected: bool,
}

/// One Wi-Fi interface of this PC
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiInterface {
    pub description: String,
    pub connected: bool,
    /// BSSID of the access point it is associated with
    pub connected_bssid: Option<[u8; 6]>,
}

/// What `survey` found; both empty on a PC without Wi-Fi
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WifiSurvey {
    pub interfaces: Vec<WifiInterface>,
    pub bss: Vec<WifiBss>,
}

impl WifiSurvey {
    /// 2.4 GHz channels of the access points this PC is connected to
    pub fn connected_channels_24ghz(&self) -> Vec<u8> {
        let mut channels: Vec<u8> = self.bss.iter().filter(|b| b.connected).filter_map(|b| b.channel_24ghz).collect();
        channels.sort_unstable();
        channels.dedup();
        channels
    }
}

/// 2.4 GHz channel number of a center frequency (v1's mapping: 2412-2472
/// MHz in 5 MHz steps are channels 1-13, 2484 MHz is channel 14)
pub fn channel_from_mhz(mhz: u32) -> Option<u8> {
    match mhz {
        2484 => Some(14),
        2412..=2472 if (mhz - 2407).is_multiple_of(5) => Some(((mhz - 2407) / 5) as u8),
        _ => None,
    }
}

/// The access points Windows knows of, from its last background scans.
/// A PC without Wi-Fi (or with the WLAN AutoConfig service stopped) gives
/// an empty survey, not an error, as in v1.
pub fn survey() -> windows::core::Result<WifiSurvey> {
    let mut negotiated = 0u32;
    let mut client = HANDLE::default();
    // SAFETY: client version 2 (Vista and later), valid out pointers.
    let err = unsafe { WlanOpenHandle(2, None, &mut negotiated, &mut client) };
    if err != ERROR_SUCCESS.0 {
        tracing::debug!("wlan: WlanOpenHandle failed ({err}): no Wi-Fi");
        return Ok(WifiSurvey::default());
    }
    let client = Client(client);

    let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    // SAFETY: open client handle, valid out pointer.
    let err = unsafe { WlanEnumInterfaces(client.0, None, &mut list) };
    if err != ERROR_SUCCESS.0 {
        return Err(windows::Win32::Foundation::WIN32_ERROR(err).into());
    }
    let list = WlanMemory(list.cast());
    // SAFETY: WlanEnumInterfaces succeeded, so `list` points to a list
    // whose dwNumberOfItems entries follow in one allocation.
    let interfaces: &[WLAN_INTERFACE_INFO] = unsafe {
        let l = list.0.cast::<WLAN_INTERFACE_INFO_LIST>();
        std::slice::from_raw_parts(std::ptr::addr_of!((*l).InterfaceInfo).cast(), (*l).dwNumberOfItems as usize)
    };

    let mut result = WifiSurvey::default();
    for intf in interfaces {
        let connected = intf.isState == wlan_interface_state_connected;
        let connected_bssid = if connected { connected_bssid(&client, &intf.InterfaceGuid) } else { None };
        result.interfaces.push(WifiInterface {
            description: crate::autostart::utf16_string(&intf.strInterfaceDescription),
            connected,
            connected_bssid,
        });

        let mut bss: *mut WLAN_BSS_LIST = std::ptr::null_mut();
        // SAFETY: open client, a GUID from the enumeration, no SSID filter,
        // valid out pointer.
        let err = unsafe {
            WlanGetNetworkBssList(client.0, &intf.InterfaceGuid, None, dot11_BSS_type_any, false, None, &mut bss)
        };
        if err != ERROR_SUCCESS.0 {
            tracing::debug!("wlan: WlanGetNetworkBssList failed ({err})");
            continue;
        }
        let bss = WlanMemory(bss.cast());
        // SAFETY: as above, dwNumberOfItems entries follow the header.
        let entries: &[WLAN_BSS_ENTRY] = unsafe {
            let l = bss.0.cast::<WLAN_BSS_LIST>();
            std::slice::from_raw_parts(std::ptr::addr_of!((*l).wlanBssEntries).cast(), (*l).dwNumberOfItems as usize)
        };
        for e in entries {
            let frequency_mhz = e.ulChCenterFrequency / 1000; // reported in kHz
            let ssid_len = (e.dot11Ssid.uSSIDLength as usize).min(e.dot11Ssid.ucSSID.len());
            result.bss.push(WifiBss {
                ssid: String::from_utf8_lossy(&e.dot11Ssid.ucSSID[..ssid_len]).into_owned(),
                bssid: e.dot11Bssid,
                rssi_dbm: e.lRssi,
                frequency_mhz,
                channel_24ghz: channel_from_mhz(frequency_mhz),
                connected: connected_bssid == Some(e.dot11Bssid),
            });
        }
    }
    Ok(result)
}

/// BSSID of the access point a connected interface is associated with
fn connected_bssid(client: &Client, interface: &windows::core::GUID) -> Option<[u8; 6]> {
    let mut size = 0u32;
    let mut data: *mut c_void = std::ptr::null_mut();
    // SAFETY: open client, valid GUID and out pointers.
    let err = unsafe {
        WlanQueryInterface(client.0, interface, wlan_intf_opcode_current_connection, None, &mut size, &mut data, None)
    };
    if err != ERROR_SUCCESS.0 || data.is_null() {
        return None;
    }
    let data = WlanMemory(data);
    if (size as usize) < size_of::<WLAN_CONNECTION_ATTRIBUTES>() {
        return None;
    }
    // SAFETY: the opcode returns a WLAN_CONNECTION_ATTRIBUTES of `size`
    // bytes, checked above.
    let attrs = unsafe { &*data.0.cast::<WLAN_CONNECTION_ATTRIBUTES>() };
    Some(attrs.wlanAssociationAttributes.dot11Bssid)
}

/// A WLAN client handle, closed on drop
struct Client(HANDLE);

impl Drop for Client {
    fn drop(&mut self) {
        // SAFETY: opened by WlanOpenHandle, closed only here.
        unsafe { WlanCloseHandle(self.0, None) };
    }
}

/// Memory the WLAN API allocated, freed on drop
struct WlanMemory(*mut c_void);

impl Drop for WlanMemory {
    fn drop(&mut self) {
        // SAFETY: allocated by the WLAN API and freed only here.
        unsafe { WlanFreeMemory(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_mapping_matches_v1() {
        assert_eq!(channel_from_mhz(2412), Some(1));
        assert_eq!(channel_from_mhz(2437), Some(6));
        assert_eq!(channel_from_mhz(2462), Some(11));
        assert_eq!(channel_from_mhz(2472), Some(13));
        assert_eq!(channel_from_mhz(2484), Some(14));
        assert_eq!(channel_from_mhz(2414), None);
        assert_eq!(channel_from_mhz(5180), None);
        assert_eq!(channel_from_mhz(5955), None); // 6 GHz channel 1
    }

    #[test]
    fn survey_runs() {
        // Read-only: the background scan results; empty without Wi-Fi
        let s = survey().unwrap();
        for b in &s.bss {
            assert_eq!(b.channel_24ghz, channel_from_mhz(b.frequency_mhz));
        }
        eprintln!(
            "{} interface(s), {} BSS, connected 2.4 GHz {:?}",
            s.interfaces.len(),
            s.bss.len(),
            s.connected_channels_24ghz()
        );
    }
}
