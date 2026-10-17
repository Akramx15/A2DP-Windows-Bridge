//! CCP: the Call Control Server (GTBS) on the PC (v1 le_audio_ccp.cpp;
//! WP5).
//!
//! GTBS (TBS 1.0, 0x184C) with the mandatory characteristics plus Bearer
//! Signal Strength (+ Reporting Interval), Incoming Call Target Bearer URI
//! and Call Friendly Name, around `le_audio::ccp_model::CallModel`. The
//! headset drives it through the Call Control Point; the telephony side is
//! the simulation of `LeModuleSetup::ccp_sim` (script started when the
//! first client enables Call State notifications) and `ccp_console`
//! (commands from stdin on a thread, posted to the BTstack thread). Logs
//! "LE Audio: CCP: GTBS handles ..." and "LE Audio: CCP: simulation ...".
//! `add_ccids`: `CCID_GTBS` for streams with the Conversational context.

use super::super::module::{LeModule, LeModuleEntry, LeModuleSetup};

/// BTstack-thread part (no worker hooks)
pub struct CcpModule {}

impl LeModule for CcpModule {
    fn name(&self) -> &'static str {
        "CCP"
    }
}

pub fn create(setup: &mut LeModuleSetup) -> LeModuleEntry {
    todo!("Step 0 skeleton")
}
