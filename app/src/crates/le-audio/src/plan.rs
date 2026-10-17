//! Stream planning (v1 plan_stream() / choose_streaming_contexts() /
//! codec_config_ltvs() in le_audio_presets.cpp).
//!
//! Per preset in the requested order, the first one every device of the
//! group supports wins: a device with Front Left + Right gets one 2-channel
//! ASE (when its PAC allows 2 channels, the SDU fits and
//! `prefer_two_channel_ase`) or two 1-channel ASEs, else one mono ASE (no
//! Audio_Channel_Allocation: it renders the downmix); a device with one
//! location gets one ASE with it; no location = mono. Left / right audio go
//! to FL / FR only when the group has both, otherwise every channel carries
//! the downmix. One CIG (id 1), CIS ids in plan order, at most [`MAX_CIS`].
//! Every SDU must fit one HCI ISO data packet (SDU + 4 <=
//! ISO_Data_Packet_Length) and every CIS needs an ISO buffer.

// Step 0 skeleton: remove once the bodies are written (WP2)
#![allow(unused_variables, dead_code)]

use crate::defs::{CONTEXT_MEDIA, PHY_2M};
use crate::pacs_ascs::{Ase, PacRecord};
use crate::presets::{DEFAULT_PRESET_ORDER, PresetTable, StreamPreset};

/// CIS of one stream at most (BTstack MAX_NR_CIS)
pub const MAX_CIS: usize = 4;

/// What the caller wants; modules may change it in
/// `LeGroupHooks::adjust_stream_request` (v1 le_audio::StreamRequest;
/// bt-host's `LeStreamRequest`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamRequest {
    /// Preset names to try, best first
    pub preset_order: Vec<String>,
    /// Streaming Audio Contexts in Enable (CONTEXT_*)
    pub contexts: u16,
    /// PHY_*
    pub target_phy: u8,
    /// One ASE carrying both channels (one CIS) when a device can, rather
    /// than one ASE per channel
    pub prefer_two_channel_ase: bool,
}

impl Default for StreamRequest {
    /// v1's defaults: [`DEFAULT_PRESET_ORDER`], Media, 2M PHY, 2-channel ASEs
    fn default() -> Self {
        StreamRequest {
            preset_order: DEFAULT_PRESET_ORDER.iter().map(|s| s.to_string()).collect(),
            contexts: CONTEXT_MEDIA,
            target_phy: PHY_2M,
            prefer_two_channel_ase: true,
        }
    }
}

/// Where an output channel's audio comes from
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelSource {
    Left,
    Right,
    #[default]
    Mono,
}

/// One LC3 channel of a CIS
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChannelPlan {
    /// Audio location bit, 0 = mono
    pub allocation: u32,
    pub source: ChannelSource,
}

/// One CIS / ASE of the plan
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CisPlan {
    /// bt-host's LeDeviceId value
    pub device_id: u32,
    pub ase_id: u8,
    pub cis_id: u8,
    /// ACL handle of the device
    pub acl: u16,
    /// From GAP_SUBEVENT_CIG_CREATED (0xFFFF until then)
    pub cis_handle: u16,
    /// In SDU order (ascending location bit)
    pub channels: Vec<ChannelPlan>,
}

impl CisPlan {
    /// All audio location bits of the channels
    pub fn allocation(&self) -> u32 {
        todo!("Step 0 skeleton")
    }

    /// SDU length: `octets` per channel
    pub fn sdu_len(&self, octets: u16) -> u16 {
        todo!("Step 0 skeleton")
    }
}

/// The configuration of a unicast stream (v1 struct StreamPlan)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamPlan {
    pub preset: StreamPreset,
    pub cig_id: u8,
    pub phy: u8,
    /// StreamRequest::contexts
    pub requested_contexts: u16,
    /// Streaming Audio Contexts sent in Enable ([`choose_streaming_contexts`];
    /// 0 = none available yet, the BAP procedure waits for one before Enable)
    pub contexts: u16,
    /// Finalized after Config Codec (clamped into every ASE's range)
    pub presentation_delay_us: u32,
    pub cis: Vec<CisPlan>,
}

impl StreamPlan {
    /// LC3 channels of all CIS
    pub fn channel_count(&self) -> usize {
        todo!("Step 0 skeleton")
    }

    /// One line for the log and the UI, e.g. "48_2_2 (48 kHz, 10 ms, 80 kbps
    /// per channel), 2 CIS: ..." (v1 StreamPlan::describe; the emulator
    /// tests read it after "Streaming LE Audio:")
    pub fn describe(&self) -> String {
        todo!("Step 0 skeleton")
    }
}

/// What planning needs of one connected, ready device (a copy of v1
/// LeAudioDevice's PACS / ASCS part)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanDevice {
    /// bt-host's LeDeviceId value
    pub id: u32,
    pub acl: u16,
    pub sink_pacs: Vec<PacRecord>,
    pub sink_locations: u32,
    pub available_sink_contexts: u16,
    /// All ASEs (sink and source; only sink ASEs are planned)
    pub ases: Vec<Ase>,
}

/// The controller's ISO buffers (LE Read Buffer Size v2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IsoLimits {
    /// ISO_Data_Packet_Length
    pub packet_length: u16,
    /// Total_Num_ISO_Data_Packets
    pub packet_count: u8,
}

/// Picks the preset and the ASE / channel allocation for the devices (v1
/// plan_stream). Err with the reason (for the log / error text) when no
/// common configuration exists.
pub fn plan_stream(
    devices: &[PlanDevice],
    request: &StreamRequest,
    presets: &PresetTable,
    iso: IsoLimits,
) -> Result<StreamPlan, String> {
    todo!("Step 0 skeleton")
}

/// Streaming Audio Contexts for Enable from each device's Available Sink
/// Contexts: the requested contexts available on every device, else
/// Unspecified if that is available on every device, else 0 (none: Enable
/// must wait)
pub fn choose_streaming_contexts(available: &[u16], requested: u16) -> u16 {
    todo!("Step 0 skeleton")
}

/// Codec Specific Configuration LTVs of an ASE for the plan (sampling
/// frequency, frame duration, channel allocation unless mono, octets per
/// frame, one frame block per SDU)
pub fn codec_config_ltvs(plan: &StreamPlan, cis: &CisPlan) -> Vec<u8> {
    todo!("Step 0 skeleton")
}
