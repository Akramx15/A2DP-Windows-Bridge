/*
 * LE Audio (experimental) - MCP: the Windows side of the media control server
 *
 * See le_audio_mcp_media.h. Compiled with /std:c++20 (C++/WinRT needs C++20
 * coroutine headers); nothing here touches BTstack.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_mcp_media.h"
#include "debug_log.h"

#include <winrt/base.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.ApplicationModel.h>
#include <winrt/Windows.Media.Control.h>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <deque>
#include <map>
#include <mutex>
#include <optional>
#include <thread>

#pragma comment(lib, "windowsapp")

namespace le_audio {
namespace mcp {

const char *opcode_name(uint8_t opcode) {
    switch (opcode) {
    case OP_PLAY:           return "PLAY";
    case OP_PAUSE:          return "PAUSE";
    case OP_FAST_REWIND:    return "FAST_REWIND";
    case OP_FAST_FORWARD:   return "FAST_FORWARD";
    case OP_STOP:           return "STOP";
    case OP_MOVE_RELATIVE:  return "MOVE_RELATIVE";
    case 0x20:              return "PREVIOUS_SEGMENT";
    case 0x21:              return "NEXT_SEGMENT";
    case 0x22:              return "FIRST_SEGMENT";
    case 0x23:              return "LAST_SEGMENT";
    case 0x24:              return "GOTO_SEGMENT";
    case OP_PREVIOUS_TRACK: return "PREVIOUS_TRACK";
    case OP_NEXT_TRACK:     return "NEXT_TRACK";
    case 0x32:              return "FIRST_TRACK";
    case 0x33:              return "LAST_TRACK";
    case 0x34:              return "GOTO_TRACK";
    case 0x40:              return "PREVIOUS_GROUP";
    case 0x41:              return "NEXT_GROUP";
    case 0x42:              return "FIRST_GROUP";
    case 0x43:              return "LAST_GROUP";
    case 0x44:              return "GOTO_GROUP";
    default:                return "unknown";
    }
}

const char *state_name(uint8_t state) {
    switch (state) {
    case STATE_INACTIVE: return "inactive";
    case STATE_PLAYING:  return "playing";
    case STATE_PAUSED:   return "paused";
    case STATE_SEEKING:  return "seeking";
    default:             return "?";
    }
}

int32_t MediaInfo::position_now() const {
    if (position < 0) return -1;
    int64_t p = position;
    if (state == STATE_PLAYING && rate > 0)
        p += (int64_t)((double)(GetTickCount64() - updated_ms) / 10.0 * rate);
    if (duration > 0 && p > duration) p = duration;
    return (int32_t)std::max<int64_t>(0, p);
}

int8_t MediaInfo::playback_speed() const {
    if (rate <= 0) return 0;
    double s = std::round(64.0 * std::log2(rate));
    return (int8_t)std::clamp(s, -128.0, 127.0);
}

namespace {

using namespace winrt::Windows::Media::Control;
using winrt::Windows::Foundation::AsyncStatus;
using winrt::Windows::Foundation::IAsyncOperation;

constexpr uint32_t POLL_MS = 1000;           /* session state */
constexpr uint32_t POLL_AFTER_COMMAND_MS = 300;
constexpr uint64_t RETRY_MANAGER_MS = 30000; /* SMTC not available: try again */
constexpr auto WINRT_TIMEOUT = std::chrono::milliseconds(2000);

/* Result of an async WinRT call, waited for with a timeout (the backend
 * thread is MTA, blocking is allowed there) */
template <typename T>
std::optional<T> wait_result(IAsyncOperation<T> const &op) {
    if (op.wait_for(WINRT_TIMEOUT) != AsyncStatus::Completed) {
        try { op.Cancel(); } catch (...) {}
        return std::nullopt;
    }
    return op.GetResults();
}

/* Press and release of a media key (as BtStackTransport does for AVRCP
 * buttons): Windows hands it to the app that owns media playback */
