//! CCP: the call model of the GTBS server on the PC (v1 le_audio::ccp in
//! le_audio_ccp.{h,cpp}).
//!
//! A2DPWB has no telephony: Windows has no generic call-state API and the LE
//! Audio path has no microphone. The GTBS server (bt-host's CCP module) is
//! spec-correct around this in-memory model, which two sides drive:
//! - the headset, through the Call Control Point (Accept, Terminate, Local
//!   Hold / Retrieve, Originate): [`CallModel::accept`] ...;
//! - the telephony side through the events ([`CallModel::incoming`] ...):
//!   today only the simulation (`--ccp-sim`, `--ccp-console`,
//!   [`CallModel::simulate`]); a real integration would implement
//!   [`Backend`].
//!
//! Simulation commands: `incoming <uri>[|<friendly name>[|<target uri>]]`,
//! `alerting [<index>]`, `answered [<index>]`, `remote-hold [<index>]`,
//! `remote-retrieve [<index>]`, `hangup [<index> [<reason>]]` (default
//! reason 0x02 remote party ended; 3 = the server ended it), `status`.
//! `<index>` defaults to the most recent call.

// Step 0 skeleton: remove once the bodies are written (WP2)
#![allow(unused_variables, dead_code)]

/// TBS 1.0 Call State values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CallState {
    #[default]
    Incoming,
    /// Outgoing, not yet ringing at the far end
    Dialing,
    /// Outgoing, ringing at the far end
    Alerting,
    Active,
    LocallyHeld,
    RemotelyHeld,
    LocallyAndRemotelyHeld,
}

impl CallState {
    /// The TBS value (0x00 Incoming ... 0x06)
    pub fn value(self) -> u8 {
        todo!("Step 0 skeleton")
    }

    /// Name for the log
    pub fn name(self) -> &'static str {
        todo!("Step 0 skeleton")
    }
}

// Call Flags
pub const CALL_FLAG_OUTGOING: u8 = 0x01;
pub const CALL_FLAG_WITHHELD_BY_SERVER: u8 = 0x02;
pub const CALL_FLAG_WITHHELD_BY_NETWORK: u8 = 0x04;

// Call Control Point opcodes
pub const OP_ACCEPT: u8 = 0x00;
pub const OP_TERMINATE: u8 = 0x01;
pub const OP_LOCAL_HOLD: u8 = 0x02;
pub const OP_LOCAL_RETRIEVE: u8 = 0x03;
pub const OP_ORIGINATE: u8 = 0x04;
pub const OP_JOIN: u8 = 0x05;

// Call Control Point notification result codes
pub const RESULT_SUCCESS: u8 = 0x00;
pub const RESULT_OPCODE_NOT_SUPPORTED: u8 = 0x01;
pub const RESULT_OPERATION_NOT_POSSIBLE: u8 = 0x02;
pub const RESULT_INVALID_CALL_INDEX: u8 = 0x03;
pub const RESULT_STATE_MISMATCH: u8 = 0x04;
pub const RESULT_LACK_OF_RESOURCES: u8 = 0x05;
pub const RESULT_INVALID_OUTGOING_URI: u8 = 0x06;

// Termination Reason codes
pub const REASON_IMPROPER_URI: u8 = 0x00;
pub const REASON_CALL_FAILED: u8 = 0x01;
pub const REASON_REMOTE_ENDED: u8 = 0x02;
pub const REASON_SERVER_ENDED: u8 = 0x03;
pub const REASON_LINE_BUSY: u8 = 0x04;
pub const REASON_NETWORK_CONGESTION: u8 = 0x05;
pub const REASON_CLIENT_TERMINATED: u8 = 0x06;
pub const REASON_NO_SERVICE: u8 = 0x07;
pub const REASON_NO_ANSWER: u8 = 0x08;
pub const REASON_UNSPECIFIED: u8 = 0x09;

/// ATT application error of TBS: the value changed between the parts of a
/// long read
pub const ATT_ERROR_VALUE_CHANGED_DURING_READ_LONG: u8 = 0x80;

/// Calls the server keeps at most (Originate / incoming beyond: lack of
/// resources)
pub const MAX_CALLS: usize = 4;

/// An item of Bearer List Current Calls has a 1-octet length: 3 + URI
pub const MAX_URI: usize = 252;

/// One call
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Call {
    /// 1-255
    pub index: u8,
    pub state: CallState,
    /// CALL_FLAG_*
    pub flags: u8,
    /// Remote party, e.g. "tel:+15551234567"
    pub uri: String,
    /// May be empty
    pub friendly_name: String,
    /// Incoming: our URI the call came in on (may be empty)
    pub target_uri: String,
}

