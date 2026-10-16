/*
 * A2DP Service - streaming thread of an LE Audio profile (experimental)
 *
 * Same lifecycle as the A2DP streaming thread (a2dp_service.cpp), on
 * le_audio::Core instead of A2DP: BTstack up (with LE Audio enabled),
 * connect the device (scan first for its address type), set up the BAP
 * unicast stream (LC3), capture at the LC3 rate into the stream, keep the
 * Windows volume in sync through the VCP module (when it knows a volume),
 * and set the stream up again when a link or CIS is lost.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "a2dp_service.h"
#include "audio_device_enum.h"
#include "bt_device.h"
#include "btstack_transport.h"
#include "capture_mode.h"
#include "debug_log.h"
#include "localization.h"
#include "test_tone.h"
#include "volume_sync.h"
#include "wasapi_capture.h"
#include "le_audio/le_audio_core.h"

#include <cstdio>
#include <string>
#include <windows.h>

namespace {

std::wstring utf8_to_wide_le(const std::string &s) {
    if (s.empty()) return std::wstring();
    int n = MultiByteToWideChar(CP_UTF8, 0, s.c_str(), (int)s.size(), nullptr, 0);
    std::wstring w(n > 0 ? n : 0, L'\0');
    if (n > 0) MultiByteToWideChar(CP_UTF8, 0, s.c_str(), (int)s.size(), &w[0], n);
    return w;
}

} /* namespace */