void send_media_key(WORD vk) {
    INPUT in[2] = {};
    in[0].type = INPUT_KEYBOARD;
    in[0].ki.wVk = vk;
    in[0].ki.dwFlags = KEYEVENTF_EXTENDEDKEY;
    in[1] = in[0];
    in[1].ki.dwFlags = KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP;
    if (SendInput(2, in, sizeof(INPUT)) != 2)
        LOG_WARN("MCP: SendInput for media key 0x%02X failed (error %lu)", vk, GetLastError());
}

/* "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify" -> "SpotifyMusic",
 * "C:\...\foobar2000.exe" / "Spotify.exe" -> "foobar2000" / "Spotify" */
std::string name_from_aumid(std::string s) {
    size_t bang = s.find('!');
    if (bang != std::string::npos) {
        s = s.substr(0, bang);
        s = s.substr(0, s.find('_'));
        size_t dot = s.rfind('.');
        if (dot != std::string::npos && dot + 1 < s.size()) s = s.substr(dot + 1);
    }
    size_t slash = s.find_last_of("\\/");
    if (slash != std::string::npos) s = s.substr(slash + 1);
    if (s.size() > 4 && _stricmp(s.c_str() + s.size() - 4, ".exe") == 0) s.resize(s.size() - 4);
    return s.empty() ? "Windows" : s;
}

uint32_t fold_hash(const std::string &s) {
    size_t h = std::hash<std::string>()(s);
    return (uint32_t)(h ^ (h >> 32));
}

const char *SIM_PLAYER = "A2DPWB test player";
const int32_t SIM_DURATION = 18000;          /* 180 s */

} /* namespace */

struct MediaBackend::Impl {
    bool dry_run = false;
    ChangedFn changed;
    std::thread thread;

    std::mutex m;
    std::condition_variable cv;
    bool stopping = false;
    std::deque<std::function<void()>> tasks;
    MediaInfo info;                          /* guarded by m */

    /* backend thread only */
    GlobalSystemMediaTransportControlsSessionManager manager{nullptr};
    uint64_t manager_tried_ms = 0;
    bool manager_logged = false;
    std::map<std::string, std::string> names;   /* AUMID -> display name */
    int sim_track = 1;

    MediaInfo current() {
        std::lock_guard<std::mutex> lock(m);
        return info;
    }

    /* New state; changed() unless only the position moved on as expected */
    void publish(const MediaInfo &next) {
        bool differs;
        MediaInfo prev;
        {
            std::lock_guard<std::mutex> lock(m);
            prev = info;
            const int32_t a = info.position_now(), b = next.position_now();
            differs = info.session != next.session || info.state != next.state || info.player != next.player ||
                      info.title != next.title || info.duration != next.duration || info.track != next.track ||
                      info.can_seek != next.can_seek || info.playback_speed() != next.playback_speed() ||
                      (a < 0) != (b < 0) || std::abs(a - b) > 200;
            info = next;
        }
        if (!differs) return;
        if (prev.state != next.state || prev.player != next.player)
            LOG_INFO("MCP: media player \"%s\": %s", next.session ? next.player.c_str() : "(none)",
                     state_name(next.state));
        if (changed) changed();
    }

    /* ---- Windows media session (SMTC) ---- */

    void ensure_manager() {
        if (manager) return;
        const uint64_t now = GetTickCount64();
        if (manager_tried_ms && now - manager_tried_ms < RETRY_MANAGER_MS) return;
        manager_tried_ms = now;
        try {
            if (auto mgr = wait_result(GlobalSystemMediaTransportControlsSessionManager::RequestAsync()))
                manager = *mgr;
        } catch (const winrt::hresult_error &e) {
            if (!manager_logged)
                LOG_WARN("MCP: Windows media sessions not available (0x%08X): media keys only, no player state",
                         (unsigned)e.code());
            manager_logged = true;
            return;
        } catch (...) {
        }
        if (!manager && !manager_logged) {
            LOG_WARN("MCP: Windows media sessions not available: media keys only, no player state");
            manager_logged = true;
        }
        if (manager) LOG_INFO("MCP: reading the Windows media session (System Media Transport Controls)");
    }

