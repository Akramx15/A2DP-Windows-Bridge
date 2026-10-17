//! CSIP Set Coordinator (v1 le_audio_csip.cpp; WP5).
//!
//! Earbuds that form a Coordinated Set (one device per ear) stream
//! together. The user names one of them; this module finds and connects
//! the others:
//! - `on_gatt_ready`: CSIS (0x1846, primary or included by CAS): read the
//!   SIRK (0x2B84; plaintext, or encrypted with the LTK: sdf), Coordinated
//!   Set Size (0x2B85), Set Member Rank (0x2B86), Set Member Lock (0x2B87).
//!   Devices with the same SIRK form one set ("member of coordinated set 0,
//!   rank r of 2 (plaintext|encrypted ..."). The SIRK of a bonded device is
//!   kept in <bond_dir>\le_audio_csip.txt.
//! - `on_scan_result`: an advertised RSI (AD 0x2E) checked against every
//!   known SIRK (sih) sets `set_id` / `note` / `le_audio` ("... resolves to
//!   coordinated set 0 (RSI)").
//! - `prepare_group` (worker): waits for the CSIS reads, adds connected
//!   members, scans for RSIs of the set and connects the members found, in
//!   rank order ("CSIP: streaming to coordinated set 0, 2 of 2 members:
//!   ..."). Lock in rank order when every member has Set Member Lock,
//!   released after the stream started (else "without the lock").
//! - The crypto is `le_audio::csip_crypto` (self-check at start).

use super::super::module::{LeGroupHooks, LeModule, LeModuleEntry, LeModuleSetup};

/// BTstack-thread part
pub struct CsipModule {}

impl LeModule for CsipModule {
    fn name(&self) -> &'static str {
        "CSIP"
    }
}

/// Worker part (prepare_group)
pub struct CsipHooks {}

impl LeGroupHooks for CsipHooks {
    fn name(&self) -> &'static str {
        "CSIP"
    }
}

pub fn create(setup: &mut LeModuleSetup) -> LeModuleEntry {
    todo!("Step 0 skeleton")
}
