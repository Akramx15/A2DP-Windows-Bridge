/*
 * LE Audio (experimental) - helpers: LTVs, addresses, PAC / ASE parsing
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_defs.h"
#include "le_audio_device.h"

#include <cstdio>

namespace le_audio {

const char *ase_state_name(AseState s) {
    switch (s) {
    case AseState::Idle:            return "Idle";
    case AseState::CodecConfigured: return "Codec Configured";
    case AseState::QosConfigured:   return "QoS Configured";
    case AseState::Enabling:        return "Enabling";
    case AseState::Streaming:       return "Streaming";
    case AseState::Disabling:       return "Disabling";
    case AseState::Releasing:       return "Releasing";
    default:                        return "unknown";
    }
}

const uint8_t *ltv_find(const uint8_t *data, uint16_t len, uint8_t type, uint8_t *value_len) {
    uint16_t pos = 0;
    while (pos < len) {
        uint8_t l = data[pos];
        if (l == 0) { pos++; continue; }
        if (pos + 1u + l > len) return nullptr;
        if (data[pos + 1] == type) {
            if (value_len) *value_len = (uint8_t)(l - 1);
            return data + pos + 2;
        }
        pos = (uint16_t)(pos + 1u + l);
    }
    return nullptr;
}

void ltv_put(std::vector<uint8_t> &out, uint8_t type, const uint8_t *value, uint8_t value_len) {
    out.push_back((uint8_t)(value_len + 1));
    out.push_back(type);
    out.insert(out.end(), value, value + value_len);
}

std::string addr_to_string(const uint8_t addr[6]) {
    char buf[18];
    snprintf(buf, sizeof(buf), "%02X:%02X:%02X:%02X:%02X:%02X",
             addr[0], addr[1], addr[2], addr[3], addr[4], addr[5]);
    return buf;
}

bool addr_from_string(const std::string &s, uint8_t addr[6]) {
    unsigned a[6];
    if (s.size() < 17 || sscanf_s(s.c_str(), "%02x:%02x:%02x:%02x:%02x:%02x",
                                &a[0], &a[1], &a[2], &a[3], &a[4], &a[5]) != 6)
        return false;
    for (int i = 0; i < 6; i++) addr[i] = (uint8_t)a[i];
    return true;
}

uint32_t sf_code_to_hz(uint8_t code) {
    switch (code) {
    case SF_CODE_8000:  return 8000;
    case 0x02:          return 11025;
    case SF_CODE_16000: return 16000;
    case 0x04:          return 22050;
    case SF_CODE_24000: return 24000;
    case SF_CODE_32000: return 32000;
    case SF_CODE_44100: return 44100;
    case SF_CODE_48000: return 48000;
    default:            return 0;
    }
}

uint8_t sf_hz_to_code(uint32_t hz) {
    switch (hz) {
    case 8000:  return SF_CODE_8000;
    case 16000: return SF_CODE_16000;
    case 24000: return SF_CODE_24000;
    case 32000: return SF_CODE_32000;
    case 44100: return SF_CODE_44100;
    case 48000: return SF_CODE_48000;
    default:    return 0;
    }
}

uint16_t sf_hz_to_cap_bit(uint32_t hz) {
    switch (hz) {
    case 8000:  return SF_BIT_8000;
    case 16000: return SF_BIT_16000;
    case 24000: return SF_BIT_24000;
    case 32000: return SF_BIT_32000;
    case 44100: return SF_BIT_44100;
    case 48000: return SF_BIT_48000;
    default:    return 0;
    }
}

/* ---- PAC records (PACS 3.1 / 3.4) ---- */

