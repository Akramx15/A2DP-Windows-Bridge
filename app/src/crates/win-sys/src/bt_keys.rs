//! The link keys (and LE keys) Windows' Bluetooth keeps for one radio, so that devices it
//! paired with that radio (an adapter BlueCaster now drives with WinUSB) connect
//! to BlueCaster without pairing again.
//!
//! Windows keeps them in
//! HKLM\SYSTEM\CurrentControlSet\Services\BTHPORT\Parameters\Keys\<radio>,
//! one REG_BINARY value of 16 bytes per classic device, named by its
//! address (12 lowercase hex digits, as is the radio's key). LE devices are
//! subkeys there, named by their identity address, with the values `LTK`,
//! `KeyLength`, `EDIV`, `ERand`, `IRK`, `AddressType` and `AuthReq` (a
//! classic device Windows got LE keys of by CTKD has both). Only SYSTEM may
//! read that key: an
//! administrator gets in with the backup privilege (REG_OPTION_BACKUP_RESTORE),
//! so the caller runs elevated (bluecaster-driver.exe `windows-keys`).
//!
//! `write_link_keys` goes the other way when the adapter goes back to
//! Windows: BlueCaster's newer keys of the devices Windows paired too
//! replace Windows' old ones, so those devices connect in Windows without
//! pairing again. Writing takes the restore privilege as well.
//!
//! The key bytes are in the order of HCI's Link Key Notification, the order
//! BTstack and BlueZ keep them in too. LTK and IRK are least significant
//! octet first, as SMP carries them (BlueZ' info file takes them as they
//! are; BTstack keeps them the other way round).

use windows::Win32::Foundation::{
    CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_NOT_ALL_ASSIGNED, GetLastError,
    HANDLE, LUID,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_BACKUP_NAME, SE_PRIVILEGE_ENABLED,
    SE_RESTORE_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_BINARY, REG_DWORD, REG_OPTION_BACKUP_RESTORE, REG_QWORD,
    REG_VALUE_TYPE, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{HSTRING, PCWSTR};

use crate::autostart::{RegKey, check};

const KEYS: &str = r"SYSTEM\CurrentControlSet\Services\BTHPORT\Parameters\Keys";

/// A classic device's link key as Windows keeps it
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsLinkKey {
    /// Display order ("XX" of "XX:.." first)
    pub address: [u8; 6],
    pub key: [u8; 16],
}

/// An LE device's keys as Windows keeps them (a value Windows did not
/// write is None)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowsLeKey {
    /// Display order: the identity address (the subkey's name)
    pub address: [u8; 6],
    /// 0 public, 1 random (static)
    pub address_type: Option<u32>,
    /// Least significant octet first
    pub ltk: [u8; 16],
    /// Encryption key size in octets
    pub key_length: Option<u32>,
    pub ediv: Option<u32>,
    pub erand: Option<u64>,
    /// Least significant octet first
    pub irk: Option<[u8; 16]>,
    /// SMP AuthReq of the pairing
    pub auth_req: Option<u32>,
}

/// What Windows keeps for one radio
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowsKeys {
    pub link_keys: Vec<WindowsLinkKey>,
    pub le_keys: Vec<WindowsLeKey>,
}

/// The classic link keys and LE keys Windows keeps for the radio `radio`
/// (display order). Ok(None): Windows has no keys for that radio (it never
/// paired a device with it). Needs administrator rights.
pub fn read_keys(radio: &[u8; 6]) -> windows::core::Result<Option<WindowsKeys>> {
    enable_privilege(SE_BACKUP_NAME)?;
    let path = format!(r"{KEYS}\{}", hex(radio));
    let mut key = HKEY::default();
    // SAFETY: valid root key, NUL-terminated subkey and out pointer.
    let err = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(path.as_str()),
            Some(REG_OPTION_BACKUP_RESTORE.0),
            KEY_READ,
            &mut key,
        )
    };
    if err == ERROR_FILE_NOT_FOUND {
        tracing::info!("bt_keys: Windows has no keys for the radio {}", hex(radio));
        return Ok(None);
    }
    check(err)?;
    let key = RegKey(key);
    let mut out = Vec::new();
    for index in 0.. {
        let mut name = [0u16; 256];
        let mut name_len = name.len() as u32;
        let mut data = [0u8; 64];
        let mut data_len = data.len() as u32;
        let mut ty = 0u32;
        // SAFETY: open key; the name and data buffers are as long as their
        // lengths say, the out pointers are valid.
        let err = unsafe {
            RegEnumValueW(
                key.0,
                index,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                Some(&mut ty),
                Some(data.as_mut_ptr()),
                Some(&mut data_len),
            )
        };
        if err == ERROR_NO_MORE_ITEMS {
            break;
        }
        // Longer than a link key: not one
        if err == ERROR_MORE_DATA {
            continue;
        }
        check(err)?;
        let name = String::from_utf16_lossy(&name[..name_len as usize]);
        if let Some(k) = link_key(&name, REG_VALUE_TYPE(ty), &data[..data_len as usize]) {
            out.push(k);
        }
    }
    let le_keys = le_keys(&key);
    tracing::info!("bt_keys: {} link key(s), {} LE device(s) for the radio {}", out.len(), le_keys.len(), hex(radio));
    Ok(Some(WindowsKeys { link_keys: out, le_keys }))
}

