/*
 * LE Audio (experimental) - CSIP Set Coordinator (stub, does nothing yet)
 *
 * Responsibility: earbuds that form a Coordinated Set (two devices, one per
 * ear) stream together. The coordinator reads the CSIS of a connected member
 * (SIRK, set size, rank; the lock is not needed for unicast audio), finds the
 * other members by their Resolvable Set Identifier (RSI, AD type 0x2E) and
 * connects them, so the core streams to the whole set (one CIS per earbud,
 * channels by audio location - plan_stream() already does that for a list of
 * devices).
 *
 * Hooks to use (le_audio_module.h):
 *  - on_gatt_ready(dev): CSIS (UUID_SERVICE_CSIS 0x1846) in dev.services;
 *    read SIRK (0x2B84, may be encrypted with the LTK: CSIP 4.5), Set Size
 *    (0x2B85) and Rank (0x2B86) with le_audio::gatt_client.
 *  - on_scan_result(result, ad, len): result.rsi holds the RSI; resolve it
 *    with a known SIRK (sih, CSIP 4.7) and set result.set_id / result.note /
 *    result.le_audio for members of a known set.
 *  - prepare_group(core, ids, cancelled) (worker thread): for each set in ids
 *    with fewer connected members than Set Size, core.scan() for resolving
 *    RSIs, core.connect() the members found and append their ids.
 *  - on_disconnected(dev, reason): drop per-device state.
 * Bumble side: tools/emu/le_audio/csip.py (CSIS on each emulated earbud, RSI
 * in its advertising data).
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_csip.h"

namespace le_audio {

namespace {

class CsipModule : public Module {
public:
    const char *name() const override { return "CSIP"; }
};

} /* namespace */

Module *csip_module() {
    static CsipModule module;
    return &module;
}

} /* namespace le_audio */
