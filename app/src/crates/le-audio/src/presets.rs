//! LC3 / QoS presets (v1 le_audio_presets.{h,cpp}).
//!
//! The table holds the BAP 1.0 configurations A2DPWB can use: codec
//! configuration settings (BAP Table 3.12, e.g. "48_2" = 48 kHz, 10 ms, 100
//! octets) combined with a unicast QoS setting (BAP Table 5.2: "_1" low
//! latency, "_2" high reliability), plus GMAP's low-latency settings for a
//! Unicast Game Terminal ("32_1_gr" ... "48_4_gr", GMAP 1.0 Table 3.16:
//! RTN 1, max transport latency 15 ms (7.5 ms frames) / 20 ms (10 ms),
//! presentation delay 10 ms). Every built-in preset is unframed with a
//! presentation delay of 40 ms unless it is a "_gr" one.
//!
//! v1's table (name, Hz, frame us, octets, RTN, max latency ms, target
//! latency):
//! low latency 16_1_1 .. 48_6_1 (RTN 2 below 48 kHz, 5 at 48 kHz),
//! high reliability 16_2_2, 24_2_2, 32_2_2, 48_2_2 (RTN 13, 95 ms),
//! 48_4_2, 48_6_2 (RTN 13, 100 ms), and the six "_gr" presets.

// Step 0 skeleton: remove once the bodies are written (WP2)
#![allow(unused_variables, dead_code)]

/// One preset (v1 struct StreamPreset)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamPreset {
    /// e.g. "48_2_2"
    pub name: String,
    pub sampling_hz: u32,
    /// 10000 or 7500
    pub frame_us: u16,
    /// Per codec frame and channel
    pub octets: u16,
    /// TARGET_LATENCY_* (Config Codec)
    pub target_latency: u8,
    /// 0 unframed
    pub framing: u8,
    /// Retransmission number
    pub rtn: u8,
    pub max_transport_latency_ms: u16,
    pub presentation_delay_us: u32,
}

impl StreamPreset {
    /// One frame per SDU: the SDU interval is the frame duration
    pub fn sdu_interval_us(&self) -> u32 {
        todo!("Step 0 skeleton")
    }

    /// Bit rate of one channel
    pub fn kbps_per_channel(&self) -> u32 {
        todo!("Step 0 skeleton")
    }
}

/// The presets the stream may use, by name: the built-in ones, plus what
/// modules add (v1 stream_presets() / add_stream_preset() /
/// find_stream_preset(), a process-wide table there; one per LE core
/// session here)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetTable {
    presets: Vec<StreamPreset>,
}

impl PresetTable {
    /// The built-in presets (BAP Table 5.2, GMAP Table 3.16)
    pub fn new() -> PresetTable {
        todo!("Step 0 skeleton")
    }

    pub fn presets(&self) -> &[StreamPreset] {
        &self.presets
    }

    /// Adds a preset, or replaces the one with the same name
    pub fn add(&mut self, preset: StreamPreset) {
        todo!("Step 0 skeleton")
    }

    pub fn find(&self, name: &str) -> Option<&StreamPreset> {
        todo!("Step 0 skeleton")
    }
}

impl Default for PresetTable {
    fn default() -> Self {
        PresetTable::new()
    }
}

/// What a stream tries when nothing else is asked for, best first (v1
/// StreamRequest::preset_order)
pub const DEFAULT_PRESET_ORDER: [&str; 6] = ["48_6_2", "48_4_2", "48_2_2", "32_2_2", "24_2_2", "16_2_2"];