    GlobalSystemMediaTransportControlsSession session() {
        ensure_manager();
        if (!manager) return nullptr;
        try {
            return manager.GetCurrentSession();
        } catch (...) {
            return nullptr;
        }
    }

    std::string display_name(const std::string &aumid) {
        auto it = names.find(aumid);
        if (it != names.end()) return it->second;
        std::string name;
        try {
            auto app = winrt::Windows::ApplicationModel::AppInfo::GetFromAppUserModelId(winrt::to_hstring(aumid));
            name = winrt::to_string(app.DisplayInfo().DisplayName());
        } catch (...) {
        }
        if (name.empty()) name = name_from_aumid(aumid);
        names[aumid] = name;
        return name;
    }

    MediaInfo read_session() {
        MediaInfo mi;
        auto s = session();
        if (!s) return mi;
        try {
            const std::string aumid = winrt::to_string(s.SourceAppUserModelId());
            mi.session = true;
            mi.player = display_name(aumid);
            auto pb = s.GetPlaybackInfo();
            switch (pb.PlaybackStatus()) {
            case GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing:
                mi.state = STATE_PLAYING;
                break;
            case GlobalSystemMediaTransportControlsSessionPlaybackStatus::Closed:
                mi.state = STATE_INACTIVE;
                break;
            default:                         /* paused, stopped, opened, changing */
                mi.state = STATE_PAUSED;
                break;
            }
            if (auto rate = pb.PlaybackRate()) mi.rate = rate.Value();
            mi.can_seek = pb.Controls().IsPlaybackPositionEnabled();
            std::string artist;
            if (auto props = wait_result(s.TryGetMediaPropertiesAsync())) {
                if (*props) {
                    mi.title = winrt::to_string((*props).Title());
                    artist = winrt::to_string((*props).Artist());
                }
            }
            auto tl = s.GetTimelineProperties();
            const int64_t start = tl.StartTime().count(), end = tl.EndTime().count();
            const int64_t pos = tl.Position().count();          /* 100 ns units */
            if (end > start) mi.duration = (int32_t)std::min<int64_t>((end - start) / 100000, INT32_MAX - 1);
            if (end > start || pos > 0) {
                int64_t p = pos - start;
                if (mi.state == STATE_PLAYING) {
                    const int64_t elapsed = (winrt::clock::now() - tl.LastUpdatedTime()).count();
                    if (elapsed > 0 && elapsed < 36000000000LL) p += (int64_t)((double)elapsed * mi.rate);
                }
                mi.position = (int32_t)std::clamp<int64_t>(p / 100000, 0, INT32_MAX - 1);
                if (mi.duration > 0) mi.position = std::min(mi.position, mi.duration);
                mi.updated_ms = GetTickCount64();
            }
            mi.track = fold_hash(aumid + '\n' + mi.title + '\n' + artist + '\n' + std::to_string(mi.duration));
        } catch (...) {
            /* the session went away while reading */
            return MediaInfo();
        }
        return mi;
    }

    void poll() {
        if (dry_run) return;
        publish(read_session());
    }

    bool seek_session(GlobalSystemMediaTransportControlsSession const &s, int32_t position) {
        try {
            const int64_t start = s.GetTimelineProperties().StartTime().count();
            auto ok = wait_result(s.TryChangePlaybackPositionAsync(start + (int64_t)position * 100000));
            return ok && *ok;
        } catch (...) {
            return false;
        }
    }

