/*
 * LE Audio (experimental) - command-line streaming (A2DPWB --cli --le-audio)
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_cli.h"
#include "le_audio_core.h"
#include "le_audio_vcp.h"
#include "le_audio_tmap_gmap.h"

#include "app_settings.h"
#include "audio_device_enum.h"
#include "bt_adapter_enum.h"
#include "btstack_transport.h"
#include "config_path.h"
#include "hci_capture.h"
#include "test_tone.h"
#include "wasapi_capture.h"

#include <chrono>
#include <cstdio>
#include <thread>
#include <windows.h>

namespace {

void print_stats(le_audio::Core &core) {
    le_audio::Stream::Stats s = core.stream().stats();
    printf("LE Audio: %llu SDU sets sent (%llu SDUs), %llu encoded, %llu silence, %llu underruns, "
           "%llu dropped, %llu send errors\n",
           (unsigned long long)s.sets_sent, (unsigned long long)s.sdus_sent,
           (unsigned long long)s.sets_encoded, (unsigned long long)s.silence_sets,
           (unsigned long long)s.underruns, (unsigned long long)s.overflows,
           (unsigned long long)s.send_errors);
    fflush(stdout);
}

/* Connect every address of the group; the ids in order. False if one fails. */
bool connect_group(le_audio::Core &core, const std::vector<std::string> &addrs, std::vector<int> &ids,
                   const std::function<bool()> &cancelled) {
    ids.clear();
    for (const auto &a : addrs) {
        le_audio::LeAddr addr;
        if (!le_audio::addr_from_string(a, addr.addr)) {
            fprintf(stderr, "Invalid LE address: %s\n", a.c_str());
            return false;
        }
        /* The address (an RPA of a bonded device changes: find() resolves
         * it), its type and the name come from the advertisement */
        le_audio::ScanResult found;
        std::string name;
        if (core.find(addr.addr, 6000, found, cancelled)) {
            addr = found.addr;
            name = found.name;
            printf("Found %s \"%s\" (address type %s)%s\n", a.c_str(), name.c_str(),
                   addr.type ? "random" : "public", found.le_audio ? ", LE Audio" : "");
            if (found.resolved && found.addr.str() != a)
                printf("  advertising as %s now (identity %s)\n", found.addr.str().c_str(),
                       found.identity.str().c_str());
        } else {
            printf("%s not seen advertising, connecting with a public address\n", a.c_str());
        }
        std::string error;
        int id = core.connect(addr, name, 30000, &error, cancelled);
        if (id < 0) {
            fprintf(stderr, "LE Audio: cannot connect %s: %s\n", a.c_str(), error.c_str());
            return false;
        }
        ids.push_back(id);
    }
    return true;
}

} /* namespace */

