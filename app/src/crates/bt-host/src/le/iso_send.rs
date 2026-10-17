//! ISO SDU sending on the BTstack thread (v1 Stream's sending half,
//! le_audio_stream.cpp).
//!
//! On HCI_EVENT_CIS_CAN_SEND_NOW the sender takes the next SDU set from the
//! shared jitter buffer ([`le_audio::sdu_queue::SduQueue`], silence on an
//! underrun) and sends each CIS its SDU as one HCI ISO data packet:
//! complete SDU (never fragmented: plan_stream only takes SDUs that fit),
//! sequence number counting up per CIS, no time stamp (the controller
//! places SDUs by sequence number). Two SDUs queued per CIS
//! (hci_set_num_iso_packets_to_queue; queue x CIS within the controller's
//! ISO buffers), and the [`le_audio::sdu_queue::SendGuard`] keeps the sent
//! sets at most queue + 2 intervals ahead of real time. A round starts once
//! every CIS still up got its SDU, so one CIS going down does not stall the
//! others. Logs "LE Audio: no audio for SDU set N", "LE Audio: N SDU sets
//! sent ... U underruns" and, at the end, "LE Audio: stopped sending: N SDU
//! sets (...), S silence, U underruns, ...".

use btstack_sys::Stack;

use super::LeStreamStats;
use super::core::SharedSduQueue;

/// SDUs queued per CIS in the controller
pub const QUEUE_DEPTH: u8 = 2;

/// The sender of the running stream
#[derive(Default)]
pub struct IsoSender {
    sending: bool,
    cis_handles: Vec<u16>,
}

impl IsoSender {
    /// Starts sending on these CIS (plan order) from `queue`, one set per
    /// `frame_us`
    pub fn start(&mut self, stack: &Stack, cis_handles: &[u16], queue: SharedSduQueue, frame_us: u32) {
        todo!("Step 0 skeleton")
    }

    /// Stops sending (logs the totals)
    pub fn stop(&mut self, stack: &Stack) {
        todo!("Step 0 skeleton")
    }

    pub fn sending(&self) -> bool {
        self.sending
    }

    /// HCI_EVENT_CIS_CAN_SEND_NOW
    pub fn on_cis_can_send_now(&mut self, stack: &Stack, packet: &[u8]) {
        todo!("Step 0 skeleton")
    }

    /// A CIS of the stream disconnected: rounds no longer wait for it
    pub fn on_cis_down(&mut self, handle: u16) {
        todo!("Step 0 skeleton")
    }

    /// The guard's timer fired: send the next round if it is due
    pub fn on_timer(&mut self, stack: &Stack) {
        todo!("Step 0 skeleton")
    }

    pub fn stats(&self) -> LeStreamStats {
        todo!("Step 0 skeleton")
    }
}