void A2dpService::le_audio_streaming_thread_inner() {
    auto &p = active_profile_;
    le_audio::Core &core = le_audio::Core::instance();
    auto cancelled = [this]() { return stop_requested_.load(); };
    auto finish_idle = [this]() {
        running_.store(false);
        notify_state(State::Idle, L("status.ready"));
    };
    auto fail = [this](const std::string &text) {
        running_.store(false);
        notify_state(State::Error, text);
    };

    LOG_INFO("A2dpService: LE Audio (experimental) stream to %s (%s)", p.device_address.c_str(),
             p.device_name.c_str());
    if (!le_audio_enabled_.load()) {
        fail(L("error.le_audio_disabled"));
        return;
    }
    le_audio::LeAddr addr;
    if (!le_audio::addr_from_string(p.device_address, addr.addr)) {
        fail(L("error.invalid_address"));
        return;
    }
    if (p.capture_mode == "app" && !p.test_tone) {
        fail(L("error.le_audio_capture"));
        return;
    }

    notify_state(State::Connecting, L("status.initializing_btstack"));
    if (!ensure_btstack_init()) {
        fail(L(btstack_init_error_key()));
        return;
    }
    for (int i = 0; i < 50 && !core.ready() && !cancelled(); i++) Sleep(100);
    if (cancelled()) { finish_idle(); return; }
    if (!core.ready()) {
        fail(L("error.le_audio_unavailable"));
        return;
    }

    /* Connect (the address type comes from the advertisement) and stream */
    std::vector<int> ids;
    le_audio::StreamStatus status;
    auto connect_and_stream = [&](std::string &error) {
        notify_state(State::Connecting, L("status.connecting_device"));
        le_audio::ScanResult found;
        le_audio::LeAddr target = addr;
        std::string name = p.device_name;
        if (core.find(addr.addr, 8000, found, cancelled)) {
            target.type = found.addr.type;
            if (name.empty()) name = found.name;
        }
        if (cancelled()) return false;
        int id = core.connect(target, name, 30000, &error, cancelled);
        if (id < 0) {
            char text[256];
            snprintf(text, sizeof(text), L("error.le_audio_connect"), error.c_str());
            error = text;
            return false;
        }
        ids = {id};
        le_audio::StreamRequest request;
        if (!core.start_stream(ids, request, &status, &error, cancelled)) {
            char text[256];
            snprintf(text, sizeof(text), L("error.le_audio_stream"), error.c_str());
            error = text;
            core.stop_stream();
            return false;
        }
        return true;
    };
    std::string error;
    if (!connect_and_stream(error)) {
        core.disconnect_all();
        if (cancelled()) finish_idle();
        else fail(error);
        return;
    }

    /* Audio at the LC3 rate (WASAPI converts to it) */
    const uint32_t rate = core.stream().sample_rate();
    auto push = [&core](const uint8_t *data, uint32_t frames, uint32_t channels, uint32_t sr, uint32_t bits) {
        core.stream().push_pcm(data, frames, channels, sr, bits);
    };
    TestTone test_tone;
    WasapiCapture capture;
    std::wstring device_id;
    if (p.capture_mode == "virtual") device_id = utf8_to_wide_le(p.audio_device_id);
    bool switched_default = false;
    bool capture_ok = true;
    if (p.test_tone) {
        test_tone.start(rate, push);
    } else {
        if (!device_id.empty() && p.auto_switch_device) {
            original_default_device_ = AudioDeviceEnumerator::get_default_device_id();
            switched_default = AudioDeviceEnumerator::set_default_device(device_id);
        }
        capture_ok = capture.init(rate, device_id.empty() ? nullptr : device_id.c_str()) && capture.start(push);
    }
    if (!capture_ok) {
        core.stop_stream();
        core.disconnect_all();
        if (switched_default && !original_default_device_.empty()) {
            AudioDeviceEnumerator::set_default_device(original_default_device_);
            original_default_device_.clear();
        }
        fail(L("error.wasapi_init"));
        return;
    }

    uint32_t src_rate = p.test_tone ? rate : capture.get_sample_rate();
    uint32_t src_ch = p.test_tone ? TestTone::CHANNELS : capture.get_channels();
    uint32_t src_bits = p.test_tone ? TestTone::BITS_PER_SAMPLE : capture.get_bits_per_sample();
    auto report = [&]() {
        notify_state(State::Streaming, L("status.connected"));
        const le_audio::StreamPlan &plan = status.plan;
        notify_stream_info({"LC3 (LE Audio)", plan.preset.kbps_per_channel() * (uint32_t)plan.channel_count(),
                            plan.preset.sampling_hz, (uint32_t)plan.channel_count(), src_rate, src_ch, src_bits});
        LOG_INFO("A2dpService: LE Audio streaming: %s", status.summary.c_str());
    };
    report();
    save_device(p.device_address.c_str(), p.device_name, true);

    /* Windows volume <-> headset volume through the VCP module */
    bool sync_volume = !p.test_tone && p.volume_sync != "off";
    std::wstring volume_device_id;
    if (p.volume_sync == "device") volume_device_id = utf8_to_wide_le(p.volume_device_id);
    else if (p.capture_mode == "virtual") volume_device_id = device_id;
    VolumeSync volume_sync(volume_device_id,
                           {[]() { return le_audio::Core::instance().remote_volume(); },
                            [](uint8_t level) { le_audio::Core::instance().request_volume(level); }});

    while (!stop_requested_.load()) {
        Sleep(200);
        volume_sync.tick(sync_volume);
        if (!core.stream_lost()) continue;

        /* A device or CIS went away: tear down and set it up again */
        LOG_WARN("A2dpService: LE Audio stream lost, reconnecting");
        notify_state(State::Reconnecting, L("status.connection_lost"));
        core.stop_stream();
        bool restored = false;
        for (int attempt = 1; attempt <= 10 && !stop_requested_.load(); attempt++) {
            char msg[64];
            snprintf(msg, sizeof(msg), L("status.reconnect_attempt"), attempt, 10);
            notify_state(State::Reconnecting, msg);
            for (int i = 0; i < 30 && !stop_requested_.load(); i++) Sleep(100);
            if (stop_requested_.load()) break;
            std::string err;
            if (connect_and_stream(err)) {
                restored = true;
                break;
            }
            LOG_WARN("A2dpService: LE Audio reconnect attempt %d failed: %s", attempt, err.c_str());
        }
        if (!restored) {
            if (!stop_requested_.load()) notify_state(State::Error, L("error.reconnect_failed"));
            break;
        }
        report();
    }

    LOG_INFO("A2dpService: LE Audio streaming stopped, cleaning up");
    test_tone.stop();
    capture.stop();
    core.stop_stream();
    core.disconnect_all();
    volume_sync.stop();
    if (!original_default_device_.empty()) {
        AudioDeviceEnumerator::set_default_device(original_default_device_);
        original_default_device_.clear();
    }
    running_.store(false);
    if (stop_requested_.load()) notify_state(State::Idle, L("status.ready"));
}
