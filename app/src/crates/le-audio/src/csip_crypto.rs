//! The CSIS crypto toolbox (CSIS 1.0.1 section 4; v1 le_audio::csip in
//! le_audio_csip.{h,cpp}).
//!
//! Keys and values are 128-bit numbers in the spec's notation: byte 0 is
//! the most significant octet (the reverse of the over-the-air order). v1
//! ran [`self_test`] when the CSIP module started and logged a failure.

// Step 0 skeleton: remove once the bodies are written (WP2)
#![allow(unused_variables, dead_code)]

/// A 128-bit key or value, most significant octet first
pub type Key = [u8; 16];

/// AES-128 (the security function e)
pub fn aes128(key: &Key, input: &[u8; 16]) -> [u8; 16] {
    todo!("Step 0 skeleton")
}

/// AES-CMAC (RFC 4493)
pub fn aes_cmac(key: &Key, msg: &[u8]) -> [u8; 16] {
    todo!("Step 0 skeleton")
}

/// The salt generation function s1 (4.3)
pub fn s1(m: &[u8]) -> Key {
    todo!("Step 0 skeleton")
}

/// The key derivation function k1 (4.4)
pub fn k1(n: &[u8], salt: &Key, p: &[u8]) -> Key {
    todo!("Step 0 skeleton")
}

/// SIRK encryption function sef (4.5): k1(K, s1("SIRKenc"), "csis") XOR
/// sirk, K = the LTK of the link
pub fn sef(k: &Key, sirk: &Key) -> Key {
    todo!("Step 0 skeleton")
}

/// SIRK decryption function sdf (4.6): the same operation as [`sef`]
pub fn sdf(k: &Key, encrypted_sirk: &Key) -> Key {
    todo!("Step 0 skeleton")
}

/// RSI hash function sih (4.7): the 24-bit hash of `prand` (24 bits) under
/// `sirk`
pub fn sih(sirk: &Key, prand: u32) -> u32 {
    todo!("Step 0 skeleton")
}

/// True if `rsi` (6 octets as advertised: hash, prand; least significant
/// octet first) was generated with `sirk`
pub fn rsi_matches(sirk: &Key, rsi: &[u8; 6]) -> bool {
    todo!("Step 0 skeleton")
}

/// Checks the functions against the sample data of CSIS appendix A
pub fn self_test() -> bool {
    todo!("Step 0 skeleton")
}
