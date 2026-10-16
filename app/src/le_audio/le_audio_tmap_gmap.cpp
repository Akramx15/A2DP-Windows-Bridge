/*
 * LE Audio (experimental) - TMAP and GMAP roles (stub, does nothing yet)
 *
 * Responsibility:
 *  - TMAP: the PC's TMAS (UUID_SERVICE_TMAS 0x1855) with TMAP Role (0x2B51),
 *    e.g. Unicast Media Sender (bit 2) | Call Gateway (bit 0) as far as
 *    supported; read the headset's TMAP Role (Unicast Media Receiver ...) to
 *    log and adapt.
 *  - GMAP: the PC's GMAS (UUID_SERVICE_GMAS 0x1858) with GMAP Role (0x2C00) =
 *    Unicast Game Gateway and UGG Features (0x2C01); read the headset's GMAP
 *    Role and UGT Features (0x2C02). For a Unicast Game Terminal prefer the
 *    GMAP low-latency QoS settings (GMAP 1.0 Table 3.16, e.g. "48_2_gr":
 *    10 ms, RTN 1, max transport latency 10 ms, presentation delay 10 ms,
 *    and the 7.5 ms ones).
 *
 * Hooks to use (le_audio_module.h, le_audio_presets.h):
 *  - add_gatt_services(server): TMAS and GMAS (read-only characteristics).
 *  - on_stack_init(core): add_stream_preset() for each GMAP preset, with
 *    target_latency = TARGET_LATENCY_LOW.
 *  - on_gatt_ready(dev): read the remote TMAP / GMAP roles with
 *    le_audio::gatt_client; remember them per device id.
 *  - adjust_stream_request(core, ids, request) (worker thread): when all
 *    devices are UGTs (and low latency is wanted), put the GMAP presets first
 *    in request.preset_order and set request.contexts to CONTEXT_GAME.
 * Bumble side: tools/emu/le_audio/tmap_gmap.py (bumble.profiles.tmap / gmap on
 * the sink).
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_tmap_gmap.h"

namespace le_audio {

namespace {

class TmapGmapModule : public Module {
public:
    const char *name() const override { return "TMAP/GMAP"; }
};

} /* namespace */

Module *tmap_gmap_module() {
    static TmapGmapModule module;
    return &module;
}

} /* namespace le_audio */
