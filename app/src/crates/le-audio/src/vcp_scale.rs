//! VCP (Volume Controller) values (v1 le_audio::vcp in le_audio_vcp.{h,cpp}):
//! the AVRCP / VolumeSync scale 0-127 against VCS Volume_Setting 0-255, the
//! Volume State value, Volume Control Point operations, and the
//! `--le-sim-volume` spec of the tests.

// Step 0 skeleton: remove once the bodies are written (WP2)
#![allow(unused_variables, dead_code)]

/// Volume State characteristic (Volume_Setting, Mute, Change_Counter)
pub const UUID_VOLUME_STATE: u16 = 0x2B7D;
pub const UUID_VOLUME_CONTROL_POINT: u16 = 0x2B7E;
pub const UUID_VOLUME_FLAGS: u16 = 0x2B7F;

pub const OP_RELATIVE_VOLUME_DOWN: u8 = 0x00;
pub const OP_RELATIVE_VOLUME_UP: u8 = 0x01;
pub const OP_UNMUTE_RELATIVE_VOLUME_DOWN: u8 = 0x02;
pub const OP_UNMUTE_RELATIVE_VOLUME_UP: u8 = 0x03;
pub const OP_SET_ABSOLUTE_VOLUME: u8 = 0x04;
pub const OP_UNMUTE: u8 = 0x05;
pub const OP_MUTE: u8 = 0x06;

/// VCS application error: the Change_Counter of the write was not the
/// current one (re-read Volume State and send again)
pub const ATT_ERROR_INVALID_CHANGE_COUNTER: u8 = 0x80;
pub const ATT_ERROR_OPCODE_NOT_SUPPORTED: u8 = 0x81;

/// Level 0-127 to Volume_Setting 0-255, rounded to the nearest value.
/// `to_level(to_setting(l)) == l` for every level, so a level sent to the
/// headset and reported back stays the same.
pub const fn to_setting(level: u8) -> u8 {
    // todo!() with a message is not allowed in a const fn
    todo!()
}

/// Volume_Setting 0-255 to level 0-127, rounded to the nearest value
pub const fn to_level(setting: u8) -> u8 {
    // todo!() with a message is not allowed in a const fn
    todo!()
}

/// A Volume State value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VolumeState {
    pub setting: u8,
    pub mute: bool,
    pub change_counter: u8,
}

impl VolumeState {
    /// Parses the 3-octet value; None if shorter
    pub fn parse(value: &[u8]) -> Option<VolumeState> {
        todo!("Step 0 skeleton")
    }
}

/// Set Absolute Volume with the last known Change_Counter
pub fn cp_set_absolute_volume(change_counter: u8, setting: u8) -> [u8; 3] {
    todo!("Step 0 skeleton")
}

/// Unmute (or another operation without a parameter: OP_*)
pub fn cp_simple(opcode: u8, change_counter: u8) -> [u8; 2] {
    todo!("Step 0 skeleton")
}

/// The test-only simulated Windows volume of `--le-sim-volume
/// "<level>[,<seconds>:<level>...]"` (v1 SimulatedWindowsVolume::parse):
/// the simulated Windows volume (0-127) at the start, then the level it
/// changes to at the given seconds after the stream started. core runs
/// VolumeSync against it, so the emulator tests exercise the real sync
/// logic without ever touching the user's output device.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SimVolumeSpec {
    pub start: u8,
    /// (milliseconds after the stream started, level), ascending
    pub steps: Vec<(u32, u8)>,
}

impl SimVolumeSpec {
    /// None if malformed or a level is above 127
    pub fn parse(spec: &str) -> Option<SimVolumeSpec> {
        todo!("Step 0 skeleton")
    }
}
