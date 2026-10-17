//! Saved devices: devices.txt, one "XX:XX:XX:XX:XX:XX|name" per line (v1's
//! format). v1's experimental LE Audio entries (le_audio_devices.txt, or
//! "|le" suffixes from an early prototype) are left alone.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SavedDevice {
    pub address: String,
    pub name: String,
}

const FILE: &str = "devices.txt";

pub fn load(dir: &Path) -> Vec<SavedDevice> {
    let Ok(text) = std::fs::read_to_string(dir.join(FILE)) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let (addr, name) = line.split_once('|')?;
            if name.ends_with("|le") || bt_host::parse_addr(addr).is_none() {
                return None;
            }
            Some(SavedDevice { address: addr.to_uppercase(), name: name.to_string() })
        })
        .collect()
}

fn write(dir: &Path, devices: &[SavedDevice]) -> std::io::Result<()> {
    let text: String = devices.iter().map(|d| format!("{}|{}\n", d.address, d.name)).collect();
    std::fs::write(dir.join(FILE), text)
}

/// Adds or renames a device (an empty name keeps the known one)
pub fn save(dir: &Path, address: &str, name: &str) -> std::io::Result<()> {
    let mut devices = load(dir);
    let address = address.to_uppercase();
    match devices.iter_mut().find(|d| d.address == address) {
        Some(d) => {
            if !name.is_empty() {
                d.name = name.to_string();
            }
        }
        None => devices.push(SavedDevice { address, name: name.to_string() }),
    }
    write(dir, &devices)
}

pub fn remove(dir: &Path, address: &str) -> std::io::Result<()> {
    let mut devices = load(dir);
    devices.retain(|d| !d.address.eq_ignore_ascii_case(address));
    write(dir, &devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_remove() {
        let dir = std::env::temp_dir().join(format!("a2dpwb-dev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), "00:11:22:33:44:55|Old\nbad line\n").unwrap();
        save(&dir, "aa:bb:cc:dd:ee:ff", "Phones").unwrap();
        save(&dir, "00:11:22:33:44:55", "").unwrap();
        let d = load(&dir);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].name, "Old");
        assert_eq!(d[1].address, "AA:BB:CC:DD:EE:FF");
        remove(&dir, "00:11:22:33:44:55").unwrap();
        assert_eq!(load(&dir).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
