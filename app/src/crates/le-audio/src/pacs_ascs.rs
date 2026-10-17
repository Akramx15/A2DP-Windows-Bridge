//! PACS and ASCS values: PAC records, ASE characteristic values and the
//! ASE Control Point PDUs (v1 le_audio_device.h / le_audio_util.cpp, and the
//! PDUs le_audio_bap.cpp builds inline).

// Step 0 skeleton: remove once the bodies are written (WP2)
#![allow(unused_variables, dead_code)]

use crate::defs::AseState;

// ---- PACS ----

/// The LC3 Codec Specific Capabilities of a PAC record
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lc3Capabilities {
    /// SF_BIT_*
    pub sampling_frequencies: u16,
    /// FD_BIT_*
    pub frame_durations: u8,
    /// Bit n = n+1 channels; default 1 channel (0x01)
    pub channel_counts: u8,
    pub min_octets: u16,
    pub max_octets: u16,
    /// Default 1
    pub max_frames_per_sdu: u8,
}

impl Default for Lc3Capabilities {
    /// v1's defaults: 1 channel, 1 frame per SDU, nothing else supported
    fn default() -> Self {
        Lc3Capabilities {
            sampling_frequencies: 0,
            frame_durations: 0,
            channel_counts: 0x01,
            min_octets: 0,
            max_octets: 0,
            max_frames_per_sdu: 1,
        }
    }
}

/// One PAC record of a Sink PAC characteristic
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PacRecord {
    /// Coding format, company, vendor codec
    pub codec_id: [u8; 5],
    /// Coding format LC3
    pub lc3: bool,
    /// Valid when `lc3`
    pub caps: Lc3Capabilities,
    /// Raw Codec Specific Capabilities LTVs
    pub capabilities: Vec<u8>,
    /// Raw Metadata LTVs
    pub metadata: Vec<u8>,
    /// Metadata Preferred Audio Contexts (0x01), 0 if absent
    pub preferred_contexts: u16,
}

/// Parses a PAC characteristic value (Number_of_PAC_records + records);
/// None if it is malformed (v1 parse_pac_records)
pub fn parse_pac_records(data: &[u8]) -> Option<Vec<PacRecord>> {
    todo!("Step 0 skeleton")
}

/// Short summary of the sink PAC records for the log and bt-host's
/// `LeDeviceInfo::pacs` (v1 DeviceInfo::pacs)
pub fn describe_pac_records(records: &[PacRecord]) -> String {
    todo!("Step 0 skeleton")
}

// ---- ASCS ----

/// One ASE as the client knows it (v1 struct Ase)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ase {
    pub id: u8,
    /// Sink ASE (we send audio to it)
    pub sink: bool,
    pub value_handle: u16,
    pub state: AseState,
    // Codec Configured: the server's preferences
    pub pref_framing: u8,
    pub pref_phy: u8,
    pub pref_rtn: u8,
    pub pref_max_latency_ms: u16,
    pub pd_min_us: u32,
    pub pd_max_us: u32,
    pub pref_pd_min_us: u32,
    pub pref_pd_max_us: u32,
    // QoS Configured and later
    pub cig_id: u8,
    pub cis_id: u8,
    /// Codec Specific Configuration LTVs as configured
    pub codec_config: Vec<u8>,
}

impl Default for Ase {
    fn default() -> Self {
        Ase {
            id: 0,
            sink: true,
            value_handle: 0,
            state: AseState::Unknown,
            pref_framing: 0,
            pref_phy: 0,
            pref_rtn: 0,
            pref_max_latency_ms: 0,
            pd_min_us: 0,
            pd_max_us: 0,
            pref_pd_min_us: 0,
            pref_pd_max_us: 0,
            cig_id: 0,
            cis_id: 0,
            codec_config: Vec::new(),
        }
    }
}

/// Parses an ASE characteristic value into `ase` (id, state and the
/// parameters of that state); false if malformed (v1 parse_ase_value).
/// `sink` and `value_handle` are left as they are.
pub fn parse_ase_value(data: &[u8], ase: &mut Ase) -> bool {
    todo!("Step 0 skeleton")
}

// ---- ASE Control Point PDUs (client to server) ----

/// One ASE of a Config Codec operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigCodec {
    pub ase_id: u8,
    /// TARGET_LATENCY_*
    pub target_latency: u8,
    /// PHY_* (Target_PHY)
    pub target_phy: u8,
    /// Coding format, company, vendor codec
    pub codec_id: [u8; 5],
    /// Codec Specific Configuration LTVs
    pub config: Vec<u8>,
}

/// One ASE of a Config QoS operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigQos {
    pub ase_id: u8,
    pub cig_id: u8,
    pub cis_id: u8,
    /// 24 bits
    pub sdu_interval_us: u32,
    pub framing: u8,
    pub phy: u8,
    pub max_sdu: u16,
    pub rtn: u8,
    pub max_transport_latency_ms: u16,
    /// 24 bits
    pub presentation_delay_us: u32,
}

/// Config Codec (opcode 0x01) for several ASEs of one server
pub fn cp_config_codec(ases: &[ConfigCodec]) -> Vec<u8> {
    todo!("Step 0 skeleton")
}

/// Config QoS (opcode 0x02) for several ASEs of one server
pub fn cp_config_qos(ases: &[ConfigQos]) -> Vec<u8> {
    todo!("Step 0 skeleton")
}

/// Enable (0x03) or Update Metadata (0x07): (ASE id, Metadata LTVs) each
pub fn cp_with_metadata(opcode: u8, ases: &[(u8, Vec<u8>)]) -> Vec<u8> {
    todo!("Step 0 skeleton")
}

/// Receiver Start Ready, Disable, Receiver Stop Ready or Release: the
/// opcode and the ASE ids
pub fn cp_ase_ids(opcode: u8, ase_ids: &[u8]) -> Vec<u8> {
    todo!("Step 0 skeleton")
}

/// The Metadata of Enable: Streaming Audio Contexts (0x02) and, when
/// `ccids` is not empty, the CCID List (0x05) without duplicates
pub fn enable_metadata(contexts: u16, ccids: &[u8]) -> Vec<u8> {
    todo!("Step 0 skeleton")
}

/// One ASE's entry of an ASE Control Point notification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpResponseEntry {
    pub ase_id: u8,
    /// 0x00 success, else the Response_Code
    pub code: u8,
    pub reason: u8,
}

/// An ASE Control Point notification
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpResponse {
    pub opcode: u8,
    pub entries: Vec<CpResponseEntry>,
}

/// Parses an ASE Control Point notification; None if malformed
pub fn parse_cp_response(data: &[u8]) -> Option<CpResponse> {
    todo!("Step 0 skeleton")
}