bool parse_pac_records(const uint8_t *data, uint16_t len, std::vector<PacRecord> &out) {
    if (len < 1) return false;
    uint8_t count = data[0];
    uint16_t pos = 1;
    for (uint8_t i = 0; i < count; i++) {
        if (pos + 6u > len) return false;
        PacRecord r;
        memcpy(r.codec_id, data + pos, 5);
        pos += 5;
        uint8_t cap_len = data[pos++];
        if (pos + cap_len + 1u > len) return false;
        r.capabilities.assign(data + pos, data + pos + cap_len);
        pos = (uint16_t)(pos + cap_len);
        uint8_t meta_len = data[pos++];
        if (pos + meta_len > len) return false;
        r.metadata.assign(data + pos, data + pos + meta_len);
        pos = (uint16_t)(pos + meta_len);

        r.lc3 = r.codec_id[0] == CODING_FORMAT_LC3;
        if (r.lc3) {
            const uint8_t *caps = r.capabilities.data();
            uint16_t clen = (uint16_t)r.capabilities.size();
            uint8_t vl = 0;
            const uint8_t *v;
            if ((v = ltv_find(caps, clen, CAP_SAMPLING_FREQUENCIES, &vl)) && vl >= 2)
                r.caps.sampling_frequencies = rd16(v);
            if ((v = ltv_find(caps, clen, CAP_FRAME_DURATIONS, &vl)) && vl >= 1)
                r.caps.frame_durations = v[0];
            if ((v = ltv_find(caps, clen, CAP_CHANNEL_COUNTS, &vl)) && vl >= 1)
                r.caps.channel_counts = v[0];
            if ((v = ltv_find(caps, clen, CAP_OCTETS_PER_FRAME, &vl)) && vl >= 4) {
                r.caps.min_octets = rd16(v);
                r.caps.max_octets = rd16(v + 2);
            }
            if ((v = ltv_find(caps, clen, CAP_MAX_FRAMES_PER_SDU, &vl)) && vl >= 1)
                r.caps.max_frames_per_sdu = v[0];
        }
        uint8_t vl = 0;
        const uint8_t *ctx = ltv_find(r.metadata.data(), (uint16_t)r.metadata.size(),
                                      META_PREFERRED_CONTEXTS, &vl);
        if (ctx && vl >= 2) r.preferred_contexts = rd16(ctx);
        out.push_back(std::move(r));
    }
    return true;
}

/* ---- ASE characteristic value (ASCS 4.1) ---- */

bool parse_ase_value(const uint8_t *data, uint16_t len, Ase &ase) {
    if (len < 2) return false;
    ase.id = data[0];
    ase.state = static_cast<AseState>(data[1]);
    const uint8_t *p = data + 2;
    uint16_t n = (uint16_t)(len - 2);
    switch (ase.state) {
    case AseState::CodecConfigured:
        if (n < 23) return false;
        ase.pref_framing = p[0];
        ase.pref_phy = p[1];
        ase.pref_rtn = p[2];
        ase.pref_max_latency_ms = rd16(p + 3);
        ase.pd_min_us = rd24(p + 5);
        ase.pd_max_us = rd24(p + 8);
        ase.pref_pd_min_us = rd24(p + 11);
        ase.pref_pd_max_us = rd24(p + 14);
        /* codec id p[17..21], config length p[22] */
        if (n < 23u + p[22]) return false;
        ase.codec_config.assign(p + 23, p + 23 + p[22]);
        break;
    case AseState::QosConfigured:
        if (n < 15) return false;
        ase.cig_id = p[0];
        ase.cis_id = p[1];
        break;
    case AseState::Enabling:
    case AseState::Streaming:
    case AseState::Disabling:
        if (n < 3) return false;
        ase.cig_id = p[0];
        ase.cis_id = p[1];
        break;
    case AseState::Idle:
    case AseState::Releasing:
        break;
    default:
        return false;
    }
    return true;
}

/* ---- LeAudioDevice ---- */

const GattService *LeAudioDevice::find_service(uint16_t uuid16) const {
    for (const auto &s : services)
        if (s.uuid16 == uuid16) return &s;
    return nullptr;
}

std::vector<const GattChar *> LeAudioDevice::find_chars(uint16_t service_uuid16, uint16_t char_uuid16) const {
    std::vector<const GattChar *> out;
    for (const auto &s : services) {
        if (service_uuid16 != 0 && s.uuid16 != service_uuid16) continue;
        for (const auto &c : s.chars)
            if (c.uuid16 == char_uuid16) out.push_back(&c);
    }
    return out;
}

const GattChar *LeAudioDevice::find_char(uint16_t service_uuid16, uint16_t char_uuid16) const {
    auto v = find_chars(service_uuid16, char_uuid16);
    return v.empty() ? nullptr : v[0];
}

Ase *LeAudioDevice::ase(uint8_t ase_id) {
    for (auto &a : ases)
        if (a.id == ase_id) return &a;
    return nullptr;
}

int LeAudioDevice::sink_ase_count() const {
    int n = 0;
    for (const auto &a : ases) n += a.sink ? 1 : 0;
    return n;
}

} /* namespace le_audio */
