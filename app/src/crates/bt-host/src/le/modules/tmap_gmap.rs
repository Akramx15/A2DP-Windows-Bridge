//! TMAP and GMAP (v1 le_audio_tmap_gmap.cpp; WP5).
//!
//! - TMAS on the PC: TMAP Role = Unicast Media Sender; GMAS: GMAP Role =
//!   Unicast Game Gateway, UGG Features = Multiplex | 96 kbps Source
//!   (`le_audio::defs::PC_*`).
//! - The headset's TMAP Role, GMAP Role and UGT Features are read in
//!   `on_gatt_ready` and logged ("TMAP: UMR, GMAP: UGT (sink, multiplex)").
//! - `adjust_stream_request` (worker): with low latency on
//!   (`host.le_low_latency()`) and every device of the group a UGT with the
//!   Sink feature, the "_gr" presets go first with the Game context, or
//!   Media when a device has Media but not Game available ("GMAP: streaming
//!   48_3_gr, contexts 0x0008", "has no Game context available (0x0006):
//!   Media context instead"); otherwise the request stays as it is.

use le_audio::defs::{GMAP_ROLE_UGT, UGT_FEATURE_SINK};

use super::super::LeDeviceId;
use super::super::module::{LeGroupHooks, LeModule, LeModuleEntry, LeModuleSetup};

/// The roles a connected device has (v1 TmapGmapInfo)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TmapGmapInfo {
    /// All reads finished
    pub done: bool,
    pub has_tmas: bool,
    /// Valid when has_tmas
    pub tmap_role: u16,
    pub has_gmas: bool,
    /// Valid when has_gmas
    pub gmap_role: u8,
    /// Valid with GMAP_ROLE_UGT
    pub ugt_features: u8,
}

impl TmapGmapInfo {
    /// A Unicast Game Terminal that renders audio (GMAP low latency
    /// applies)
    pub fn ugt_sink(&self) -> bool {
        self.has_gmas && self.gmap_role & GMAP_ROLE_UGT != 0 && self.ugt_features & UGT_FEATURE_SINK != 0
    }
}

/// BTstack-thread part
pub struct TmapGmapModule {}

impl TmapGmapModule {
    /// What was read of a device; None while unknown (not connected / not
    /// read yet)
    pub fn info(&self, device: LeDeviceId) -> Option<TmapGmapInfo> {
        todo!("Step 0 skeleton")
    }
}

impl LeModule for TmapGmapModule {
    fn name(&self) -> &'static str {
        "TMAP/GMAP"
    }
}

/// Worker part (adjust_stream_request)
pub struct TmapGmapHooks {}

impl LeGroupHooks for TmapGmapHooks {
    fn name(&self) -> &'static str {
        "TMAP/GMAP"
    }
}

pub fn create(setup: &mut LeModuleSetup) -> LeModuleEntry {
    todo!("Step 0 skeleton")
}
