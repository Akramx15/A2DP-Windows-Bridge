//! The LE bonds BTstack keeps in `le_audio_bonds.tlv` (btstack_tlv_windows
//! with le_device_db_tlv), read without a stack: the driver restore hands
//! them to Windows (developer mode, issue #86) after the stack let go of the
//! adapter. A running stack holds the file open without sharing, so it
//! cannot be read then.
//!
//! The file: the 8-byte header "BTstack\0", then entries of a big-endian
//! tag, a big-endian length and the value; a later entry of a tag replaces
//! an earlier one, length 0 deletes it. A bond is the tag "BTD" + index
//! with le_device_db_entry_t as its value (60 bytes; 100 with signed
//! writes, whose extra fields are not read).

use std::path::Path;

/// One bond as BTstack keeps it: addresses in display order, keys and Rand
/// most significant octet first
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredLeBond {
    /// 0 public, 1 random
    pub addr_type: u8,
    pub addr: [u8; 6],
    /// All zero: none
    pub irk: [u8; 16],
    pub ltk: [u8; 16],
    pub ediv: u16,
    pub rand: [u8; 8],
    pub key_size: u8,
    pub authenticated: bool,
    pub secure_connection: bool,
}

const MAGIC: &[u8] = b"BTstack";
const HEADER_LEN: usize = 8;
/// btstack_tlv_windows refuses longer values
const MAX_VALUE: usize = 2048;
/// sizeof(le_device_db_entry_t) without ENABLE_LE_SIGNED_WRITE
const ENTRY_LEN: usize = 60;
const ENTRY_LEN_SIGNED: usize = 100;

fn is_bond_tag(tag: u32) -> bool {
    tag >> 8 == u32::from_be_bytes([0, b'B', b'T', b'D'])
}

/// The bonds with an LTK in the file `path`; Err when it cannot be read
/// (missing, or open in a running stack)
pub fn read(path: &Path) -> std::io::Result<Vec<StoredLeBond>> {
    Ok(parse(&std::fs::read(path)?))
}

/// The bonds of the file's bytes, in tag order (an unreadable tail ends the
/// list, as it ends BTstack's reading)
pub fn parse(data: &[u8]) -> Vec<StoredLeBond> {
    if data.len() < HEADER_LEN || &data[..MAGIC.len()] != MAGIC {
        return Vec::new();
    }
    let mut entries: Vec<(u32, &[u8])> = Vec::new();
    let mut at = HEADER_LEN;
    while at + 8 <= data.len() {
        let tag = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        let len = u32::from_be_bytes([data[at + 4], data[at + 5], data[at + 6], data[at + 7]]) as usize;
        at += 8;
        if len > MAX_VALUE || at + len > data.len() {
            break;
        }
        entries.retain(|(t, _)| *t != tag);
        if len > 0 {
            entries.push((tag, &data[at..at + len]));
        }
        at += len;
    }
    entries.sort_by_key(|(t, _)| *t);
    entries.into_iter().filter(|(t, _)| is_bond_tag(*t)).filter_map(|(_, v)| entry(v)).collect()
}

/// le_device_db_entry_t (little-endian, MSVC layout): seq_nr u32, addr_type
/// int, addr[6], irk[16], ltk[16], ediv u16, rand[8], key_size,
/// authenticated, authorized, secure_connection
fn entry(v: &[u8]) -> Option<StoredLeBond> {
    if v.len() != ENTRY_LEN && v.len() != ENTRY_LEN_SIGNED {
        return None;
    }
    let addr_type = i32::from_le_bytes(v[4..8].try_into().ok()?);
    let ltk: [u8; 16] = v[30..46].try_into().ok()?;
    if ltk == [0; 16] || !(0..=1).contains(&addr_type) {
        return None;
    }
    Some(StoredLeBond {
        addr_type: addr_type as u8,
        addr: v[8..14].try_into().ok()?,
        irk: v[14..30].try_into().ok()?,
        ltk,
        ediv: u16::from_le_bytes([v[46], v[47]]),
        rand: v[48..56].try_into().ok()?,
        key_size: v[56],
        authenticated: v[57] != 0,
        secure_connection: v[59] != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bond_value(addr: [u8; 6], ltk: u8, signed: bool) -> Vec<u8> {
        let mut v = vec![0u8; if signed { ENTRY_LEN_SIGNED } else { ENTRY_LEN }];
        v[0..4].copy_from_slice(&7u32.to_le_bytes());
        v[4..8].copy_from_slice(&0i32.to_le_bytes());
        v[8..14].copy_from_slice(&addr);
        v[14..30].copy_from_slice(&[0x11; 16]);
        v[30..46].copy_from_slice(&[ltk; 16]);
        v[46..48].copy_from_slice(&0x1234u16.to_le_bytes());
        v[48..56].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        v[56] = 16;
        v[57] = 1;
        v[59] = 1;
        v
    }

    fn tlv(entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut out = b"BTstack\0".to_vec();
        for (tag, value) in entries {
            out.extend(tag.to_be_bytes());
            out.extend((value.len() as u32).to_be_bytes());
            out.extend(value);
        }
        out
    }

    const BTD0: u32 = u32::from_be_bytes([b'B', b'T', b'D', 0]);
    const BTD1: u32 = u32::from_be_bytes([b'B', b'T', b'D', 1]);

    #[test]
    fn bonds_come_out_as_btstack_keeps_them() {
        let a = [0xC0, 1, 2, 3, 4, 5];
        let b = [0xD0, 1, 2, 3, 4, 5];
        let data = tlv(&[
            (BTD0, bond_value(a, 0x22, false)),
            // Another store's tag: not a bond
            (u32::from_be_bytes(*b"GATT"), vec![1, 2, 3]),
            (BTD1, bond_value(b, 0x33, true)),
            // Stored again: the later one counts
            (BTD0, bond_value(a, 0x44, false)),
        ]);
        let bonds = parse(&data);
        assert_eq!(bonds.len(), 2);
        assert_eq!(bonds[0].addr, a);
        assert_eq!(bonds[0].ltk, [0x44; 16]);
        assert_eq!(bonds[0].irk, [0x11; 16]);
        assert_eq!((bonds[0].ediv, bonds[0].rand, bonds[0].key_size), (0x1234, [1, 2, 3, 4, 5, 6, 7, 8], 16));
        assert!(bonds[0].authenticated && bonds[0].secure_connection);
        assert_eq!(bonds[1].addr, b);
    }

    #[test]
    fn deleted_empty_and_broken_entries_are_left_out() {
        let a = [0xC0, 1, 2, 3, 4, 5];
        // Deleted (length 0)
        assert!(parse(&tlv(&[(BTD0, bond_value(a, 0x22, false)), (BTD0, vec![])])).is_empty());
        // No LTK
        assert!(parse(&tlv(&[(BTD0, bond_value(a, 0, false))])).is_empty());
        // Another size
        assert!(parse(&tlv(&[(BTD0, vec![0; 59])])).is_empty());
        // No header, or a cut file
        assert!(parse(b"nothing").is_empty());
        let mut cut = tlv(&[(BTD0, bond_value(a, 0x22, false))]);
        cut.truncate(cut.len() - 1);
        assert!(parse(&cut).is_empty());
    }
}