/// The LE devices: the subkeys of the radio's key named by an address
fn le_keys(radio: &RegKey) -> Vec<WindowsLeKey> {
    let mut out = Vec::new();
    for index in 0.. {
        let mut name = [0u16; 256];
        let mut name_len = name.len() as u32;
        // SAFETY: open key; the name buffer is as long as name_len says.
        let err = unsafe {
            RegEnumKeyExW(
                radio.0,
                index,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            )
        };
        if err == ERROR_NO_MORE_ITEMS || check(err).is_err() {
            break;
        }
        let name = String::from_utf16_lossy(&name[..name_len as usize]);
        let Some(address) = address_from_name(&name) else { continue };
        let mut device = HKEY::default();
        // SAFETY: open parent key, NUL-terminated subkey name, out pointer.
        let err = unsafe {
            RegOpenKeyExW(
                radio.0,
                &HSTRING::from(name.as_str()),
                Some(REG_OPTION_BACKUP_RESTORE.0),
                KEY_READ,
                &mut device,
            )
        };
        if check(err).is_err() {
            tracing::debug!("bt_keys: LE device {name} not readable");
            continue;
        }
        let device = RegKey(device);
        if let Some(k) = le_key(address, |value| query(&device, value)) {
            out.push(k);
        }
    }
    out
}

/// One value of an open key (at most 64 bytes)
fn query(key: &RegKey, name: &str) -> Option<(REG_VALUE_TYPE, Vec<u8>)> {
    let mut data = [0u8; 64];
    let mut len = data.len() as u32;
    let mut ty = REG_VALUE_TYPE(0);
    // SAFETY: open key, NUL-terminated name; the buffer is as long as len says.
    let err = unsafe {
        RegQueryValueExW(key.0, &HSTRING::from(name), None, Some(&mut ty), Some(data.as_mut_ptr()), Some(&mut len))
    };
    check(err).ok()?;
    Some((ty, data[..len as usize].to_vec()))
}

/// An LE device's keys from its values (`value` reads one); None without
/// an LTK (nothing to encrypt with)
fn le_key(address: [u8; 6], value: impl Fn(&str) -> Option<(REG_VALUE_TYPE, Vec<u8>)>) -> Option<WindowsLeKey> {
    let key16 = |name: &str| match value(name)? {
        (REG_BINARY, d) => <[u8; 16]>::try_from(d.as_slice()).ok().filter(|k| *k != [0u8; 16]),
        _ => None,
    };
    let dword = |name: &str| match value(name)? {
        (REG_DWORD, d) => Some(u32::from_le_bytes(d.as_slice().try_into().ok()?)),
        _ => None,
    };
    // A QWORD, or 8 bytes of REG_BINARY; little-endian
    let qword = |name: &str| match value(name)? {
        (REG_QWORD | REG_BINARY, d) => Some(u64::from_le_bytes(d.as_slice().try_into().ok()?)),
        _ => None,
    };
    Some(WindowsLeKey {
        address,
        address_type: dword("AddressType"),
        ltk: key16("LTK")?,
        key_length: dword("KeyLength"),
        ediv: dword("EDIV"),
        erand: qword("ERand"),
        irk: key16("IRK"),
        auth_req: dword("AuthReq"),
    })
}

