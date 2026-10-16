/*
 * LE Audio (experimental) - VCP Volume Controller
 *
 * The headset volume of an LE Audio device (Volume Control Service) on the
 * scale VolumeSync uses for AVRCP absolute volume (0-127); see
 * le_audio_vcp.cpp for the behaviour.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_VCP_H
#define LE_AUDIO_VCP_H

#include "le_audio_module.h"

#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

class VolumeSync;

namespace le_audio {

/* Registered in le_audio_modules.cpp */
Module *vcp_module();

namespace vcp {

/* AVRCP / VolumeSync level 0-127 <-> VCS Volume_Setting 0-255, both
 * rounded to the nearest value. to_level(to_setting(l)) == l for every
 * level, so a level sent to the headset and reported back stays the same. */
constexpr uint8_t to_setting(int level) {
    return static_cast<uint8_t>(level <= 0 ? 0 : level >= 127 ? 255 : (level * 255 + 63) / 127);
}
constexpr int to_level(int setting) {
    return setting <= 0 ? 0 : setting >= 255 ? 127 : (setting * 127 + 127) / 255;
}

/* Test only (A2DPWB --cli --le-audio --le-sim-volume ...): runs VolumeSync
 * with the LE Audio volume target against a simulated Windows volume, so the
 * emulator tests exercise the real sync logic without ever touching the
 * user's output device. Spec: "<level>[,<seconds>:<level>...]" - the
 * simulated Windows volume (0-127) at the start, then the level it changes
 * to at the given seconds after the stream started (the volume keys). */
class SimulatedWindowsVolume {
public:
    SimulatedWindowsVolume();
    ~SimulatedWindowsVolume();
    SimulatedWindowsVolume(const SimulatedWindowsVolume &) = delete;
    SimulatedWindowsVolume &operator=(const SimulatedWindowsVolume &) = delete;

    bool parse(const std::string &spec);
    /* Call every 100-200 ms while streaming; the first call starts it */
    void tick();

private:
    std::unique_ptr<VolumeSync> sync_;
    int windows_ = 0;
    std::vector<std::pair<uint32_t, int>> steps_;  /* ms after start, level */
    size_t next_ = 0;
    uint32_t start_ms_ = 0;
};

} /* namespace vcp */
} /* namespace le_audio */

#endif /* LE_AUDIO_VCP_H */
