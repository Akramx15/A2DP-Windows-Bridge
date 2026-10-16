/*
 * LE Audio (experimental) - LC3 / QoS presets and stream planning
 *
 * stream_presets() holds the BAP 1.0 configurations A2DPWB can use: codec
 * configuration settings (BAP Table 3.12, e.g. "48_2" = 48 kHz, 10 ms,
 * 100 octets) combined with a unicast QoS setting (BAP Table 5.2: "_1" low
 * latency, "_2" high reliability). StreamRequest::preset_order says which
 * to try, best first; plan_stream() takes the first one every device of the
 * group supports and allocates the channels to the devices' sink ASEs by
 * audio location.
 *
 * GMAP (le_audio_tmap_gmap.cpp) adds its low-latency presets with
 * add_stream_preset() in on_stack_init() and puts them first in
 * StreamRequest::preset_order from adjust_stream_request().
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_PRESETS_H
#define LE_AUDIO_PRESETS_H

#include "le_audio_device.h"

#include <cstdint>
#include <string>
#include <vector>

namespace le_audio {

struct StreamPreset {
    std::string name;                  /* e.g. "48_2_2" */
    uint32_t sampling_hz = 48000;
    uint16_t frame_us = 10000;         /* 10000 or 7500 */
    uint16_t octets = 100;             /* per codec frame and channel */
    uint8_t target_latency = TARGET_LATENCY_HIGH_RELIABILITY;  /* Config Codec */
    uint8_t framing = 0;               /* 0 unframed */
    uint8_t rtn = 13;                  /* retransmission number */
    uint16_t max_transport_latency_ms = 95;
    uint32_t presentation_delay_us = 40000;
    uint32_t sdu_interval_us() const { return frame_us; }
    uint32_t kbps_per_channel() const { return (uint32_t)octets * 8u * 1000u / frame_us; }
};

/* All known presets (BAP Table 5.2 media / conversational settings) */
const std::vector<StreamPreset> &stream_presets();
/* Add or replace a preset by name (BTstack thread or before streaming) */
void add_stream_preset(const StreamPreset &preset);
const StreamPreset *find_stream_preset(const std::string &name);

/* What the caller wants; modules may change it in adjust_stream_request() */
struct StreamRequest {
    /* Presets to try, best first */
    std::vector<std::string> preset_order = {"48_6_2", "48_4_2", "48_2_2", "32_2_2", "24_2_2", "16_2_2"};
    uint16_t contexts = CONTEXT_MEDIA;  /* Streaming Audio Contexts in Enable */
    uint8_t target_phy = PHY_2M;
    /* One ASE carrying both channels (one CIS) when a device can, rather
     * than one ASE per channel */
    bool prefer_two_channel_ase = true;
};

/* Where an output channel's audio comes from */
enum class ChannelSource { Left, Right, Mono };

struct ChannelPlan {
    uint32_t allocation = 0;           /* audio location bit, 0 = mono */
    ChannelSource source = ChannelSource::Mono;
};

struct CisPlan {
    int device_id = -1;
    uint8_t ase_id = 0;
    uint8_t cis_id = 0;
    uint16_t acl = CON_HANDLE_INVALID;
    uint16_t cis_handle = CON_HANDLE_INVALID;  /* from GAP_SUBEVENT_CIG_CREATED */
    std::vector<ChannelPlan> channels;          /* in SDU order (ascending location bit) */
    uint32_t allocation() const { uint32_t a = 0; for (auto &c : channels) a |= c.allocation; return a; }
    uint16_t sdu_len(uint16_t octets) const { return (uint16_t)(octets * channels.size()); }
};

struct StreamPlan {
    StreamPreset preset;
    uint8_t cig_id = 1;
    uint8_t phy = PHY_2M;
    uint16_t contexts = CONTEXT_MEDIA;
    uint32_t presentation_delay_us = 40000;   /* finalized after Config Codec */
    std::vector<CisPlan> cis;
    int channel_count() const { int n = 0; for (auto &c : cis) n += (int)c.channels.size(); return n; }
    std::string describe() const;
};

/* Picks the preset and the ASE / channel allocation for devices (all
 * connected and ready). False with a reason when no common configuration
 * exists. */
bool plan_stream(const std::vector<const LeAudioDevice *> &devices, const StreamRequest &request,
                 StreamPlan &plan, std::string &why);

/* Codec Specific Configuration LTVs of an ASE for the plan */
std::vector<uint8_t> codec_config_ltvs(const StreamPlan &plan, const CisPlan &cis);

} /* namespace le_audio */

#endif /* LE_AUDIO_PRESETS_H */