/// "a1b2c3d4e5f6" (a registry name) to display order
fn address_from_name(name: &str) -> Option<[u8; 6]> {
    if name.len() != 12 {
        return None;
    }
    let mut address = [0u8; 6];
    for (i, a) in address.iter_mut().enumerate() {
        *a = u8::from_str_radix(name.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(address)
}

/// A value of a radio's key that is a device's link key: named by a
/// device address, 16 bytes (the radio's own values, such as its IRK, have
/// other names)
fn link_key(name: &str, ty: REG_VALUE_TYPE, data: &[u8]) -> Option<WindowsLinkKey> {
    if ty != REG_BINARY {
        return None;
    }
    let key: [u8; 16] = data.try_into().ok()?;
    Some(WindowsLinkKey { address: address_from_name(name)?, key })
}

/// "aabbccddeeff", the form of the registry's key names
fn hex(a: &[u8; 6]) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect()
}

/// What becomes of one of BlueCaster's keys when written to Windows
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteAction {
    /// Windows has no key of that device for this radio (never paired it,
    /// or through another radio): nothing is added
    NotPaired,
    /// Windows has this very key already
    Same,
    /// Windows' key differs: replaced, the old one is given back
    Replace([u8; 16]),
}

/// `now`: the value Windows keeps under the device's name, if any
fn write_action(now: Option<(REG_VALUE_TYPE, &[u8])>, ours: &[u8; 16]) -> WriteAction {
    match now {
        Some((REG_BINARY, data)) => match <[u8; 16]>::try_from(data) {
            Ok(old) if old == *ours => WriteAction::Same,
            Ok(old) => WriteAction::Replace(old),
            Err(_) => WriteAction::NotPaired,
        },
        _ => WriteAction::NotPaired,
    }
}

/// What `write_link_keys` did
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WrittenKeys {
    /// Windows' keys that were replaced, as they were (for a backup)
    pub replaced: Vec<WindowsLinkKey>,
    /// Devices Windows had the same key of already
    pub same: Vec<[u8; 6]>,
}

/// Writes BlueCaster's link keys `keys` for the radio `radio` (display
/// order) where Windows keeps a key of that device already, so it uses the
/// newer pairing; devices Windows never paired with that radio are left
/// out. Ok(None): Windows has no keys for that radio. To take effect, it
/// must be written while Windows' Bluetooth does not run the radio (it
/// reads the keys when the radio starts). Needs administrator rights.
pub fn write_link_keys(radio: &[u8; 6], keys: &[WindowsLinkKey]) -> windows::core::Result<Option<WrittenKeys>> {
    enable_privilege(SE_BACKUP_NAME)?;
    enable_privilege(SE_RESTORE_NAME)?;
    let path = format!(r"{KEYS}\{}", hex(radio));
    let mut key = HKEY::default();
    // SAFETY: valid root key, NUL-terminated subkey and out pointer. With
    // both privileges the key opens for reading and writing.
    let err = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(path.as_str()),
            Some(REG_OPTION_BACKUP_RESTORE.0),
            KEY_READ | KEY_SET_VALUE,
            &mut key,
        )
    };
    if err == ERROR_FILE_NOT_FOUND {
        tracing::info!("bt_keys: Windows has no keys for the radio {}; none written", hex(radio));
        return Ok(None);
    }
    check(err)?;
    let key = RegKey(key);
    let mut out = WrittenKeys::default();
    for k in keys {
        let name = hex(&k.address);
        let now = query(&key, &name);
        match write_action(now.as_ref().map(|(t, d)| (*t, d.as_slice())), &k.key) {
            WriteAction::NotPaired => {}
            WriteAction::Same => out.same.push(k.address),
            WriteAction::Replace(old) => {
                // SAFETY: open key, NUL-terminated name, 16 bytes of data.
                let err =
                    unsafe { RegSetValueExW(key.0, &HSTRING::from(name.as_str()), None, REG_BINARY, Some(&k.key)) };
                check(err)?;
                out.replaced.push(WindowsLinkKey { address: k.address, key: old });
            }
        }
    }
    tracing::info!(
        "bt_keys: radio {}: {} key(s) written, {} the same already, {} not paired in Windows",
        hex(radio),
        out.replaced.len(),
        out.same.len(),
        keys.len() - out.replaced.len() - out.same.len()
    );
    Ok(Some(out))
}

