/*
 * LE Audio (experimental) - assigned numbers and small helpers
 *
 * UUIDs, LTV types and enums from the Bluetooth SIG Assigned Numbers, PACS,
 * ASCS and BAP 1.0.x, plus LTV (length-type-value) read/write helpers shared
 * by the core and the profile modules.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_DEFS_H
#define LE_AUDIO_DEFS_H

#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

namespace le_audio {

/* ---- GATT services (Assigned Numbers 3.4) ---- */
constexpr uint16_t UUID_SERVICE_GAP     = 0x1800;
constexpr uint16_t UUID_SERVICE_GATT    = 0x1801;
constexpr uint16_t UUID_SERVICE_AICS    = 0x1843;  /* Audio Input Control */
constexpr uint16_t UUID_SERVICE_VCS     = 0x1844;  /* Volume Control */
constexpr uint16_t UUID_SERVICE_VOCS    = 0x1845;  /* Volume Offset Control */
constexpr uint16_t UUID_SERVICE_CSIS    = 0x1846;  /* Coordinated Set Identification */
constexpr uint16_t UUID_SERVICE_MCS     = 0x1848;  /* Media Control */
constexpr uint16_t UUID_SERVICE_GMCS    = 0x1849;  /* Generic Media Control */
constexpr uint16_t UUID_SERVICE_TBS     = 0x184B;  /* Telephone Bearer */
constexpr uint16_t UUID_SERVICE_GTBS    = 0x184C;  /* Generic Telephone Bearer */
constexpr uint16_t UUID_SERVICE_ASCS    = 0x184E;  /* Audio Stream Control */
constexpr uint16_t UUID_SERVICE_BASS    = 0x184F;  /* Broadcast Audio Scan */
constexpr uint16_t UUID_SERVICE_PACS    = 0x1850;  /* Published Audio Capabilities */
constexpr uint16_t UUID_SERVICE_CAS     = 0x1853;  /* Common Audio */
constexpr uint16_t UUID_SERVICE_HAS     = 0x1854;  /* Hearing Access */
constexpr uint16_t UUID_SERVICE_TMAS    = 0x1855;  /* Telephony and Media Audio */
constexpr uint16_t UUID_SERVICE_GMAS    = 0x1858;  /* Gaming Audio */

/* ---- Characteristics ---- */
constexpr uint16_t UUID_CHR_DEVICE_NAME          = 0x2A00;
constexpr uint16_t UUID_CHR_APPEARANCE           = 0x2A01;
constexpr uint16_t UUID_CHR_SERVICE_CHANGED      = 0x2A05;
constexpr uint16_t UUID_CHR_SINK_ASE             = 0x2BC4;
constexpr uint16_t UUID_CHR_SOURCE_ASE           = 0x2BC5;
constexpr uint16_t UUID_CHR_ASE_CONTROL_POINT    = 0x2BC6;
constexpr uint16_t UUID_CHR_SINK_PAC             = 0x2BC9;
constexpr uint16_t UUID_CHR_SINK_AUDIO_LOCATIONS = 0x2BCA;
constexpr uint16_t UUID_CHR_SOURCE_PAC           = 0x2BCB;
constexpr uint16_t UUID_CHR_SOURCE_AUDIO_LOCATIONS = 0x2BCC;
constexpr uint16_t UUID_CHR_AVAILABLE_AUDIO_CONTEXTS = 0x2BCD;
constexpr uint16_t UUID_CHR_SUPPORTED_AUDIO_CONTEXTS = 0x2BCE;

constexpr uint16_t UUID_DESC_CCC = 0x2902;

/* ---- Advertising data types used by the scan filter ---- */
constexpr uint8_t AD_FLAGS                = 0x01;
constexpr uint8_t AD_UUID16_INCOMPLETE    = 0x02;
constexpr uint8_t AD_UUID16_COMPLETE      = 0x03;
constexpr uint8_t AD_NAME_SHORT           = 0x08;
constexpr uint8_t AD_NAME_COMPLETE        = 0x09;
constexpr uint8_t AD_SERVICE_DATA_UUID16  = 0x16;
constexpr uint8_t AD_APPEARANCE           = 0x19;
constexpr uint8_t AD_RSI                  = 0x2E;  /* Resolvable Set Identifier (CSIP) */

/* ---- Audio locations (Assigned Numbers 6.12.1) ---- */
constexpr uint32_t LOCATION_MONO        = 0x00000000;
constexpr uint32_t LOCATION_FRONT_LEFT  = 0x00000001;
constexpr uint32_t LOCATION_FRONT_RIGHT = 0x00000002;

/* ---- Context types (Assigned Numbers 6.12.3) ---- */
constexpr uint16_t CONTEXT_UNSPECIFIED    = 0x0001;
constexpr uint16_t CONTEXT_CONVERSATIONAL = 0x0002;
constexpr uint16_t CONTEXT_MEDIA          = 0x0004;
constexpr uint16_t CONTEXT_GAME           = 0x0008;

/* ---- Coding formats ---- */
constexpr uint8_t CODING_FORMAT_LC3 = 0x06;

/* ---- Codec Specific Capabilities LTV types (PAC records) ---- */
constexpr uint8_t CAP_SAMPLING_FREQUENCIES = 0x01;  /* uint16 bitfield */
constexpr uint8_t CAP_FRAME_DURATIONS      = 0x02;  /* uint8 bitfield */
constexpr uint8_t CAP_CHANNEL_COUNTS       = 0x03;  /* uint8 bitfield, bit n = n+1 channels */
constexpr uint8_t CAP_OCTETS_PER_FRAME     = 0x04;  /* uint16 min, uint16 max */
constexpr uint8_t CAP_MAX_FRAMES_PER_SDU   = 0x05;  /* uint8 */