/// The telephony side, asked before the model applies a request of the
/// headset (v1 ccp::Backend). The defaults are the simulation: everything
/// is allowed.
pub trait Backend {
    fn provider_name(&self) -> String {
        "A2DPWB".into()
    }
    /// Uniform Caller Identifier (Assigned Numbers); "un000" = unknown
    fn uci(&self) -> String {
        "un000".into()
    }
    /// Bearer Technology: 0x04 Wi-Fi (TBS has no "wired IP" value)
    fn technology(&self) -> u8 {
        0x04
    }
    /// Comma-separated URI schemes (Originate checks them)
    fn uri_schemes(&self) -> String {
        "tel,sip".into()
    }
    /// Local Hold / Local Retrieve available (Call Control Point Optional
    /// Opcodes)
    fn supports_local_hold(&self) -> bool {
        true
    }
    // Requests of the headset; return a RESULT_* code
    fn accept(&mut self, _call: &Call) -> u8 {
        RESULT_SUCCESS
    }
    fn terminate(&mut self, _call: &Call) -> u8 {
        RESULT_SUCCESS
    }
    fn hold(&mut self, _call: &Call) -> u8 {
        RESULT_SUCCESS
    }
    fn retrieve(&mut self, _call: &Call) -> u8 {
        RESULT_SUCCESS
    }
    fn originate(&mut self, _uri: &str) -> u8 {
        RESULT_SUCCESS
    }
}

/// The simulation backend (allows everything)
#[derive(Debug, Clone, Copy, Default)]
pub struct SimBackend;

impl Backend for SimBackend {}

/// What changed since [`CallModel::take_changes`] was last called: the
/// server notifies Call State and Bearer List Current Calls (and Incoming
/// Call, Call Friendly Name, ...) when `calls`, and one Termination Reason
/// per entry of `terminations`
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Changes {
    pub calls: bool,
    /// (call index, REASON_*)
    pub terminations: Vec<(u8, u8)>,
}

/// The calls of the bearer
#[derive(Debug, Clone, Default)]
pub struct CallModel {
    calls: Vec<Call>,
    next_index: u8,
    changes: Changes,
}

impl CallModel {
    pub fn new() -> CallModel {
        todo!("Step 0 skeleton")
    }

    /// The current calls, oldest first
    pub fn calls(&self) -> &[Call] {
        &self.calls
    }

    pub fn find(&self, index: u8) -> Option<&Call> {
        todo!("Step 0 skeleton")
    }

    // ---- Events of the telephony side ----

    /// A call comes in: its call index, None if the list is full
    pub fn incoming(&mut self, uri: &str, friendly_name: &str, target_uri: &str) -> Option<u8> {
        todo!("Step 0 skeleton")
    }
    /// An outgoing call rings at the far end (Dialing -> Alerting)
    pub fn remote_alerting(&mut self, index: u8) -> bool {
        todo!("Step 0 skeleton")
    }
    /// The far end answered an outgoing call (Dialing / Alerting -> Active)
    pub fn remote_answered(&mut self, index: u8) -> bool {
        todo!("Step 0 skeleton")
    }
    /// The far end put the call on hold / took it back
    pub fn remote_hold(&mut self, index: u8, held: bool) -> bool {
        todo!("Step 0 skeleton")
    }
    /// The call ended (REASON_*)
    pub fn ended(&mut self, index: u8, reason: u8) -> bool {
        todo!("Step 0 skeleton")
    }

    // ---- Requests of the headset (Call Control Point): RESULT_* ----

    /// Accept an incoming call (other active calls go on local hold)
    pub fn accept(&mut self, backend: &mut dyn Backend, index: u8) -> u8 {
        todo!("Step 0 skeleton")
    }
    pub fn terminate(&mut self, backend: &mut dyn Backend, index: u8) -> u8 {
        todo!("Step 0 skeleton")
    }
    pub fn local_hold(&mut self, backend: &mut dyn Backend, index: u8) -> u8 {
        todo!("Step 0 skeleton")
    }
    pub fn local_retrieve(&mut self, backend: &mut dyn Backend, index: u8) -> u8 {
        todo!("Step 0 skeleton")
    }
    /// Originate: (RESULT_*, the new call's index or 0)
    pub fn originate(&mut self, backend: &mut dyn Backend, uri: &str) -> (u8, u8) {
        todo!("Step 0 skeleton")
    }

    /// Notifications due since the last call
    pub fn take_changes(&mut self) -> Changes {
        std::mem::take(&mut self.changes)
    }

    // ---- Characteristic values ----

    /// Bearer List Current Calls
    pub fn list_value(&self) -> Vec<u8> {
        todo!("Step 0 skeleton")
    }
    /// Call State
    pub fn state_value(&self) -> Vec<u8> {
        todo!("Step 0 skeleton")
    }
    /// Incoming Call (the latest incoming call; empty if none)
    pub fn incoming_value(&self) -> Vec<u8> {
        todo!("Step 0 skeleton")
    }
    /// Incoming Call Target Bearer URI
    pub fn target_value(&self) -> Vec<u8> {
        todo!("Step 0 skeleton")
    }
    /// Call Friendly Name
    pub fn friendly_value(&self) -> Vec<u8> {
        todo!("Step 0 skeleton")
    }

    // ---- Simulation ----

    /// One simulation command (see the module docs); Err with why it
    /// cannot be done. Ok with the text to log ("status" lists the calls).
    pub fn simulate(&mut self, command: &str) -> Result<String, String> {
        todo!("Step 0 skeleton")
    }
}

/// One step of a `--ccp-sim` script
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimStep {
    /// Milliseconds after the first CCP client enabled Call State
    /// notifications
    pub at_ms: u32,
    pub command: String,
}

/// Parses `--ccp-sim "<t>:<cmd>;<t>:<cmd>;..."` (t in seconds, may have a
/// fraction); None if malformed
pub fn parse_sim_script(spec: &str) -> Option<Vec<SimStep>> {
    todo!("Step 0 skeleton")
}
