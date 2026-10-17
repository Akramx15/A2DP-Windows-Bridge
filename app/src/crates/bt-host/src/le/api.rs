//! The LE Audio worker API on [`BtHost`] (v1 le_audio::Core's worker
//! functions): blocking calls for core's LE session and the CLI that run
//! their work on the BTstack thread and wait for it with a timeout. A user
//! stop ([`crate::Canceller`], `cancel_pending_waits`) ends the waits, as
//! for A2DP. Never call these from the BTstack thread (a hook, a GATT
//! callback): they would deadlock until their timeout.
//!
//! With `HostConfig::le_audio` None every call fails at once (`le_ready`
//! false, empty results, errors).

use std::time::Duration;

use le_audio::plan::StreamPlan;
use le_audio::sdu_queue::SduSet;

use super::core::{LeCtx, LeDone};
use super::{LeAddr, LeDeviceId, LeDeviceInfo, LeScanResult, LeStreamRequest, LeStreamStarted, LeStreamStats};
use crate::BtHost;

impl BtHost {
    /// LE Audio usable: configured, the LE core initialized and HCI working
    /// (v1 Core::ready)
    pub fn le_ready(&self) -> bool {
        todo!("Step 0 skeleton")
    }

    /// ISO buffers of the controller (LE Read Buffer Size v2):
    /// (ISO_Data_Packet_Length, Total_Num_ISO_Data_Packets). (0, 0): no ISO
    /// support, `le_start_stream` refuses (BTstack's ISO send would loop for
    /// ever).
    pub fn le_iso_buffers(&self) -> (u16, u8) {
        todo!("Step 0 skeleton")
    }

    /// LE scan (extended scanning when the controller has it, else legacy)
    /// for `duration`: all advertisers, `le_audio` marks LE Audio devices
    pub fn le_scan(&self, duration: Duration) -> Vec<LeScanResult> {
        todo!("Step 0 skeleton")
    }

    /// Scans until `addr` is seen (to learn its address type and name).
    /// `addr` may be an identity address or an RPA of a bonded device: an
    /// advertiser whose RPA resolves to the same identity matches too, and
    /// the result's `addr` is the address it advertises now (connect with
    /// that). None on timeout / cancel.
    pub fn le_find(&self, addr: [u8; 6], timeout: Duration) -> Option<LeScanResult> {
        todo!("Step 0 skeleton")
    }

    /// Connects, pairs / encrypts, discovers GATT, reads PACS / ASCS, runs
    /// the modules' on_gatt_ready ("connecting with <addr>", "LE Audio:
    /// <addr> ready (device N)"). An already connected device returns at
    /// once. `addr` may be an RPA or the identity of a bonded device: the id
    /// is the same for both. Err: why, for the log.
    pub fn le_connect(&self, addr: LeAddr, name: &str, timeout: Duration) -> Result<LeDeviceId, String> {
        todo!("Step 0 skeleton")
    }

    /// Disconnects one device (waits up to 3 s)
    pub fn le_disconnect(&self, device: LeDeviceId) {
        todo!("Step 0 skeleton")
    }

    /// Disconnects every LE device (waits up to 3 s)
    pub fn le_disconnect_all(&self) {
        todo!("Step 0 skeleton")
    }

    /// Plans and sets up the unicast stream to `devices` (connected and
    /// ready), after the modules' prepare_group / adjust_stream_request:
    /// plan_stream, the jitter buffer (with the encoded silence set
    /// `silence(&plan)` gives: one silent LC3 SDU per CIS), the BAP
    /// procedure, ISO sending. Returns once audio may flow ("Streaming LE
    /// Audio: <summary>"); push the SDU sets into the returned sink. Err:
    /// why ("adapter does not support LE Audio (no ISO buffers)", no common
    /// configuration, a BAP step that failed or timed out).
    pub fn le_start_stream(
        &self,
        devices: Vec<LeDeviceId>,
        request: LeStreamRequest,
        silence: &dyn Fn(&StreamPlan) -> SduSet,
    ) -> Result<LeStreamStarted, String> {
        todo!("Step 0 skeleton")
    }

    /// Disable / release the ASEs, remove the CIG (devices stay connected;
    /// waits up to 8 s)
    pub fn le_stop_stream(&self) {
        todo!("Step 0 skeleton")
    }

    /// True once after a device or CIS of the running stream was lost (the
    /// session stops and sets the stream up again)
    pub fn le_stream_lost(&self) -> bool {
        todo!("Step 0 skeleton")
    }

    /// Counters of the running (or last) stream
    pub fn le_stream_stats(&self) -> LeStreamStats {
        todo!("Step 0 skeleton")
    }

    /// The devices the LE core knows
    pub fn le_devices(&self) -> Vec<LeDeviceInfo> {
        todo!("Step 0 skeleton")
    }

    /// Headset volume through the modules (VCP), 0-127, None while unknown;
    /// any thread
    pub fn le_remote_volume(&self) -> Option<u8> {
        todo!("Step 0 skeleton")
    }

    /// Sets the headset volume (0-127) through the modules (VCP); false if
    /// no module handles it. Any thread.
    pub fn le_request_volume(&self, level: u8) -> bool {
        todo!("Step 0 skeleton")
    }

    /// "Low latency (GMAP)": read when a stream is set up. Off by default;
    /// per BtHost (set it after each start).
    pub fn le_set_low_latency(&self, on: bool) {
        todo!("Step 0 skeleton")
    }

    pub fn le_low_latency(&self) -> bool {
        todo!("Step 0 skeleton")
    }

    // ---- for the modules' worker hooks (v1 Core::run / call / post) ----

    /// Runs `f` on the BTstack thread and waits for its result; None if
    /// the run loop did not take it within `timeout` (it may still run
    /// later: capture owned data only)
    pub(crate) fn le_run<R: Send + 'static>(
        &self,
        timeout: Duration,
        f: impl FnOnce(&mut LeCtx<'_>) -> R + Send + 'static,
    ) -> Option<R> {
        todo!("Step 0 skeleton")
    }

    /// Runs `f` on the BTstack thread; it calls `done(ok)` when finished,
    /// maybe later from a GATT / timer callback. Waits up to `timeout` or
    /// until cancelled; false on timeout / cancel / done(false).
    pub(crate) fn le_call(&self, timeout: Duration, f: impl FnOnce(&mut LeCtx<'_>, LeDone) + Send + 'static) -> bool {
        todo!("Step 0 skeleton")
    }

    /// Runs `f` on the BTstack thread without waiting
    pub(crate) fn le_post(&self, f: impl FnOnce(&mut LeCtx<'_>) + Send + 'static) {
        todo!("Step 0 skeleton")
    }
}
