/*
 * LE Audio (experimental) - profile module registry
 *
 * Every profile module is listed here once; adding a module means adding
 * its factory below. The five profile modules are pre-registered, so their
 * owners only edit their own le_audio_<name>.{h,cpp}.
 *
 * Order matters where hooks compete: prepare_group() runs CSIP first (it adds
 * set members), adjust_stream_request() then sees the whole group, and the
 * first module that answers remote_volume() wins.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_module.h"

namespace le_audio {

const std::vector<Module *> &modules() {
    static const std::vector<Module *> list = {
        csip_module(),        /* le_audio_csip.cpp */
        vcp_module(),         /* le_audio_vcp.cpp */
        mcp_module(),         /* le_audio_mcp.cpp */
        ccp_module(),         /* le_audio_ccp.cpp */
        tmap_gmap_module(),   /* le_audio_tmap_gmap.cpp */
    };
    return list;
}

} /* namespace le_audio */
