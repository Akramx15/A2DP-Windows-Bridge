//! Tests of the chip tables (src/chips.rs)

use bt_host::chips;

use chips::*;
use std::path::PathBuf;

#[test]
fn chip_table_lookups() {
    assert_eq!(realtek_chip_name(0x8771), Some("RTL8761BU"));
    assert_eq!(realtek_fw_name(0x8771), Some("rtl8761bu_fw"));
    assert_eq!(realtek_cfg_name(0x8771), Some("rtl8761bu_config"));
    assert_eq!(realtek_chip_name(CUSTOM_CHIP_PID), Some("Custom"));
    assert_eq!(realtek_chip_name(0x1234), None);
    // PIDs are unique, and every name follows the file naming
    for (i, c) in REALTEK_CHIPS.iter().enumerate() {
        assert!(REALTEK_CHIPS[i + 1..].iter().all(|o| o.pid != c.pid));
        let stem = c.fw_name.strip_suffix("_fw").unwrap();
        assert_eq!(c.cfg_name, format!("{stem}_config"));
    }
    for m in OEM_TABLE {
        assert!(realtek_chip(m.realtek_pid).is_some(), "OEM entry {:04X}:{:04X}", m.vid, m.pid);
    }
}

#[test]
fn realtek_pid_resolution() {
    let btstack = [(0x0BDA, 0x8771), (0x0BDA, 0xB82C)];
    assert_eq!(resolve_realtek_pid(0x0BDA, 0x8771, &btstack), Some(0x8771));
    // Realtek's VID but not in BTstack's table: a card reader, Wi-Fi
    assert_eq!(resolve_realtek_pid(0x0BDA, 0x0129, &btstack), None);
    // TP-Link UB500
    assert_eq!(resolve_realtek_pid(0x2357, 0x0604, &btstack), Some(0x8771));
    assert_eq!(resolve_realtek_pid(0x8087, 0x0033, &btstack), None);
}

#[test]
fn vendors_and_names() {
    assert_eq!(vendor_for_usb(0x2357, 0x0604, Some(0x8771)), ChipVendor::Realtek);
    assert_eq!(vendor_for_usb(0x0BDA, 0x0129, None), ChipVendor::Realtek);
    assert_eq!(vendor_for_usb(0x8087, 0x0033, None), ChipVendor::Intel);
    assert_eq!(vendor_for_usb(0x0A5C, 0x21E8, None), ChipVendor::Broadcom);
    assert_eq!(vendor_for_usb(0x0B05, 0x17CB, None), ChipVendor::Broadcom); // ASUS BT400
    assert_eq!(vendor_for_usb(0x0B05, 0x1234, None), ChipVendor::Unknown);
    assert_eq!(vendor_for_usb(0x0A12, 0x0001, None), ChipVendor::Csr);
    assert_eq!(vendor_for_usb(0x0E8D, 0x0608, None), ChipVendor::MediaTek);
    assert_eq!(vendor_for_usb(0x0CF3, 0xE300, None), ChipVendor::Qualcomm);
    assert!(is_bluetooth_vendor_vid(0x8087, 0x0033));
    assert!(!is_bluetooth_vendor_vid(0x0BDA, 0x8771));
    assert!(!is_bluetooth_vendor_vid(0x046D, 0xC52B));
    assert_eq!(ChipVendor::Csr.name(), "CSR");
    assert_eq!(company_name(0x005D), Some("Realtek"));
    assert_eq!(company_name(0x000A), Some("Qualcomm (CSR)"));
    assert_eq!(company_name(0x0009), Some("Infineon"));
    assert_eq!(company_name(0x1234), None);
}

#[test]
fn display_names_like_v1() {
    assert_eq!(display_name(0x2357, 0x0604, Some(0x8771), ChipVendor::Realtek), "RTL8761BU (0x2357:0x0604)");
    assert_eq!(display_name(0x8087, 0x0033, None, ChipVendor::Intel), "Intel (0x8087:0x0033) [experimental]");
    assert_eq!(display_name(0x0E8D, 0x0608, None, ChipVendor::MediaTek), "MediaTek (0x0E8D:0x0608) [unsupported]");
    assert_eq!(display_name(0x0A12, 0x0001, None, ChipVendor::Csr), "CSR (0x0A12:0x0001)");
    assert_eq!(display_name(0x0BDA, 0x1234, None, ChipVendor::Realtek), "0x0BDA:0x1234");
    assert_eq!(display_name(0x1234, 0x5678, None, ChipVendor::Unknown), "0x1234:0x5678");
}