    uint8_t execute_windows(uint8_t op, int32_t param) {
        const MediaInfo cur = current();
        auto s = session();
        if (!s) {
            /* No media session: the media keys AVRCP buttons use */
            WORD vk = 0;
            switch (op) {
            case OP_PLAY:
            case OP_PAUSE:          vk = VK_MEDIA_PLAY_PAUSE; break;
            case OP_STOP:           vk = VK_MEDIA_STOP; break;
            case OP_NEXT_TRACK:     vk = VK_MEDIA_NEXT_TRACK; break;
            case OP_PREVIOUS_TRACK: vk = VK_MEDIA_PREV_TRACK; break;
            default:
                LOG_INFO("MCP: %s: no media session in Windows", opcode_name(op));
                return RESULT_MEDIA_PLAYER_INACTIVE;
            }
            LOG_INFO("MCP: %s -> media key 0x%02X (no media session in Windows)", opcode_name(op), vk);
            send_media_key(vk);
            return RESULT_SUCCESS;
        }
        std::optional<bool> ok;
        try {
            switch (op) {
            case OP_PLAY:
                if (cur.state == STATE_PLAYING) ok = true;
                else ok = wait_result(s.TryPlayAsync());
                break;
            case OP_PAUSE:
                if (cur.state == STATE_PAUSED) ok = true;
                else ok = wait_result(s.TryPauseAsync());
                break;
            case OP_STOP:
                /* MCS: Stop = paused at the start of the track */
                ok = wait_result(s.TryStopAsync());
                if (!ok || !*ok) {
                    ok = wait_result(s.TryPauseAsync());
                    if (ok && *ok && cur.can_seek) seek_session(s, 0);
                }
                break;
            case OP_NEXT_TRACK:
                ok = wait_result(s.TrySkipNextAsync());
                break;
            case OP_PREVIOUS_TRACK:
                ok = wait_result(s.TrySkipPreviousAsync());
                break;
            case OP_MOVE_RELATIVE: {
                const int32_t now = cur.position_now();
                if (!cur.can_seek || now < 0) return RESULT_COMMAND_CANNOT_BE_COMPLETED;
                int64_t target = std::max<int64_t>(0, (int64_t)now + param);
                if (cur.duration > 0) target = std::min<int64_t>(target, cur.duration);
                ok = seek_session(s, (int32_t)target);
                break;
            }
            default:
                return RESULT_OPCODE_NOT_SUPPORTED;
            }
        } catch (...) {
            ok = std::nullopt;
        }
        LOG_INFO("MCP: %s -> \"%s\": %s", opcode_name(op), cur.player.c_str(),
                 !ok ? "no answer" : *ok ? "done" : "refused by the player");
        return ok && *ok ? RESULT_SUCCESS : RESULT_COMMAND_CANNOT_BE_COMPLETED;
    }

    /* ---- Simulated player (dry run) ---- */

    void sim_set_track(MediaInfo &mi) {
        /* Long and not ASCII: tests cutting notifications to the MTU at a
         * character boundary and long reads (/utf-8: the literal is UTF-8) */
        mi.title = "Test track " + std::to_string(sim_track) + " ♪";
        for (int i = 0; i < 5; i++) mi.title += " Ünïcödé títlé";
        mi.track = (uint32_t)sim_track;
        mi.position = 0;
    }

    void sim_init() {
        MediaInfo mi;
        mi.session = true;
        mi.state = STATE_PAUSED;
        mi.player = SIM_PLAYER;
        mi.duration = SIM_DURATION;
        mi.can_seek = true;
        mi.updated_ms = GetTickCount64();
        sim_set_track(mi);
        publish(mi);
    }

    uint8_t execute_dry(uint8_t op, int32_t param) {
        MediaInfo mi = current();
        mi.position = mi.position_now();
        mi.updated_ms = GetTickCount64();
        switch (op) {
        case OP_PLAY:  mi.state = STATE_PLAYING; break;
        case OP_PAUSE: mi.state = STATE_PAUSED; break;
        case OP_STOP:  mi.state = STATE_PAUSED; mi.position = 0; break;
        case OP_NEXT_TRACK:
            sim_track++;
            sim_set_track(mi);
            break;
        case OP_PREVIOUS_TRACK:
            if (mi.position > 300 || sim_track == 1) mi.position = 0;
            else { sim_track--; sim_set_track(mi); }
            break;
        case OP_MOVE_RELATIVE:
            mi.position = (int32_t)std::clamp<int64_t>((int64_t)mi.position + param, 0, mi.duration);
            break;
        default:
            return RESULT_OPCODE_NOT_SUPPORTED;
        }
        if (op == OP_MOVE_RELATIVE)
            LOG_INFO("MCP: dry run: %s %d (nothing sent to Windows)", opcode_name(op), param);
        else
            LOG_INFO("MCP: dry run: %s (nothing sent to Windows)", opcode_name(op));
        publish(mi);
        return RESULT_SUCCESS;
    }

