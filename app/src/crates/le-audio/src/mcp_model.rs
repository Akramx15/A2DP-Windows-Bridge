//! MCP: the media model of the GMCS server on the PC (v1
//! le_audio_mcp.cpp, le_audio_mcp_media.h).
//!
//! The PC is the Media Control Server; the headset reads the player state
//! from GMCS and sends play / pause / stop / next / previous / move relative
//! through the Media Control Point. [`MediaBackend`] is the Windows side
//! (System Media Transport Controls, media keys as a fallback): core
//! implements it with win-sys and hands it to bt-host in `LeConfig::media`.
//! Without one (`--mcp-dry-run`, implied by `--hci-tcp`) the
//! [`SimulatedPlayer`] answers and commands are only logged ("MCP: dry run:
//! <OP>").

// Step 0 skeleton: remove once the bodies are written (WP2)
#![allow(unused_variables, dead_code)]

// GMCS characteristics
pub const UUID_CHR_MEDIA_PLAYER_NAME: u16 = 0x2B93;
pub const UUID_CHR_TRACK_CHANGED: u16 = 0x2B96;
pub const UUID_CHR_TRACK_TITLE: u16 = 0x2B97;
pub const UUID_CHR_TRACK_DURATION: u16 = 0x2B98;
pub const UUID_CHR_TRACK_POSITION: u16 = 0x2B99;
pub const UUID_CHR_PLAYBACK_SPEED: u16 = 0x2B9A;
pub const UUID_CHR_SEEKING_SPEED: u16 = 0x2B9B;
pub const UUID_CHR_MEDIA_STATE: u16 = 0x2BA3;
pub const UUID_CHR_MEDIA_CONTROL_POINT: u16 = 0x2BA4;
pub const UUID_CHR_OPCODES_SUPPORTED: u16 = 0x2BA5;

// Media State (MCS 3.17)
pub const STATE_INACTIVE: u8 = 0x00;
pub const STATE_PLAYING: u8 = 0x01;
pub const STATE_PAUSED: u8 = 0x02;
pub const STATE_SEEKING: u8 = 0x03;

// Media Control Point opcodes (MCS 3.18) A2DPWB knows
pub const OP_PLAY: u8 = 0x01;
pub const OP_PAUSE: u8 = 0x02;
pub const OP_FAST_REWIND: u8 = 0x03;
pub const OP_FAST_FORWARD: u8 = 0x04;
pub const OP_STOP: u8 = 0x05;
pub const OP_MOVE_RELATIVE: u8 = 0x10;
pub const OP_PREVIOUS_TRACK: u8 = 0x30;
pub const OP_NEXT_TRACK: u8 = 0x31;

// Media Control Point result codes (MCS 3.18.2)
pub const RESULT_SUCCESS: u8 = 0x01;
pub const RESULT_OPCODE_NOT_SUPPORTED: u8 = 0x02;
pub const RESULT_MEDIA_PLAYER_INACTIVE: u8 = 0x03;
pub const RESULT_COMMAND_CANNOT_BE_COMPLETED: u8 = 0x04;

/// MCS application error: the value changed between the parts of a long
/// read
pub const ATT_ERROR_VALUE_CHANGED_DURING_READ_LONG: u8 = 0x80;
/// Longest attribute value (ATT)
pub const MAX_TEXT: usize = 512;
/// 2 s off the expected position: Track Position is notified
pub const POSITION_JUMP: i32 = 200;

/// Name for the log, e.g. "PLAY" (the emulator checks "MCP: dry run: <OP>")
pub fn opcode_name(opcode: u8) -> &'static str {
    todo!("Step 0 skeleton")
}

/// Name for the log
pub fn state_name(state: u8) -> &'static str {
    todo!("Step 0 skeleton")
}

/// Name for the log
pub fn result_name(result: u8) -> &'static str {
    todo!("Step 0 skeleton")
}

/// Media Control Point Opcodes Supported bit of an opcode (MCS 3.19), 0 if
/// unknown
pub fn opcode_bit(opcode: u8) -> u32 {
    todo!("Step 0 skeleton")
}

