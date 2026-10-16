/*
 * LE Audio (experimental) - MCP media control server (stub, does nothing yet)
 *
 * Responsibility: the PC is the Media Control Server. The headset (Media
 * Control Client) reads the player state from GMCS and sends play / pause /
 * next / previous through the Media Control Point, which A2DPWB turns into
 * Windows media keys (as the AVRCP Target does for A2DP, see
 * BtStackTransport::handle_avrcp_operation and set_media_keys_enabled()).
 *
 * Hooks to use (le_audio_module.h, le_audio_gatt_server.h):
 *  - add_gatt_services(server): GMCS (UUID_SERVICE_GMCS 0x1849) with at least
 *    Media Player Name (0x2B93), Track Changed (0x2B96), Track Title
 *    (0x2B97), Track Duration (0x2B98), Track Position (0x2B99), Media State
 *    (0x2BA3), Media Control Point (0x2BA4) with Opcodes Supported (0x2BA5)
 *    and Content Control ID (0x2BBA), each with
 *    GattServer::add_characteristic(uuid, properties, read, write).
 *  - on_stream_state(status, streaming): Media State playing / paused, sent
 *    with GattServer::notify_all() (BTstack thread).
 *  - Control point writes arrive in the write callback (BTstack thread):
 *    answer with the control point notification; send the media key from
 *    another thread (never block the BTstack thread).
 * The CCID of GMCS belongs in the Enable metadata (CCID List, 0x05) of a
 * media stream; StreamRequest has no field for it yet. Adding one means a
 * small change in le_audio_presets.h and in Unicast::enter(Phase::Enable)
 * (le_audio_bap.cpp) - keep it a separate commit.
 * Bumble side: tools/emu/le_audio/mcp.py (a GMCS client on the sink).
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_mcp.h"

namespace le_audio {

namespace {

class McpModule : public Module {
public:
    const char *name() const override { return "MCP"; }
};

} /* namespace */

Module *mcp_module() {
    static McpModule module;
    return &module;
}

} /* namespace le_audio */
