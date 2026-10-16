/*
 * LE Audio (experimental) - LC3 / QoS presets and stream planning
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_presets.h"

#include <algorithm>
#include <cstdio>
#include <map>

namespace le_audio {

namespace {

StreamPreset make(const char *name, uint32_t hz, uint16_t frame_us, uint16_t octets, uint8_t rtn,
                  uint16_t latency_ms, uint8_t target_latency) {
    StreamPreset p;
    p.name = name;
    p.sampling_hz = hz;
    p.frame_us = frame_us;
    p.octets = octets;
    p.rtn = rtn;
    p.max_transport_latency_ms = latency_ms;
    p.target_latency = target_latency;
    p.presentation_delay_us = 40000;
    return p;
}

std::vector<StreamPreset> &table() {
    const uint8_t LL = TARGET_LATENCY_LOW, HR = TARGET_LATENCY_HIGH_RELIABILITY;
    static std::vector<StreamPreset> t = {
        /* BAP 1.0 Table 5.2, low latency */
        make("16_1_1", 16000, 7500, 30, 2, 8, LL),
        make("16_2_1", 16000, 10000, 40, 2, 10, LL),
        make("24_1_1", 24000, 7500, 45, 2, 8, LL),
        make("24_2_1", 24000, 10000, 60, 2, 10, LL),
        make("32_1_1", 32000, 7500, 60, 2, 8, LL),
        make("32_2_1", 32000, 10000, 80, 2, 10, LL),
        make("48_1_1", 48000, 7500, 75, 5, 15, LL),
        make("48_2_1", 48000, 10000, 100, 5, 20, LL),
        make("48_3_1", 48000, 7500, 90, 5, 15, LL),
        make("48_4_1", 48000, 10000, 120, 5, 20, LL),
        make("48_5_1", 48000, 7500, 117, 5, 15, LL),
        make("48_6_1", 48000, 10000, 155, 5, 20, LL),
        /* high reliability (media) */
        make("16_2_2", 16000, 10000, 40, 13, 95, HR),
        make("24_2_2", 24000, 10000, 60, 13, 95, HR),
        make("32_2_2", 32000, 10000, 80, 13, 95, HR),
        make("48_2_2", 48000, 10000, 100, 13, 95, HR),
        make("48_4_2", 48000, 10000, 120, 13, 100, HR),
        make("48_6_2", 48000, 10000, 155, 13, 100, HR),
    };
    return t;
}

/* Does one of the device's LC3 sink PAC records allow this preset with the
 * given number of channels per ASE? */
bool device_supports(const LeAudioDevice &d, const StreamPreset &p, int channels_per_ase) {
    uint16_t sf_bit = sf_hz_to_cap_bit(p.sampling_hz);
    uint8_t fd_bit = p.frame_us == 7500 ? FD_BIT_7500US : FD_BIT_10000US;
    for (const auto &r : d.sink_pacs) {
        if (!r.lc3) continue;
        if (!(r.caps.sampling_frequencies & sf_bit)) continue;
        if (!(r.caps.frame_durations & fd_bit)) continue;
        if (r.caps.max_octets && (p.octets < r.caps.min_octets || p.octets > r.caps.max_octets)) continue;
        if (!(r.caps.channel_counts & (1u << (channels_per_ase - 1)))) continue;
        if (r.caps.max_frames_per_sdu < channels_per_ase) continue;
        return true;
    }
    return false;
}

bool device_two_channel_ase(const LeAudioDevice &d) {
    for (const auto &r : d.sink_pacs)
        if (r.lc3 && (r.caps.channel_counts & 0x02) && r.caps.max_frames_per_sdu >= 2) return true;
    return false;
}

} /* namespace */

const std::vector<StreamPreset> &stream_presets() { return table(); }

void add_stream_preset(const StreamPreset &preset) {
    auto &t = table();
    for (auto &p : t) {
        if (p.name == preset.name) { p = preset; return; }
    }
    t.push_back(preset);
}

const StreamPreset *find_stream_preset(const std::string &name) {
    for (const auto &p : table())
        if (p.name == name) return &p;
    return nullptr;
}

std::string StreamPlan::describe() const {
    char buf[200];
    snprintf(buf, sizeof(buf), "LC3 %u kHz %s ms %u octets (%u kbps/ch), %d ch over %u CIS, %s",
             preset.sampling_hz / 1000, preset.frame_us == 7500 ? "7.5" : "10", preset.octets,
             preset.kbps_per_channel(), channel_count(), (unsigned)cis.size(), preset.name.c_str());
    return buf;
}

