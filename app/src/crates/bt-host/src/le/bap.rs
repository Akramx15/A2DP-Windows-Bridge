//! BAP unicast client (v1 le_audio_bap.{h,cpp}).
//!
//! [`read_device`]: PACS client (Sink PAC records, Sink Audio Locations,
//! Available / Supported Audio Contexts) and ASCS client (Sink / Source
//! ASEs, ASE Control Point), notifications enabled, once per connection.
//!
//! [`Unicast`]: the BAP unicast procedure for a StreamPlan over one or more
//! devices, a state machine on the BTstack thread:
//!
//! ```text
//! Config Codec (all planned ASEs)   -> Codec Configured
//! LE Set CIG Parameters (one CIG for the group; framed when an ASE's
//!   Preferred_Framing says unframed is not supported)
//! Config QoS                        -> QoS Configured
//! Enable (Streaming Audio Contexts + CCID List from the modules; waits
//!   for an Available Audio Contexts notification when none is available:
//!   "no audio context available for Enable")
//! LE Create CIS (all at once) + ISO data paths
//! Sink ASEs: the server's Receiver Start Ready -> Streaming
//! ISO SDUs (iso_send)
//! ```
//!
//! and back: Disable -> QoS Configured, HCI Disconnect of the CIS
//! ("disconnecting CIS"; servers may keep an ASE in Releasing until the CIS
//! is gone), Release -> Idle, LE Remove CIG. Each step has a timeout; a
//! failing step tears down what was set up and reports why.

use le_audio::plan::StreamPlan;

use super::LeDeviceId;
use super::core::{LeCallback, LeCtx};

/// Completion of [`read_device`]: true when done (also without PACS /
/// ASCS), false when the link went down or a read failed
pub type ReadDeviceDone = Box<dyn FnOnce(&mut LeCtx<'_>, bool)>;

/// Read PACS and ASCS of a device and subscribe to their notifications.
/// `done(true)` also when the device has neither (then has_pacs / has_ascs
/// stay false).
pub fn read_device(ctx: &mut LeCtx<'_>, dev: LeDeviceId, done: ReadDeviceDone) {
    todo!("Step 0 skeleton")
}

/// Step of the unicast procedure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    ConfigCodec,
    CreateCig,
    ConfigQos,
    Enable,
    CreateCis,
    WaitStreaming,
    Streaming,
    Disable,
    Release,
    RemoveCig,
}

impl Phase {
    pub fn name(self) -> &'static str {
        todo!("Step 0 skeleton")
    }
}

/// Completion of [`Unicast::start`]: Ok, or why it failed
pub type UnicastDone = Box<dyn FnOnce(&mut LeCtx<'_>, Result<(), String>)>;

/// The BAP unicast state machine (one stream at a time)
#[derive(Debug, Default)]
pub struct Unicast {
    phase: Phase,
    plan: Option<StreamPlan>,
}

impl Unicast {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn plan(&self) -> Option<&StreamPlan> {
        self.plan.as_ref()
    }

    pub fn streaming(&self) -> bool {
        self.phase == Phase::Streaming
    }

    pub fn busy(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// Does the stream use this ACL or CIS handle?
    pub fn uses_handle(&self, handle: u16) -> bool {
        todo!("Step 0 skeleton")
    }

    pub fn start(&mut self, ctx: &mut LeCtx<'_>, plan: StreamPlan, done: UnicastDone) {
        todo!("Step 0 skeleton")
    }

    pub fn stop(&mut self, ctx: &mut LeCtx<'_>, done: LeCallback) {
        todo!("Step 0 skeleton")
    }

    // ---- events (from the core / the GATT listeners) ----

    /// An ASE value changed (read or notified)
    pub fn on_ase_update(&mut self, ctx: &mut LeCtx<'_>, dev: LeDeviceId, ase_id: u8) {
        todo!("Step 0 skeleton")
    }

    /// An ASE Control Point notification
    pub fn on_cp_response(&mut self, ctx: &mut LeCtx<'_>, dev: LeDeviceId, value: &[u8]) {
        todo!("Step 0 skeleton")
    }

    /// GAP_SUBEVENT_CIG_CREATED
    pub fn on_cig_created(&mut self, ctx: &mut LeCtx<'_>, packet: &[u8]) {
        todo!("Step 0 skeleton")
    }

    /// GAP_SUBEVENT_CIS_CREATED
    pub fn on_cis_created(&mut self, ctx: &mut LeCtx<'_>, packet: &[u8]) {
        todo!("Step 0 skeleton")
    }

    pub fn on_cig_removed(&mut self, ctx: &mut LeCtx<'_>) {
        todo!("Step 0 skeleton")
    }

    /// An ACL or CIS of the stream went down
    pub fn on_link_lost(&mut self, ctx: &mut LeCtx<'_>, handle: u16) {
        todo!("Step 0 skeleton")
    }

    /// A device's Available Audio Contexts changed (PACS notification)
    pub fn on_available_contexts(&mut self, ctx: &mut LeCtx<'_>, dev: LeDeviceId) {
        todo!("Step 0 skeleton")
    }
}
