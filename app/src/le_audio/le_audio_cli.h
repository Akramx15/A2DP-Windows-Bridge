/*
 * LE Audio (experimental) - command-line streaming (A2DPWB --cli --le-audio)
 *
 *   A2DPWB.exe --cli --le-audio -d <addr> [--le-device <addr2>] [--le-preset 48_2_2]
 *              [--le-low-latency]
 *              [--test-tone] [--duration <s>] [--hci-tcp host:port] [--hci-capture f.pklg]
 *   A2DPWB.exe --cli --le-scan [seconds]
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_CLI_H
#define LE_AUDIO_CLI_H

#include "capture_mode.h"

#include <cstdint>
#include <functional>
#include <string>
#include <vector>

struct LeAudioCliOptions {
    std::vector<std::string> devices;   /* "XX:XX:XX:XX:XX:XX", the group to stream to */
    bool scan_only = false;             /* --le-scan: list LE Audio devices and exit */
    uint32_t scan_seconds = 8;
    std::string preset;                 /* --le-preset: try only this preset (e.g. "48_2_2") */
    bool low_latency = false;           /* --le-low-latency: GMAP presets for a gaming headset (UGT) */
    std::string hci_tcp;                /* --hci-tcp */
    std::string hci_capture;            /* --hci-capture */
    bool test_tone = false;             /* --test-tone */
    uint32_t duration_s = 0;            /* --duration (0 = until Ctrl+C) */
    CaptureMode capture_mode = CaptureMode::SystemLoopback;
    std::wstring audio_device_id;       /* virtual device mode */
    std::string afh;                    /* --afh */
    std::function<bool()> running;      /* false on Ctrl+C */
    std::string sim_volume;             /* --le-sim-volume (tests): VolumeSync on a simulated Windows volume */
};

/* Returns the process exit code */
int le_audio_cli_run(const LeAudioCliOptions &options);

#endif /* LE_AUDIO_CLI_H */
