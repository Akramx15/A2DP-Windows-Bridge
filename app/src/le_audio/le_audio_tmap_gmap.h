/*
 * LE Audio (experimental) - TMAP and GMAP
 *
 * TMAP (Telephony and Media Audio Profile 1.0) is a role profile on top of
 * BAP / CAP:
 *  - the PC's TMAS says A2DPWB is a Unicast Media Sender
 *    (le_audio_tmap_gmap.cpp explains why it claims nothing else);
 *  - the headset's TMAP Role is read once per connection and logged
 *    ("TMAP: UMR+CT").
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

/* TMAP Role bits (TMAP 1.0, TMAP Role characteristic) */
constexpr uint16_t TMAP_ROLE_CG  = 1u << 0;   /* Call Gateway */
constexpr uint16_t TMAP_ROLE_CT  = 1u << 1;   /* Call Terminal */
constexpr uint16_t TMAP_ROLE_UMS = 1u << 2;   /* Unicast Media Sender */
constexpr uint16_t TMAP_ROLE_UMR = 1u << 3;   /* Unicast Media Receiver */
constexpr uint16_t TMAP_ROLE_BMS = 1u << 4;   /* Broadcast Media Sender */
constexpr uint16_t TMAP_ROLE_BMR = 1u << 5;   /* Broadcast Media Receiver */

/* What the PC's TMAS says */
constexpr uint16_t PC_TMAP_ROLE = TMAP_ROLE_UMS;

/* The roles a connected device has (read in on_gatt_ready) */
struct TmapGmapInfo {
    bool done = false;             /* all reads finished */
    bool has_tmas = false;
    uint16_t tmap_role = 0;        /* valid when has_tmas */
};
/* Any thread. False while the device is unknown to the module (not
 * connected / not read yet). */
bool tmap_gmap_info(int device_id, TmapGmapInfo &out);
/* "TMAP: UMR" for the UI / log ("" if unknown) */
std::string tmap_gmap_summary(int device_id);
std::string describe_tmap_role(uint16_t role);

} /* namespace le_audio */

#endif /* LE_AUDIO_TMAP_GMAP_H */
