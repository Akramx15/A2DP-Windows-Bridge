/*
 * LE Audio (experimental) - CCP call control server (stub, does nothing yet)
 *
 * Responsibility: the PC is the Call Control Server (GTBS). Headsets look for
 * a telephone bearer on the audio source; A2DPWB exposes GTBS so they find
 * one that reports "no call" (and later, if a VoIP call is ever bridged, its
 * state).
 *
 * Hooks to use (le_audio_module.h, le_audio_gatt_server.h):
 *  - add_gatt_services(server): GTBS (UUID_SERVICE_GTBS 0x184C) with the
 *    mandatory characteristics of TBS 1.0 Table 3.1: Bearer Provider Name
 *    (0x2BB3), Bearer UCI (0x2BB4), Bearer Technology (0x2BB5), Bearer URI
 *    Schemes Supported List (0x2BB6), Bearer List Current Calls (0x2BB9),
 *    Content Control ID (0x2BBA), Status Flags (0x2BBB), Call State
 *    (0x2BBD), Call Control Point (0x2BBE), Call Control Point Optional
 *    Opcodes (0x2BBF), Termination Reason (0x2BC0), Incoming Call (0x2BC1).
 *  - Call Control Point writes (write callback, BTstack thread): answer with
 *    the control point notification ("invalid call index" etc. while there
 *    is no call).
 *  - GattServer drops the CCC state of a disconnected client itself.
 * Bumble side: tools/emu/le_audio/ccp.py (reads GTBS from the sink side).
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_ccp.h"

namespace le_audio {

namespace {

class CcpModule : public Module {
public:
    const char *name() const override { return "CCP"; }
};

} /* namespace */

Module *ccp_module() {
    static CcpModule module;
    return &module;
}

} /* namespace le_audio */
