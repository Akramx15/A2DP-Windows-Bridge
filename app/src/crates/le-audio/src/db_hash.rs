//! The GATT Database Hash of the PC's GATT server (Core Vol 3 Part G
//! 7.3; v1 GattServer::compute_db_hash in le_audio_gatt_server.cpp).
//!
//! AES-CMAC with a zero key over, per attribute in handle order: for
//! service, include and characteristic declarations and the descriptors
//! (CCC, ...) the handle, the type and the value; for characteristic
//! values only the handle and the type. v1 took that message from BTstack's
//! att_db_util (att_db_util_hash_*) and patched the hash into the
//! Database Hash value in the att_db blob. A stored hash that differs from
//! the current one drops the CCC values stored for bonded clients and makes
//! a Service Changed indication due (bt-host gatt_server).

// Step 0 skeleton: remove once the bodies are written (WP2)
#![allow(unused_variables, dead_code)]

/// An attribute type: a 16-bit UUID or a 128-bit one (little-endian, as on
/// the air)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttUuid {
    Uuid16(u16),
    Uuid128([u8; 16]),
}

/// One attribute of the database, as the hash sees it
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashAttr {
    pub handle: u16,
    pub uuid: AttUuid,
    /// The value for declarations and descriptors; ignored for
    /// characteristic values
    pub value: Vec<u8>,
}

/// The hash input of Core Vol 3 Part G 7.3.1 from the attributes (in
/// handle order)
pub fn hash_input(attrs: &[HashAttr]) -> Vec<u8> {
    todo!("Step 0 skeleton")
}

/// AES-CMAC(zero key, input), least significant octet first as the
/// Database Hash characteristic sends it
pub fn database_hash(input: &[u8]) -> [u8; 16] {
    todo!("Step 0 skeleton")
}

/// Writes `hash` into the value of the attribute with `value_handle` in a
/// BTstack att_db blob (version octet, then per attribute: size, flags,
/// handle, UUID, value; size 0 ends). False if the attribute is not there
/// or its value is not 16 octets.
pub fn patch_att_db(db: &mut [u8], value_handle: u16, hash: &[u8; 16]) -> bool {
    todo!("Step 0 skeleton")
}
