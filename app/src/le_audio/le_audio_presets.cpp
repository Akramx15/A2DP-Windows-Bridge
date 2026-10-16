/*
 * LE Audio (experimental) - LC3 / QoS presets and stream planning
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_presets.h"
#include "le_audio_core.h"

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

/* GMAP low-latency setting: unframed, RTN 1, presentation delay 10 ms */
StreamPreset make_gr(const char *name, uint32_t hz, uint16_t frame_us, uint16_t octets, uint16_t latency_ms) {
    StreamPreset p = make(name, hz, frame_us, octets, 1, latency_ms, TARGET_LATENCY_LOW);
    p.presentation_delay_us = 10000;
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
        /* GMAP 1.0 Table 3.16, UGG -> UGT (game audio to a gaming headset):
         * RTN 1, max transport latency 15 ms (7.5 ms frames) / 20 ms (10 ms),
         * presentation delay 10 ms. Used by le_audio_tmap_gmap.cpp. */
        make_gr("32_1_gr", 32000, 7500, 60, 15),
        make_gr("32_2_gr", 32000, 10000, 80, 20),
        make_gr("48_1_gr", 48000, 7500, 75, 15),
        make_gr("48_2_gr", 48000, 10000, 100, 20),
        make_gr("48_3_gr", 48000, 7500, 90, 15),
        make_gr("48_4_gr", 48000, 10000, 120, 20),
    };
    return t;
}

/* Does one of the device's LC3 sink PAC records allow this preset with the
 * given number of channels per ASE? Audio_Channel_Counts says how many
 * channels an ASE may carry. Supported_Max_Codec_Frames_Per_SDU (default 1
 * when absent) counts codec frames per channel in one SDU, i.e. it limits
 * Codec_Frame_Blocks_Per_SDU (always 1 here), not the channel count. */
