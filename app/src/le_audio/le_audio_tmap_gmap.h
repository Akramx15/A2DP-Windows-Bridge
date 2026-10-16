/*
 * LE Audio (experimental) - TMAP and GMAP
 *
 * TMAP (Telephony and Media Audio Profile 1.0) and GMAP (Gaming Audio
 * Profile 1.0) are role profiles on top of BAP / CAP:
 *  - the PC's TMAS says A2DPWB is a Unicast Media Sender, its GMAS that it
 *    is a Unicast Game Gateway (le_audio_tmap_gmap.cpp explains the roles
 *    and features claimed);
 *  - the headset's TMAP Role, GMAP Role and UGT Features are read once per
 *    connection and logged ("TMAP: UMR, GMAP: UGT (sink, multiplex)");
 *  - with "Low latency (GMAP)" on and every device of the group a Unicast
 *    Game Terminal with the Sink feature, the stream uses GMAP's low-latency
 *    presets ("48_3_gr" ..., le_audio_presets.cpp) with the Game context.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_TMAP_GMAP_H
#define LE_AUDIO_TMAP_GMAP_H

#include "le_audio_module.h"

#include <cstdint>
#include <string>

namespace le_audio {

/* Registered in le_audio_modules.cpp */
Module *tmap_gmap_module();

/* ---- Characteristics (Assigned Numbers 3.8) ---- */
constexpr uint16_t UUID_CHR_TMAP_ROLE    = 0x2B51;  /* TMAS, uint16 */
constexpr uint16_t UUID_CHR_GMAP_ROLE    = 0x2C00;  /* GMAS, uint8 */
constexpr uint16_t UUID_CHR_UGG_FEATURES = 0x2C01;  /* uint8 */
constexpr uint16_t UUID_CHR_UGT_FEATURES = 0x2C02;  /* uint8 */

/* TMAP Role bits (TMAP 1.0, TMAP Role characteristic) */
constexpr uint16_t TMAP_ROLE_CG  = 1u << 0;   /* Call Gateway */
constexpr uint16_t TMAP_ROLE_CT  = 1u << 1;   /* Call Terminal */
constexpr uint16_t TMAP_ROLE_UMS = 1u << 2;   /* Unicast Media Sender */
constexpr uint16_t TMAP_ROLE_UMR = 1u << 3;   /* Unicast Media Receiver */
constexpr uint16_t TMAP_ROLE_BMS = 1u << 4;   /* Broadcast Media Sender */
constexpr uint16_t TMAP_ROLE_BMR = 1u << 5;   /* Broadcast Media Receiver */

/* GMAP Role bits (GMAP 1.0, GMAP Role characteristic) */
constexpr uint8_t GMAP_ROLE_UGG = 1u << 0;    /* Unicast Game Gateway */
constexpr uint8_t GMAP_ROLE_UGT = 1u << 1;    /* Unicast Game Terminal */
constexpr uint8_t GMAP_ROLE_BGS = 1u << 2;    /* Broadcast Game Sender */
constexpr uint8_t GMAP_ROLE_BGR = 1u << 3;    /* Broadcast Game Receiver */

/* UGG Features bits */
constexpr uint8_t UGG_FEATURE_MULTIPLEX   = 1u << 0;  /* several LC3 frames (channels) per SDU block */
constexpr uint8_t UGG_FEATURE_96KBPS_SRC  = 1u << 1;  /* sends 96 kbps (48_3 / 48_4) */
constexpr uint8_t UGG_FEATURE_MULTISINK   = 1u << 2;  /* receives >= 2 channels in separate CIS */

/* UGT Features bits */
constexpr uint8_t UGT_FEATURE_SOURCE      = 1u << 0;
constexpr uint8_t UGT_FEATURE_80KBPS_SRC  = 1u << 1;
constexpr uint8_t UGT_FEATURE_SINK        = 1u << 2;
constexpr uint8_t UGT_FEATURE_64KBPS_SINK = 1u << 3;
constexpr uint8_t UGT_FEATURE_MULTIPLEX   = 1u << 4;
constexpr uint8_t UGT_FEATURE_MULTISINK   = 1u << 5;
constexpr uint8_t UGT_FEATURE_MULTISOURCE = 1u << 6;

/* What the PC's TMAS / GMAS say */
constexpr uint16_t PC_TMAP_ROLE   = TMAP_ROLE_UMS;
constexpr uint8_t PC_GMAP_ROLE    = GMAP_ROLE_UGG;
constexpr uint8_t PC_UGG_FEATURES = UGG_FEATURE_MULTIPLEX | UGG_FEATURE_96KBPS_SRC;

/* "Low latency (GMAP)" (settings / --le-low-latency). Any thread; read when
 * a stream is set up. Off by default. */
void set_gmap_low_latency(bool on);
bool gmap_low_latency();

/* The roles a connected device has (read in on_gatt_ready) */
struct TmapGmapInfo {
    bool done = false;             /* all reads finished */
    bool has_tmas = false;
    uint16_t tmap_role = 0;        /* valid when has_tmas */
    bool has_gmas = false;
    uint8_t gmap_role = 0;         /* valid when has_gmas */
    uint8_t ugt_features = 0;      /* valid with GMAP_ROLE_UGT */
    /* A Unicast Game Terminal that renders audio (GMAP low latency applies) */
    bool ugt_sink() const {
        return has_gmas && (gmap_role & GMAP_ROLE_UGT) && (ugt_features & UGT_FEATURE_SINK);
    }
};
/* Any thread. False while the device is unknown to the module (not
 * connected / not read yet). */
bool tmap_gmap_info(int device_id, TmapGmapInfo &out);
/* "TMAP: UMR, GMAP: UGT (sink, multiplex)" for the UI / log ("" if unknown) */
std::string tmap_gmap_summary(int device_id);
std::string describe_tmap_role(uint16_t role);
std::string describe_gmap_role(uint8_t role);
std::string describe_ugt_features(uint8_t features);

} /* namespace le_audio */

#endif /* LE_AUDIO_TMAP_GMAP_H */