int le_audio_cli_run(const LeAudioCliOptions &o) {
    auto cancelled = [&o]() { return o.running && !o.running(); };
    le_audio::Core &core = le_audio::Core::instance();
    core.set_enabled(true);

    BtStackTransport transport;
    transport.set_link_key_dir(get_config_dir());
    {
        AppSettings settings;
        settings.load();
        transport.set_firmware_dir(get_config_dir());
        bool is_auto = BtAdapterEnumerator::is_auto_chip(settings.bt_chip_pid, settings.bt_chip_fw_stem);
        transport.set_product_id(BtAdapterEnumerator::resolve_chip_pid(
            settings.bt_chip_pid, settings.bt_chip_fw_stem,
            is_auto ? BtAdapterEnumerator::enumerate() : std::vector<BtAdapterInfo>{}));
        if (!settings.bt_chip_fw_stem.empty()) transport.set_fw_stem(settings.bt_chip_fw_stem);
        BtStackTransport::set_afh_policy(o.afh.empty() ? settings.afh : o.afh);
    }
    if (!o.hci_tcp.empty()) transport.set_hci_tcp(o.hci_tcp);
    if (!o.hci_tcp.empty() || !o.hci_capture.empty()) transport.set_hci_dump_enabled(false);
    if (!o.hci_capture.empty()) {
        transport.set_hci_capture_enabled(true);
        if (!hci_capture::start(o.hci_capture)) {
            fprintf(stderr, "Cannot create HCI capture file %s\n", o.hci_capture.c_str());
            return 1;
        }
    }

    printf("\n[LE Audio, experimental] Initializing BTstack (%s transport)...\n",
           o.hci_tcp.empty() ? "WinUSB" : "H4 over TCP");
    if (!transport.init(nullptr)) {
        fprintf(stderr, "Failed to initialize BTstack.\n");
        hci_capture::stop();
        return 1;
    }
    for (int i = 0; i < 50 && !core.ready(); i++) Sleep(100);
    if (!core.ready()) {
        fprintf(stderr, "LE Audio did not start (does the adapter support LE?)\n");
        transport.shutdown();
        hci_capture::stop();
        return 1;
    }

    if (o.scan_only) {
        printf("Scanning for LE Audio devices (%u s)...\n", o.scan_seconds);
        auto results = core.scan(o.scan_seconds * 1000u, cancelled);
        int n = 0;
        for (const auto &r : results) {
            if (!r.le_audio) continue;
            n++;
            printf("  %s  %-24s  RSSI %4d  %s%s%s\n", r.addr.str().c_str(),
                   r.name.empty() ? "(no name)" : r.name.c_str(), r.rssi,
                   r.addr.type ? "random" : "public", r.rsi.empty() ? "" : ", set member",
                   r.note.empty() ? "" : (", " + r.note).c_str());
        }
        printf("%d LE Audio device(s) found\n", n);
        transport.shutdown();
        hci_capture::stop();
        return 0;
    }

    if (o.devices.empty()) {
        fprintf(stderr, "No LE Audio device given (-d <address>; --le-scan lists them)\n");
        transport.shutdown();
        hci_capture::stop();
        return 1;
    }

    le_audio::StreamRequest request;
    if (!o.preset.empty()) request.preset_order = {o.preset};
    /* GMAP low latency (only the flag, not the GUI setting); an explicit
     * --le-preset wins */
    le_audio::set_gmap_low_latency(o.low_latency && o.preset.empty());
    if (o.low_latency && !o.preset.empty())
        printf("--le-preset given: --le-low-latency is not used\n");

    std::vector<int> ids;
    le_audio::StreamStatus status;
    std::string error;
    int rc = 0;
    printf("\nConnecting %u LE Audio device(s)...\n", (unsigned)o.devices.size());
    if (!connect_group(core, o.devices, ids, cancelled) ||
        !core.start_stream(ids, request, &status, &error, cancelled)) {
        if (!error.empty()) fprintf(stderr, "LE Audio: cannot stream: %s\n", error.c_str());
        core.stop_stream();
        core.disconnect_all();
        transport.shutdown();
        hci_capture::stop();
        return 1;
    }
    printf("\nStreaming LE Audio: %s\n", status.summary.c_str());
    fflush(stdout);

    /* Audio source at the LC3 rate */
    const uint32_t rate = core.stream().sample_rate();
    TestTone tone;
    WasapiCapture capture;
    auto push = [&core](const uint8_t *data, uint32_t frames, uint32_t channels, uint32_t sr, uint32_t bits) {
        core.stream().push_pcm(data, frames, channels, sr, bits);
    };
    bool capture_ok = true;
    if (o.test_tone) {
        printf("Audio: test tone (1 kHz left / 1.5 kHz right) at %u Hz\n", rate);
        tone.start(rate, push);
    } else {
        const wchar_t *dev = o.capture_mode == CaptureMode::VirtualDevice && !o.audio_device_id.empty()
                           ? o.audio_device_id.c_str() : nullptr;
        capture_ok = capture.init(rate, dev) && capture.start(push);
        if (capture_ok)
            printf("Audio: WASAPI capture %u Hz %u ch (LC3 at %u Hz)\n", capture.get_sample_rate(),
                   capture.get_channels(), rate);
    }
    if (!capture_ok) {
        fprintf(stderr, "Failed to start audio capture\n");
        rc = 1;
    }

    /* Test only: the headset volume synced with a simulated Windows volume */
    le_audio::vcp::SimulatedWindowsVolume sim_volume;
    if (!o.sim_volume.empty() && !sim_volume.parse(o.sim_volume)) {
        fprintf(stderr, "Invalid --le-sim-volume %s\n", o.sim_volume.c_str());
        rc = 1;
    }

    auto start = std::chrono::steady_clock::now();
    auto last_print = start;
    int restarts = 0;
    while (rc == 0 && (!o.running || o.running())) {
        Sleep(100);
        auto now = std::chrono::steady_clock::now();
        if (o.duration_s && now - start >= std::chrono::seconds(o.duration_s)) {
            printf("\n--duration %u s reached\n", o.duration_s);
            break;
        }
        if (now - last_print >= std::chrono::seconds(5)) {
            last_print = now;
            print_stats(core);
        }
        if (!o.sim_volume.empty()) sim_volume.tick();
        if (core.stream_lost()) {
            /* A device or CIS went away: tear down and set it up again */
            printf("\n*** LE Audio stream lost - setting it up again ***\n");
            core.stop_stream();
            bool again = false;
            while (!again && restarts < 3 && (!o.running || o.running())) {
                restarts++;
                Sleep(2000);
                again = connect_group(core, o.devices, ids, cancelled) &&
                        core.start_stream(ids, request, &status, &error, cancelled);
                if (!again) core.stop_stream();
            }
            if (!again) {
                fprintf(stderr, "LE Audio: could not restore the stream\n");
                rc = 1;
                break;
            }
            printf("*** LE Audio stream restored: %s ***\n", status.summary.c_str());
        }
    }

    printf("\nShutting down...\n");
    tone.stop();
    capture.stop();
    Sleep(100);
    print_stats(core);
    core.stop_stream();
    core.disconnect_all();
    transport.shutdown();
    hci_capture::stop();
    printf("Done.\n");
    return rc;
}