bool plan_stream(const std::vector<const LeAudioDevice *> &devices, const StreamRequest &request,
                 StreamPlan &plan, std::string &why) {
    plan = StreamPlan();
    if (devices.empty()) { why = "no device"; return false; }

    /* 1. ASEs and channels per device, by audio location */
    std::vector<CisPlan> cis;
    for (const LeAudioDevice *d : devices) {
        std::vector<uint8_t> sink_ases;
        for (const auto &a : d->ases)
            if (a.sink) sink_ases.push_back(a.id);
        std::sort(sink_ases.begin(), sink_ases.end());
        if (sink_ases.empty()) {
            why = d->name.empty() ? d->addr.str() : d->name;
            why += ": no sink ASE (not an LE Audio sink?)";
            return false;
        }
        uint32_t lr = d->sink_locations & (LOCATION_FRONT_LEFT | LOCATION_FRONT_RIGHT);
        auto add = [&](uint8_t ase_id, std::vector<uint32_t> locs) {
            CisPlan c;
            c.device_id = d->id;
            c.ase_id = ase_id;
            c.acl = d->acl;
            for (uint32_t l : locs) c.channels.push_back({l, ChannelSource::Mono});
            cis.push_back(c);
        };
        if (popcount32(lr) == 2) {
            bool two = device_two_channel_ase(*d);
            if (two && (request.prefer_two_channel_ase || sink_ases.size() < 2))
                add(sink_ases[0], {LOCATION_FRONT_LEFT, LOCATION_FRONT_RIGHT});
            else if (sink_ases.size() >= 2) {
                add(sink_ases[0], {LOCATION_FRONT_LEFT});
                add(sink_ases[1], {LOCATION_FRONT_RIGHT});
            } else {
                /* Stereo device that takes one channel only: mono */
                add(sink_ases[0], {LOCATION_FRONT_LEFT});
            }
        } else if (popcount32(lr) == 1) {
            add(sink_ases[0], {lr});
        } else {
            add(sink_ases[0], {LOCATION_MONO});
        }
    }
    if (cis.size() > 4) { why = "more than 4 CIS needed"; return false; }

    /* 2. Channel sources: left / right only when the group has both */
    bool has_left = false, has_right = false;
    for (const auto &c : cis)
        for (const auto &ch : c.channels) {
            has_left |= ch.allocation == LOCATION_FRONT_LEFT;
            has_right |= ch.allocation == LOCATION_FRONT_RIGHT;
        }
    bool stereo = has_left && has_right;
    for (auto &c : cis)
        for (auto &ch : c.channels)
            ch.source = !stereo ? ChannelSource::Mono
                      : ch.allocation == LOCATION_FRONT_LEFT ? ChannelSource::Left
                      : ch.allocation == LOCATION_FRONT_RIGHT ? ChannelSource::Right : ChannelSource::Mono;
    for (size_t i = 0; i < cis.size(); i++) cis[i].cis_id = (uint8_t)i;

    /* 3. First preset in the requested order every device supports */
    const StreamPreset *chosen = nullptr;
    for (const auto &name : request.preset_order) {
        const StreamPreset *p = find_stream_preset(name);
        if (!p) continue;
        bool ok = true;
        for (const auto &c : cis) {
            const LeAudioDevice *d = nullptr;
            for (const LeAudioDevice *x : devices) if (x->id == c.device_id) d = x;
            ok &= d && device_supports(*d, *p, (int)c.channels.size());
        }
        if (ok) { chosen = p; break; }
    }
    if (!chosen) {
        why = "no LC3 configuration supported by all devices (sink PAC records)";
        return false;
    }

    plan.preset = *chosen;
    plan.cis = std::move(cis);
    plan.phy = request.target_phy;
    plan.presentation_delay_us = chosen->presentation_delay_us;
    /* Streaming context: what was asked if the devices have it available */
    plan.contexts = request.contexts;
    for (const LeAudioDevice *d : devices)
        if (d->available_sink_contexts && !(d->available_sink_contexts & request.contexts))
            plan.contexts = CONTEXT_UNSPECIFIED;
    return true;
}

std::vector<uint8_t> codec_config_ltvs(const StreamPlan &plan, const CisPlan &cis) {
    std::vector<uint8_t> out;
    uint8_t v[4];
    v[0] = sf_hz_to_code(plan.preset.sampling_hz);
    ltv_put(out, CFG_SAMPLING_FREQUENCY, v, 1);
    v[0] = plan.preset.frame_us == 7500 ? 0x00 : 0x01;
    ltv_put(out, CFG_FRAME_DURATION, v, 1);
    uint32_t alloc = cis.allocation();
    if (alloc != 0) {
        v[0] = (uint8_t)alloc; v[1] = (uint8_t)(alloc >> 8); v[2] = (uint8_t)(alloc >> 16); v[3] = (uint8_t)(alloc >> 24);
        ltv_put(out, CFG_CHANNEL_ALLOCATION, v, 4);
    }
    v[0] = (uint8_t)plan.preset.octets; v[1] = (uint8_t)(plan.preset.octets >> 8);
    ltv_put(out, CFG_OCTETS_PER_FRAME, v, 2);
    v[0] = 1;
    ltv_put(out, CFG_FRAME_BLOCKS_PER_SDU, v, 1);
    return out;
}

} /* namespace le_audio */