/// UTF-8 text cut to at most `max` octets without splitting a character
pub fn text_value(s: &str, max: usize) -> Vec<u8> {
    todo!("Step 0 skeleton")
}

/// What the headset sees of the Windows media player (v1 mcp::MediaInfo)
#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    /// A media session (or the simulated player) exists
    pub session: bool,
    /// STATE_*
    pub state: u8,
    /// UTF-8
    pub player: String,
    /// UTF-8
    pub title: String,
    /// 0.01 s, -1 unknown
    pub duration: i32,
    /// 0.01 s at `updated_ms`, -1 unknown
    pub position: i32,
    /// Monotonic milliseconds (the caller's clock) of `position`
    pub updated_ms: u64,
    /// Playback rate
    pub rate: f64,
    /// Move Relative / Track Position writes possible
    pub can_seek: bool,
    /// Changes with the track (title, duration, player)
    pub track: u32,
}

impl Default for MediaInfo {
    fn default() -> Self {
        MediaInfo {
            session: false,
            state: STATE_INACTIVE,
            player: String::new(),
            title: String::new(),
            duration: -1,
            position: -1,
            updated_ms: 0,
            rate: 1.0,
            can_seek: false,
            track: 0,
        }
    }
}

impl MediaInfo {
    /// Position at `now_ms` (playing: advanced since `updated_ms` at
    /// `rate`), -1 unknown
    pub fn position_now(&self, now_ms: u64) -> i32 {
        todo!("Step 0 skeleton")
    }

    /// MCS Playback Speed: 64 * log2(rate), -128..127
    pub fn playback_speed(&self) -> i8 {
        todo!("Step 0 skeleton")
    }

    /// Opcodes Supported: Play, Pause, Stop, Previous / Next Track, + Move
    /// Relative if seekable
    pub fn opcodes_supported(&self) -> u32 {
        todo!("Step 0 skeleton")
    }
}

/// Called by a backend (from its own thread) whenever [`MediaBackend::info`]
/// changed, other than the position advancing while playing
pub type ChangedFn = Box<dyn Fn() + Send + Sync>;
/// Called by a backend with the Media Control Point result code (RESULT_*)
pub type DoneFn = Box<dyn FnOnce(u8) + Send>;

/// The Windows side of the media control server (v1 mcp::MediaBackend).
/// Runs a thread of its own, never the BTstack thread: `command` and `seek`
/// return at once and `done` is called later from that thread.
pub trait MediaBackend: Send {
    /// Starts the backend thread
    fn start(&mut self, changed: ChangedFn);
    /// Stops and joins the thread; pending commands are dropped
    fn stop(&mut self);
    /// True for the simulated player (commands are only logged)
    fn dry_run(&self) -> bool;
    /// Latest state
    fn info(&self) -> MediaInfo;
    /// Carries out a Media Control Point opcode (`param`: Move Relative
    /// offset in 0.01 s)
    fn command(&mut self, opcode: u8, param: i32, done: DoneFn);
    /// Track Position write: seek to `position` (0.01 s from the start)
    fn seek(&mut self, position: i32);
}

/// The dry-run player (v1 MediaBackend with dry_run): a simulated player
/// that answers like a real one, for the emulator tests
#[derive(Debug, Default)]
pub struct SimulatedPlayer {
    info: MediaInfo,
}

impl SimulatedPlayer {
    pub fn new() -> SimulatedPlayer {
        todo!("Step 0 skeleton")
    }
}

impl MediaBackend for SimulatedPlayer {
    fn start(&mut self, changed: ChangedFn) {
        todo!("Step 0 skeleton")
    }
    fn stop(&mut self) {
        todo!("Step 0 skeleton")
    }
    fn dry_run(&self) -> bool {
        true
    }
    fn info(&self) -> MediaInfo {
        todo!("Step 0 skeleton")
    }
    fn command(&mut self, opcode: u8, param: i32, done: DoneFn) {
        todo!("Step 0 skeleton")
    }
    fn seek(&mut self, position: i32) {
        todo!("Step 0 skeleton")
    }
}
