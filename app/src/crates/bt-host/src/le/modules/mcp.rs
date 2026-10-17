//! MCP: the Media Control Server (GMCS) on the PC (v1 le_audio_mcp.cpp;
//! WP5).
//!
//! GMCS (0x1849, all characteristics need an encrypted link): Media Player
//! Name, Track Changed, Track Title, Track Duration, Track Position,
//! Playback Speed, Seeking Speed, Media State, Media Control Point, Opcodes
//! Supported, Content Control ID (`CCID_GMCS`, also in the CCID List of a
//! media stream's Enable: `add_ccids`). Long reads whose value changes get
//! 0x80; notifications are cut to ATT_MTU - 3 at a UTF-8 boundary. Commands
//! are refused while the media keys setting is off (`ctx.media_keys()`).
//! The Windows side is `LeModuleSetup::media` (a
//! `le_audio::mcp_model::MediaBackend`); without one the simulated player
//! answers ("MCP: GMCS on the PC (CCID 1), dry run", "MCP: dry run: <OP>").
//! Backend changes reach the BTstack thread through the setup's poster.

use super::super::module::{LeModule, LeModuleEntry, LeModuleSetup};

/// BTstack-thread part (no worker hooks)
pub struct McpModule {}

impl LeModule for McpModule {
    fn name(&self) -> &'static str {
        "MCP"
    }
}

pub fn create(_setup: &mut LeModuleSetup) -> LeModuleEntry {
    // Inert until WP5 implements the profile (WP4: no behaviour)
    LeModuleEntry { module: Box::new(McpModule {}), hooks: None }
}