/// A privilege on this process's token (an administrator has the backup
/// and restore privileges, disabled): opening a key with
/// REG_OPTION_BACKUP_RESTORE then skips its access check
fn enable_privilege(name: PCWSTR) -> windows::core::Result<()> {
    let mut token = HANDLE::default();
    // SAFETY: GetCurrentProcess is a pseudo handle; `token` is an out pointer.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) }?;
    let mut luid = LUID::default();
    // SAFETY: `name` is one of the static NUL-terminated SE_*_NAME strings;
    // `luid` is an out pointer.
    let r = unsafe { LookupPrivilegeValueW(PCWSTR::null(), name, &mut luid) }.and_then(|()| {
        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
        };
        // SAFETY: `token` was opened with TOKEN_ADJUST_PRIVILEGES; no
        // previous state is asked for.
        unsafe { AdjustTokenPrivileges(token, false, Some(&privileges), 0, None, None) }?;
        // Succeeds also when the token lacks the privilege: then this
        // error is set
        // SAFETY: reads this thread's last error.
        match unsafe { GetLastError() } {
            ERROR_NOT_ALL_ASSIGNED => Err(ERROR_NOT_ALL_ASSIGNED.into()),
            _ => Ok(()),
        }
    });
    // SAFETY: a handle this function opened and owns.
    let _ = unsafe { CloseHandle(token) };
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_device_values_are_link_keys() {
        let key = [0x11u8; 16];
        let k = link_key("a1b2c3d4e5f6", REG_BINARY, &key).unwrap();
        assert_eq!(k.address, [0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6]);
        assert_eq!(k.key, key);
        assert!(link_key("CentralIRK", REG_BINARY, &key).is_none());
        assert!(link_key("a1b2c3d4e5f6", REG_VALUE_TYPE(4), &key).is_none());
        assert!(link_key("a1b2c3d4e5f6", REG_BINARY, &key[..8]).is_none());
        assert!(link_key("a1b2c3d4e5gg", REG_BINARY, &key).is_none());
        assert_eq!(hex(&[0x00, 0x1A, 0x7D, 0xDA, 0x71, 0x13]), "001a7dda7113");
    }

    #[test]
    fn le_values_are_read_as_windows_writes_them() {
        let values = |name: &str| -> Option<(REG_VALUE_TYPE, Vec<u8>)> {
            Some(match name {
                "LTK" => (REG_BINARY, (1..=16).collect()),
                "IRK" => (REG_BINARY, vec![0x22; 16]),
                "KeyLength" => (REG_DWORD, 16u32.to_le_bytes().to_vec()),
                "EDIV" => (REG_DWORD, 0x1234u32.to_le_bytes().to_vec()),
                "ERand" => (REG_QWORD, 0x0102_0304_0506_0708u64.to_le_bytes().to_vec()),
                "AddressType" => (REG_DWORD, 1u32.to_le_bytes().to_vec()),
                "AuthReq" => (REG_DWORD, 0x2Du32.to_le_bytes().to_vec()),
                _ => return None,
            })
        };
        let addr = address_from_name("c0aabbccddee").unwrap();
        let k = le_key(addr, values).unwrap();
        assert_eq!(k.address, [0xC0, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE]);
        assert_eq!(k.ltk[0], 1);
        assert_eq!(k.irk, Some([0x22; 16]));
        assert_eq!((k.key_length, k.ediv, k.erand), (Some(16), Some(0x1234), Some(0x0102_0304_0506_0708)));
        assert_eq!((k.address_type, k.auth_req), (Some(1), Some(0x2D)));
        // ERand as 8 bytes of REG_BINARY too
        let binary = |name: &str| match name {
            "ERand" => Some((REG_BINARY, vec![8, 7, 6, 5, 4, 3, 2, 1])),
            _ => values(name),
        };
        assert_eq!(le_key(addr, binary).unwrap().erand, Some(0x0102_0304_0506_0708));
        // No LTK: nothing to take
        assert!(le_key(addr, |n| if n == "LTK" { None } else { values(n) }).is_none());
        assert!(le_key(addr, |n| if n == "LTK" { Some((REG_BINARY, vec![0; 16])) } else { values(n) }).is_none());
        assert!(address_from_name("MasterIRK").is_none());
    }

    #[test]
    fn only_keys_windows_has_are_replaced() {
        let ours = [0x11u8; 16];
        let old = [0x22u8; 16];
        assert_eq!(write_action(Some((REG_BINARY, &old)), &ours), WriteAction::Replace(old));
        assert_eq!(write_action(Some((REG_BINARY, &ours)), &ours), WriteAction::Same);
        // Never paired with this radio in Windows: nothing is added
        assert_eq!(write_action(None, &ours), WriteAction::NotPaired);
        // Not a link key: left as it is
        assert_eq!(write_action(Some((REG_BINARY, &old[..8])), &ours), WriteAction::NotPaired);
        assert_eq!(write_action(Some((REG_DWORD, &old[..4])), &ours), WriteAction::NotPaired);
    }
}
