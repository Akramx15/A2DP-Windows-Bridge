/*
 * LE Audio (experimental) - connected device model
 *
 * One LeAudioDevice per LE Audio peer (e.g. a headset, or one earbud of a
 * pair). Owned by le_audio::Core and only touched on the BTstack thread.
 * Profile modules read it (GATT cache, PACS, ASCS) and keep their own
 * per-device state keyed by LeAudioDevice::id.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_DEVICE_H
#define LE_AUDIO_DEVICE_H

#include "le_audio_defs.h"

#include <cstdint>
#include <string>
#include <vector>

namespace le_audio {

constexpr uint16_t CON_HANDLE_INVALID = 0xFFFF;

/* LE address in BTstack order (as printed) plus its type (BD_ADDR_TYPE_LE_*:
 * 0 public, 1 random) */
struct LeAddr {
    uint8_t addr[6] = {};
    uint8_t type = 0;
    bool operator==(const LeAddr &o) const { return type == o.type && memcmp(addr, o.addr, 6) == 0; }
    std::string str() const { return addr_to_string(addr); }
};

/* ---- GATT cache (filled once per connection by the core) ---- */

struct GattChar {
    uint16_t uuid16 = 0;          /* 0 for a 128-bit UUID */
    uint8_t uuid128[16] = {};     /* BTstack order */
    uint16_t start_handle = 0;    /* declaration */
    uint16_t value_handle = 0;
    uint16_t end_handle = 0;      /* last descriptor */
    uint16_t properties = 0;      /* ATT_PROPERTY_* */
};

struct GattService {
    uint16_t uuid16 = 0;          /* 0 for a 128-bit UUID */
    uint8_t uuid128[16] = {};
    uint16_t start_handle = 0;
    uint16_t end_handle = 0;
    bool primary = true;          /* false: secondary, found as an included service */
    uint16_t included_by = 0;     /* start handle of the including service (secondary) */
    std::vector<GattChar> chars;
};

/* ---- PACS ---- */

struct Lc3Capabilities {
    uint16_t sampling_frequencies = 0;   /* SF_BIT_* */
    uint8_t frame_durations = 0;         /* FD_BIT_* */
    uint8_t channel_counts = 0x01;       /* bit n = n+1 channels; default 1 channel */
    uint16_t min_octets = 0;
    uint16_t max_octets = 0;
    uint8_t max_frames_per_sdu = 1;      /* default 1 */
};

struct PacRecord {
    uint8_t codec_id[5] = {};            /* coding format, company, vendor codec */
    bool lc3 = false;
    Lc3Capabilities caps;                /* valid when lc3 */
    std::vector<uint8_t> capabilities;   /* raw LTVs */
    std::vector<uint8_t> metadata;       /* raw LTVs */
    uint16_t preferred_contexts = 0;     /* metadata 0x01, 0 if absent */
};

/* Parses a PAC characteristic value (Number_of_PAC_records + records) */
bool parse_pac_records(const uint8_t *data, uint16_t len, std::vector<PacRecord> &out);

/* ---- ASCS ---- */

struct Ase {
    uint8_t id = 0;
    bool sink = true;                    /* Sink ASE (we send audio to it) */
    uint16_t value_handle = 0;
    AseState state = AseState::Unknown;
    /* Codec Configured: the server's preferences */
    uint8_t pref_framing = 0;
    uint8_t pref_phy = 0;
    uint8_t pref_rtn = 0;
    uint16_t pref_max_latency_ms = 0;
    uint32_t pd_min_us = 0, pd_max_us = 0;
    uint32_t pref_pd_min_us = 0, pref_pd_max_us = 0;
    /* QoS Configured and later */
    uint8_t cig_id = 0, cis_id = 0;
    std::vector<uint8_t> codec_config;   /* LTVs as configured */
};

/* Parses an ASE characteristic value into ase (id, state, parameters) */
bool parse_ase_value(const uint8_t *data, uint16_t len, Ase &ase);

/* ---- The device ---- */

struct LeAudioDevice {
    int id = -1;                         /* stable while the core runs */
    LeAddr addr;
    std::string name;

    /* Link */
    uint16_t acl = CON_HANDLE_INVALID;   /* hci_con_handle_t */
    bool encrypted = false;
    bool pairing = false;                /* SM pairing in progress */
    bool bonded_before = false;          /* an LTK was stored when we connected */
    int pair_attempts = 0;
    bool secured = false;                /* core: security done, discovery started */
    bool gatt_ready = false;             /* cache + PACS + ASCS read */
    bool ready = false;                  /* connect() may return */
    uint8_t last_error = 0;              /* why it went down / failed (HCI / ATT / SM) */

    /* GATT cache */
    std::vector<GattService> services;
    const GattService *find_service(uint16_t uuid16) const;
    /* All characteristics with this UUID in all services with service_uuid16
     * (0 = any service) */
    std::vector<const GattChar *> find_chars(uint16_t service_uuid16, uint16_t char_uuid16) const;
    const GattChar *find_char(uint16_t service_uuid16, uint16_t char_uuid16) const;

    /* PACS (sink direction: we are the audio source) */
    std::vector<PacRecord> sink_pacs;
    uint32_t sink_locations = 0;
    uint16_t available_sink_contexts = 0, available_source_contexts = 0;
    uint16_t supported_sink_contexts = 0, supported_source_contexts = 0;
    bool has_pacs = false, has_ascs = false;

    /* ASCS */
    std::vector<Ase> ases;
    uint16_t ase_cp_handle = 0;
    Ase *ase(uint8_t id);
    int sink_ase_count() const;

    bool connected() const { return acl != CON_HANDLE_INVALID; }
};

} /* namespace le_audio */

#endif /* LE_AUDIO_DEVICE_H */