    void seek(int32_t position) {
        if (dry_run) {
            MediaInfo mi = current();
            mi.position = std::clamp(position, 0, mi.duration);
            mi.updated_ms = GetTickCount64();
            LOG_INFO("MCP: dry run: seek to %d (nothing sent to Windows)", position);
            publish(mi);
            return;
        }
        const MediaInfo cur = current();
        auto s = session();
        if (!s || !cur.can_seek) return;
        if (cur.duration > 0) position = std::min(position, cur.duration);
        LOG_INFO("MCP: seek -> \"%s\": %s", cur.player.c_str(), seek_session(s, position) ? "done" : "refused");
    }

    /* ---- Thread ---- */

    void run() {
        if (!dry_run) {
            try {
                winrt::init_apartment(winrt::apartment_type::multi_threaded);
            } catch (...) {
            }
        } else {
            LOG_INFO("MCP: dry run: simulated player, media commands are only logged");
            sim_init();
        }
        uint32_t wait_ms = 0;
        for (;;) {
            std::deque<std::function<void()>> run_now;
            {
                std::unique_lock<std::mutex> lock(m);
                cv.wait_for(lock, std::chrono::milliseconds(wait_ms), [this]() { return stopping || !tasks.empty(); });
                if (stopping) break;
                run_now.swap(tasks);
            }
            for (auto &t : run_now) t();
            if (!run_now.empty()) {
                wait_ms = POLL_AFTER_COMMAND_MS;     /* let the player react, then read it */
                continue;
            }
            poll();
            wait_ms = POLL_MS;
        }
        manager = nullptr;
        if (!dry_run) winrt::uninit_apartment();
    }

    void post(std::function<void()> fn) {
        {
            std::lock_guard<std::mutex> lock(m);
            if (stopping) return;
            tasks.push_back(std::move(fn));
        }
        cv.notify_all();
    }
};

void MediaBackend::start(bool dry_run, ChangedFn changed) {
    stop();
    dry_run_ = dry_run;
    impl_ = new Impl();
    impl_->dry_run = dry_run;
    impl_->changed = std::move(changed);
    impl_->thread = std::thread([p = impl_]() { p->run(); });
}

void MediaBackend::stop() {
    if (!impl_) return;
    {
        std::lock_guard<std::mutex> lock(impl_->m);
        impl_->stopping = true;
        impl_->tasks.clear();
    }
    impl_->cv.notify_all();
    if (impl_->thread.joinable()) impl_->thread.join();
    delete impl_;
    impl_ = nullptr;
}

MediaBackend::~MediaBackend() {
    /* Static object at process exit: do not join a thread there */
    if (impl_ && impl_->thread.joinable()) impl_->thread.detach();
}

MediaInfo MediaBackend::info() {
    if (!impl_) return MediaInfo();
    return impl_->current();
}

void MediaBackend::command(uint8_t opcode, int32_t param, DoneFn done) {
    if (!impl_) {
        if (done) done(RESULT_MEDIA_PLAYER_INACTIVE);
        return;
    }
    Impl *p = impl_;
    p->post([p, opcode, param, done]() {
        uint8_t r = p->dry_run ? p->execute_dry(opcode, param) : p->execute_windows(opcode, param);
        if (done) done(r);
    });
}

void MediaBackend::seek(int32_t position) {
    if (!impl_) return;
    Impl *p = impl_;
    p->post([p, position]() { p->seek(position); });
}

MediaBackend &media_backend() {
    static MediaBackend backend;
    return backend;
}

} /* namespace mcp */
} /* namespace le_audio */
