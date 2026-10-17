//! VCP Volume Controller (v1 le_audio_vcp.cpp; WP5).
//!
//! The headset volume follows the Windows volume and the other way round,
//! as AVRCP absolute volume does for A2DP: core's LE session runs
//! VolumeSync against `BtHost::le_remote_volume` / `le_request_volume`,
//! which this module answers ([`VcpHooks`]).
//! - Per device with VCS (0x1844), `on_gatt_ready`: Volume State (0x2B7D)
//!   notifications on, Volume State and Volume Flags (0x2B7F) read; the
//!   Volume Control Point (0x2B7E) is written with the last Change Counter;
//!   "Invalid Change Counter" (0x80) re-reads and sends once more.
//! - The group volume (Volume_Setting 0-255): starts as the first device's,
//!   later devices are set to it; `request_volume` sets it on every device
//!   (unmutes above 0); a device's own change (buttons) sets it and is
//!   reported ("VCP: group volume 100/255 (50/127)"). One Control Point
//!   write per device at a time, newer levels wait.
//! - The scale and values are `le_audio::vcp_scale`.

use super::super::module::{LeGroupHooks, LeModule, LeModuleEntry, LeModuleSetup};

/// BTstack-thread part
pub struct VcpModule {}

impl LeModule for VcpModule {
    fn name(&self) -> &'static str {
        "VCP"
    }
}

/// Any-thread part (remote_volume / request_volume)
pub struct VcpHooks {}

impl LeGroupHooks for VcpHooks {
    fn name(&self) -> &'static str {
        "VCP"
    }
}

pub fn create(_setup: &mut LeModuleSetup) -> LeModuleEntry {
    // Inert until WP5 implements the profile (WP4: no behaviour)
    LeModuleEntry { module: Box::new(VcpModule {}), hooks: Some(std::sync::Arc::new(VcpHooks {})) }
}
