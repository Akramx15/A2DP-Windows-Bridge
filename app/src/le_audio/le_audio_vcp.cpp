/*
 * LE Audio (experimental) - VCP Volume Controller (stub, does nothing yet)
 *
 * Responsibility: the headset volume follows the Windows volume and the
 * other way round, as AVRCP absolute volume does for A2DP (volume_sync.h).
 * The LE streaming thread already runs VolumeSync with an LE target that
 * calls Core::remote_volume() / Core::request_volume(), which ask this
 * module; while it answers -1 / false VolumeSync does nothing.
 *
 * Hooks to use (le_audio_module.h):
 *  - on_gatt_ready(dev): VCS (UUID_SERVICE_VCS 0x1844): read and subscribe to
 *    Volume State (0x2B7D: volume 0-255, mute, change counter) and Volume
 *    Flags (0x2B7F); remember the Volume Control Point (0x2B7E). VOCS / AICS
 *    are included (secondary) services: dev.services lists them with
 *    primary == false and included_by set.
 *  - remote_volume() (any thread): the last Volume State mapped 0-255 to
 *    0-127, -1 while unknown (no VCS, not read yet, link down). Keep it in an
 *    atomic. For a coordinated set use the first member's volume.
 *  - request_volume(level) (any thread): Core::instance().post() a "Set
 *    Absolute Volume" write (opcode 0x04, change counter, volume) to every
 *    connected member and return true. On "Invalid Change Counter" (0x80)
 *    read Volume State again and retry once.
 *  - on_disconnected(dev, reason): forget that device.
 * Bumble side: tools/emu/le_audio/vcp.py (bumble.profiles.vcs on the sink).
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_vcp.h"

namespace le_audio {

namespace {

class VcpModule : public Module {
public:
    const char *name() const override { return "VCP"; }
};

} /* namespace */

Module *vcp_module() {
    static VcpModule module;
    return &module;
}

} /* namespace le_audio */