/* Supported Sampling Frequencies bits */
constexpr uint16_t SF_BIT_8000  = 1u << 0;
constexpr uint16_t SF_BIT_16000 = 1u << 2;
constexpr uint16_t SF_BIT_24000 = 1u << 4;
constexpr uint16_t SF_BIT_32000 = 1u << 5;
constexpr uint16_t SF_BIT_44100 = 1u << 6;
constexpr uint16_t SF_BIT_48000 = 1u << 7;
/* Supported Frame Durations bits */
constexpr uint8_t FD_BIT_7500US  = 1u << 0;
constexpr uint8_t FD_BIT_10000US = 1u << 1;

/* ---- Codec Specific Configuration LTV types (ASE Config Codec) ---- */
constexpr uint8_t CFG_SAMPLING_FREQUENCY = 0x01;  /* uint8 code */
constexpr uint8_t CFG_FRAME_DURATION     = 0x02;  /* 0x00 7.5 ms, 0x01 10 ms */
constexpr uint8_t CFG_CHANNEL_ALLOCATION = 0x03;  /* uint32 audio locations */
constexpr uint8_t CFG_OCTETS_PER_FRAME   = 0x04;  /* uint16 */
constexpr uint8_t CFG_FRAME_BLOCKS_PER_SDU = 0x05; /* uint8 */

/* Sampling Frequency configuration codes */
constexpr uint8_t SF_CODE_8000  = 0x01;
constexpr uint8_t SF_CODE_16000 = 0x03;
constexpr uint8_t SF_CODE_24000 = 0x05;
constexpr uint8_t SF_CODE_32000 = 0x06;
constexpr uint8_t SF_CODE_44100 = 0x07;
constexpr uint8_t SF_CODE_48000 = 0x08;

/* ---- Metadata LTV types ---- */
constexpr uint8_t META_PREFERRED_CONTEXTS = 0x01;
constexpr uint8_t META_STREAMING_CONTEXTS = 0x02;

/* ---- ASCS ---- */
enum class AseState : uint8_t {
    Idle = 0x00,
    CodecConfigured = 0x01,
    QosConfigured = 0x02,
    Enabling = 0x03,
    Streaming = 0x04,
    Disabling = 0x05,
    Releasing = 0x06,
    Unknown = 0xFF,
};
const char *ase_state_name(AseState s);

enum AseOpcode : uint8_t {
    ASE_OP_CONFIG_CODEC = 0x01,
    ASE_OP_CONFIG_QOS = 0x02,
    ASE_OP_ENABLE = 0x03,
    ASE_OP_RECEIVER_START_READY = 0x04,
    ASE_OP_DISABLE = 0x05,
    ASE_OP_RECEIVER_STOP_READY = 0x06,
    ASE_OP_UPDATE_METADATA = 0x07,
    ASE_OP_RELEASE = 0x08,
};

/* ASCS Target_Latency (Config Codec) */
constexpr uint8_t TARGET_LATENCY_LOW = 0x01;
constexpr uint8_t TARGET_LATENCY_BALANCED = 0x02;
constexpr uint8_t TARGET_LATENCY_HIGH_RELIABILITY = 0x03;
/* PHY bits (Target_PHY uses 0x01 1M, 0x02 2M, 0x03 Coded; QoS uses a bitfield) */
constexpr uint8_t PHY_1M = 0x01;
constexpr uint8_t PHY_2M = 0x02;

/* ---- Little-endian helpers ---- */
inline uint16_t rd16(const uint8_t *p) { return (uint16_t)(p[0] | (p[1] << 8)); }
inline uint32_t rd24(const uint8_t *p) { return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16); }
inline uint32_t rd32(const uint8_t *p) { return rd24(p) | ((uint32_t)p[3] << 24); }
inline void put8(std::vector<uint8_t> &v, uint8_t x) { v.push_back(x); }
inline void put16(std::vector<uint8_t> &v, uint16_t x) { v.push_back((uint8_t)x); v.push_back((uint8_t)(x >> 8)); }
inline void put24(std::vector<uint8_t> &v, uint32_t x) { put16(v, (uint16_t)x); v.push_back((uint8_t)(x >> 16)); }
inline void put32(std::vector<uint8_t> &v, uint32_t x) { put16(v, (uint16_t)x); put16(v, (uint16_t)(x >> 16)); }

/* Find an LTV of the given type in data (LTVs: length counts type + value).
 * Returns the value and its length, or nullptr. */
const uint8_t *ltv_find(const uint8_t *data, uint16_t len, uint8_t type, uint8_t *value_len);
/* Append an LTV */
void ltv_put(std::vector<uint8_t> &out, uint8_t type, const uint8_t *value, uint8_t value_len);

/* Number of set bits (channels of an allocation) */
inline int popcount32(uint32_t x) { int n = 0; while (x) { n += (int)(x & 1u); x >>= 1; } return n; }

/* "XX:XX:XX:XX:XX:XX" of a BTstack-order address */
std::string addr_to_string(const uint8_t addr[6]);
/* Parses "XX:XX:XX:XX:XX:XX" into BTstack order; false if malformed */
bool addr_from_string(const std::string &s, uint8_t addr[6]);

/* LC3 sampling frequency code <-> Hz; 0 when unknown */
uint32_t sf_code_to_hz(uint8_t code);
uint8_t sf_hz_to_code(uint32_t hz);
uint16_t sf_hz_to_cap_bit(uint32_t hz);

} /* namespace le_audio */

#endif /* LE_AUDIO_DEFS_H */