#[test]
fn chip_setting_resolution() {
    assert!(is_auto_chip(0, ""));
    assert!(!is_auto_chip(0, "rtl8723bu"));
    assert!(!is_auto_chip(0x8771, ""));
    // Auto: the first detected Realtek adapter
    assert_eq!(resolve_chip_pid(0, "", [None, Some(0xB82C), Some(0x8771)]), 0xB82C);
    assert_eq!(resolve_chip_pid(0, "", [None, None]), 0);
    // A chosen chip wins over what is detected
    assert_eq!(resolve_chip_pid(0x8771, "", [Some(0xB82C)]), 0x8771);
    assert_eq!(resolve_chip_pid(CUSTOM_CHIP_PID, "", []), CUSTOM_CHIP_PID);
    // Not Realtek: no Realtek init
    assert_eq!(resolve_chip_pid(NON_REALTEK_CHIP_PID, "", [Some(0x8771)]), 0);
    // A chip known only by its files: 0, the stem names the files
    assert_eq!(resolve_chip_pid(0, "rtl8723bu", [Some(0x8771)]), 0);
}

#[test]
fn firmware_file_names() {
    assert_eq!(firmware_files(0x8771, ""), Some(("rtl8761bu_fw.bin".to_string(), "rtl8761bu_config.bin".to_string())));
    assert_eq!(
        firmware_files(0, "rtl8723bu"),
        Some(("rtl8723bu_fw.bin".to_string(), "rtl8723bu_config.bin".to_string()))
    );
    assert_eq!(firmware_files(CUSTOM_CHIP_PID, ""), Some(("custom_fw.bin".into(), "custom_config.bin".into())));
    assert_eq!(firmware_files(0, ""), None);
}

#[test]
fn firmware_content_rules() {
    let bin = [0x52u8, 0x54, 0x42, 0x54]; // "RTBT"
    assert!(firmware_content_valid(40_000, &bin));
    assert!(firmware_content_valid(FIRMWARE_MIN_BYTES, &bin));
    assert!(!firmware_content_valid(FIRMWARE_MIN_BYTES - 1, &bin));
    assert!(!firmware_content_valid(40_000, b"<!doctype html><html>"));
    assert!(!firmware_content_valid(40_000, b"<!DOCTYPE html>"));
    assert!(!firmware_content_valid(40_000, b"\r\n  <html lang=\"en\">"));
    assert!(!firmware_content_valid(40_000, b"<HTML>"));
    assert!(firmware_content_valid(40_000, b"<x"));
    assert!(firmware_content_valid(40_000, b""));
}

/// A scratch folder under the target dir's temp, removed on drop
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("a2dpwb_chips_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn firmware_presence_and_scan() {
    let tmp = TempDir::new("scan");
    let dir = &tmp.0;
    let firmware = vec![0x42u8; 30_000];
    std::fs::write(dir.join("rtl8761bu_fw.bin"), &firmware).unwrap();
    std::fs::write(dir.join("rtl8761bu_config.bin"), b"").unwrap(); // empty config is fine
    std::fs::write(dir.join("RTL8723BU_FW.BIN"), &firmware).unwrap(); // unknown chip, other case
    std::fs::write(dir.join("rtl8822bu_fw.bin"), b"<!DOCTYPE html>").unwrap(); // a saved web page
    std::fs::write(dir.join("custom_fw.bin"), &firmware).unwrap();
    std::fs::write(dir.join("_fw.bin"), &firmware).unwrap(); // no stem
    std::fs::write(dir.join("notes.txt"), b"x").unwrap();

    assert_eq!(firmware_present(dir, 0x8771, ""), Some(true));
    assert_eq!(firmware_present(dir, 0xB82C, ""), Some(false)); // HTML, and no config
    assert_eq!(firmware_present(dir, 0, "RTL8723BU"), Some(false)); // no config
    assert_eq!(firmware_present(dir, 0, ""), None);
    assert!(firmware_file_valid(&dir.join("rtl8761bu_fw.bin")));
    assert!(!firmware_file_valid(&dir.join("missing_fw.bin")));

    let found = scan_firmware_files(dir);
    let summary: Vec<(&str, &str, Option<u16>, bool)> =
        found.iter().map(|e| (e.stem.as_str(), e.display_name.as_str(), e.pid, e.has_config)).collect();
    assert_eq!(
        summary,
        [
            ("rtl8761bu", "RTL8761BU", Some(0x8771), true),
            ("rtl8822bu", "RTL8822BU", Some(0xB82C), false),
            ("RTL8723BU", "RTL8723BU", None, false),
        ]
    );
    assert!(scan_firmware_files(&dir.join("missing")).is_empty());
}
