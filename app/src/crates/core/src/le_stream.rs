//! An LE Audio (experimental) streaming session: v1's
//! A2dpService::le_audio_streaming_thread_inner (a2dp_service_le_audio.cpp),
//! on the session thread of [`crate::Service`] for a [`StreamRequest`] whose
//! `le` is Some.
//!
//! Same lifecycle as the A2DP session ([`crate::stream`]): refuse when the
//! setting is off (error.le_audio_disabled) or the capture mode is "app"
//! (error.le_audio_capture); BTstack up with `HostConfig::le_audio` (wait
//! up to 5 s for `BtHost::le_ready`, else error.le_audio_unavailable);
//! find and connect the device and the extra devices (`le_find` first for
//! the address type, then `le_connect`; error.le_audio_connect); the stream
//! request from `LeRequest` (preset -> preset order, low latency ->
//! `le_set_low_latency`); `le_start_stream` (error.le_audio_stream);
//! capture at the LC3 rate into pipeline's LE encode thread, which pushes
//! SDU sets into the returned sink; VolumeSync against
//! `le_remote_volume` / `le_request_volume` (the LeHeadphones target) when
//! a module knows a volume; on `le_stream_lost` stop and set the stream up
//! again ("LE Audio stream restored"; up to 10 times); on stop
//! `le_stop_stream` and `le_disconnect_all`.
//!
//! Step 0 skeleton: `service.rs` does not dispatch here yet (WP6).

// Step 0 skeleton: remove once the session is written and dispatched (WP6)
#![allow(dead_code, unused_variables)]

use std::sync::Arc;

use crate::service::Inner;
use crate::stream::StreamRequest;

/// The session thread for `req` (`req.le` is Some). Returns when stopped or
/// on an error (state reported), as `stream::run` does.
pub(crate) fn run(inner: &Arc<Inner>, req: StreamRequest) {
    todo!("Step 0 skeleton")
}
