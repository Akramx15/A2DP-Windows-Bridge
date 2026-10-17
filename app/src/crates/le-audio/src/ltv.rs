//! Length-type-value structures (Codec Specific Capabilities /
//! Configuration, Metadata, advertising data): the length octet counts the
//! type and the value (v1 ltv_find / ltv_put in le_audio_util.cpp).

// Step 0 skeleton: remove once the bodies are written (WP2)
#![allow(unused_variables, dead_code, clippy::ptr_arg)]

/// The value of the first LTV of type `ty` in `data`; None if there is none
/// or the structure is malformed before it
pub fn find(data: &[u8], ty: u8) -> Option<&[u8]> {
    todo!("Step 0 skeleton")
}

/// Appends one LTV (`value` at most 254 octets)
pub fn put(out: &mut Vec<u8>, ty: u8, value: &[u8]) {
    todo!("Step 0 skeleton")
}

/// The (type, value) pairs of `data` in order; stops at a malformed or
/// zero-length entry
pub fn iter(data: &[u8]) -> Ltvs<'_> {
    Ltvs { rest: data }
}

/// Iterator of [`iter`]
#[derive(Debug, Clone)]
pub struct Ltvs<'a> {
    rest: &'a [u8],
}

impl<'a> Iterator for Ltvs<'a> {
    type Item = (u8, &'a [u8]);

    fn next(&mut self) -> Option<(u8, &'a [u8])> {
        todo!("Step 0 skeleton")
    }
}