bool device_supports(const LeAudioDevice &d, const StreamPreset &p, int channels_per_ase) {
    uint16_t sf_bit = sf_hz_to_cap_bit(p.sampling_hz);
    uint8_t fd_bit = p.frame_us == 7500 ? FD_BIT_7500US : FD_BIT_10000US;
    for (const auto &r : d.sink_pacs) {
        if (!r.lc3) continue;
        if (!(r.caps.sampling_frequencies & sf_bit)) continue;
        if (!(r.caps.frame_durations & fd_bit)) continue;
        if (r.caps.max_octets && (p.octets < r.caps.min_octets || p.octets > r.caps.max_octets)) continue;
        if (!(r.caps.channel_counts & (1u << (channels_per_ase - 1)))) continue;
        return true;
    }
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

uint16_t choose_streaming_contexts(const std::vector<const LeAudioDevice *> &devices, uint16_t requested) {
    /* BAP 1.0.1 5.6.3 / PACS: Enable may only name contexts that are
     * available on the server; Unspecified when the requested ones are not
     * (if Unspecified is available itself) */
    uint16_t common = requested, unspecified = CONTEXT_UNSPECIFIED;
    for (const LeAudioDevice *d : devices) {
        common &= d->available_sink_contexts;
        unspecified &= d->available_sink_contexts;
    }
    return common ? common : unspecified;
}

bool plan_stream(const std::vector<const LeAudioDevice *> &devices, const StreamRequest &request,
                 StreamPlan &plan, std::string &why) {
    plan = StreamPlan();
    if (devices.empty()) { why = "no device"; return false; }

    /* The controller's ISO buffers: every SDU must fit one HCI ISO data
     * packet (BTstack would fragment a longer one, and its per-CIS flow
     * control counts SDUs, not packets), and every CIS needs at least one */
    const Core &core = Core::instance();
    const uint16_t iso_len = core.iso_packet_length();
    const int iso_count = core.iso_packet_count();
    if (iso_len == 0 || iso_count == 0) {
        why = "the Bluetooth adapter reports no ISO data buffers (no LE Audio / CIS support)";
        return false;
    }
    const int max_cis = std::min(4, iso_count);    /* MAX_NR_CIS (btstack_config.h) */
    auto fits = [iso_len](uint32_t sdu_len) { return sdu_len + 4u <= iso_len; };

    std::map<int, std::vector<uint8_t>> sink_ases;
    for (const LeAudioDevice *d : devices) {
        std::vector<uint8_t> &ids = sink_ases[d->id];
        for (const auto &a : d->ases)
            if (a.sink) ids.push_back(a.id);
        std::sort(ids.begin(), ids.end());
        if (ids.empty()) {
            why = d->name.empty() ? d->addr.str() : d->name;
            why += ": no sink ASE (not an LE Audio sink?)";
            return false;
        }
    }

    /* ASEs and channels of every device for preset p, by audio location;
     * false if a device cannot take p */
    auto layout = [&](const StreamPreset &p, std::vector<CisPlan> &cis, bool &too_long) -> bool {
        cis.clear();
        for (const LeAudioDevice *d : devices) {
            const std::vector<uint8_t> &ids = sink_ases[d->id];
            auto add = [&](uint8_t ase_id, std::vector<uint32_t> locs) {
                CisPlan c;
                c.device_id = d->id;
                c.ase_id = ase_id;
                c.acl = d->acl;
                for (uint32_t l : locs) c.channels.push_back({l, ChannelSource::Mono});
                cis.push_back(c);
            };
            bool one = device_supports(*d, p, 1);
            bool one_fits = fits(p.octets);
            too_long |= one && !one_fits;
            uint32_t lr = d->sink_locations & (LOCATION_FRONT_LEFT | LOCATION_FRONT_RIGHT);
            if (popcount32(lr) == 2) {
                bool two_ok = device_supports(*d, p, 2);
                bool two = two_ok && fits(2u * p.octets);
                bool split = one && one_fits && ids.size() >= 2;
                too_long |= two_ok && !two;
                if (two && (request.prefer_two_channel_ase || !split))
                    add(ids[0], {LOCATION_FRONT_LEFT, LOCATION_FRONT_RIGHT});
                else if (split) {
                    add(ids[0], {LOCATION_FRONT_LEFT});
                    add(ids[1], {LOCATION_FRONT_RIGHT});
                } else if (one && one_fits) {
                    /* A stereo device that takes one channel only: mono (no
                     * Audio_Channel_Allocation), so that it renders the
                     * downmix rather than playing it on the left only */
                    add(ids[0], {LOCATION_MONO});
                } else {
                    return false;
                }
            } else if (one && one_fits) {
                add(ids[0], {popcount32(lr) == 1 ? lr : LOCATION_MONO});
            } else {
                return false;
            }
        }
        return true;
    };

    /* First preset in the requested order every device supports */
    const StreamPreset *chosen = nullptr;
    std::vector<CisPlan> cis;
    bool too_long = false, too_many = false;
    for (const auto &name : request.preset_order) {
        const StreamPreset *p = find_stream_preset(name);
        if (!p) continue;
        if (!layout(*p, cis, too_long)) continue;
        if ((int)cis.size() > max_cis) { too_many = true; continue; }
        chosen = p;
        break;
    }
    if (!chosen) {
        char buf[160];
        if (too_many)
            snprintf(buf, sizeof(buf), "the group needs more CIS than the adapter can take (%d)", max_cis);
        else if (too_long)
            snprintf(buf, sizeof(buf), "no LC3 configuration of the devices fits the adapter's ISO packets "
                     "(%u bytes)", iso_len);
        else
            snprintf(buf, sizeof(buf), "no LC3 configuration supported by all devices (sink PAC records)");
        why = buf;
        return false;
    }

    /* Channel sources: left / right only when the group has both */
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

    plan.preset = *chosen;
    plan.cis = std::move(cis);
    plan.phy = request.target_phy;
    plan.presentation_delay_us = chosen->presentation_delay_us;
    /* Streaming context: checked again right before Enable (Unicast), as
     * the available contexts may change until then */
    plan.requested_contexts = request.contexts;
    plan.contexts = choose_streaming_contexts(devices, request.contexts);
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
