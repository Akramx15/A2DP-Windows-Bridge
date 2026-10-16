/*
 * LE Audio (experimental) - MCP: the Windows side of the media control server
 *
 * MediaBackend runs one thread of its own (never the BTstack thread) that
 *  - reads the current media session of Windows (System Media Transport
 *    Controls, Windows.Media.Control: the player shown in the volume flyout)
 *    about once a second: player, state, track title, duration, position;
 *  - carries out the headset's Media Control Point commands: through that
 *    session (Play / Pause / Stop / Next / Previous / seek), or as media keys
 *    (SendInput, as the AVRCP Target does for A2DP) when Windows has no media
 *    session.
 * In dry-run mode (A2DPWB --mcp-dry-run, implied by --hci-tcp) it neither
 * reads nor controls anything in Windows: a simulated player answers and the
 * commands are only logged (emulator tests).
 *
 * Built with C++/WinRT (/std:c++20 for le_audio_mcp_media.cpp only); this
 * header is plain C++17.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_MCP_MEDIA_H
#define LE_AUDIO_MCP_MEDIA_H

#include <cstdint>
#include <functional>
#include <string>

namespace le_audio {
namespace mcp {

/* Media State (MCS 3.17) */
enum : uint8_t { STATE_INACTIVE = 0x00, STATE_PLAYING = 0x01, STATE_PAUSED = 0x02, STATE_SEEKING = 0x03 };

/* Media Control Point opcodes (MCS 3.18) A2DPWB knows */
enum : uint8_t {
    OP_PLAY = 0x01,
    OP_PAUSE = 0x02,
    OP_FAST_REWIND = 0x03,
    OP_FAST_FORWARD = 0x04,
    OP_STOP = 0x05,
    OP_MOVE_RELATIVE = 0x10,
    OP_PREVIOUS_TRACK = 0x30,
    OP_NEXT_TRACK = 0x31,
};

/* Media Control Point result codes (MCS 3.18.2) */
enum : uint8_t {
    RESULT_SUCCESS = 0x01,
    RESULT_OPCODE_NOT_SUPPORTED = 0x02,
    RESULT_MEDIA_PLAYER_INACTIVE = 0x03,
    RESULT_COMMAND_CANNOT_BE_COMPLETED = 0x04,
};

const char *opcode_name(uint8_t opcode);
const char *state_name(uint8_t state);

/* What the headset sees of the Windows media player */
struct MediaInfo {
    bool session = false;       /* a media session (or the simulated player) exists */
    uint8_t state = STATE_INACTIVE;
    std::string player;         /* UTF-8 */
    std::string title;          /* UTF-8 */
    int32_t duration = -1;      /* 0.01 s, -1 unknown */
    int32_t position = -1;      /* 0.01 s at updated_ms, -1 unknown */
    uint64_t updated_ms = 0;    /* GetTickCount64() of position */
    double rate = 1.0;          /* playback rate */
    bool can_seek = false;      /* Move Relative / Track Position writes possible */
    uint32_t track = 0;         /* changes with the track (title, duration, player) */

    /* Position now (playing: advanced since updated_ms), -1 unknown */
    int32_t position_now() const;
    /* MCS Playback Speed: 64 * log2(rate), -128..127 */
    int8_t playback_speed() const;
};

class MediaBackend {
public:
    using ChangedFn = std::function<void()>;
    using DoneFn = std::function<void(uint8_t result)>;

    /* Starts the thread (dry_run: simulated player). changed() is called
     * from it whenever info() changed (other than the position advancing
     * while playing). */
    void start(bool dry_run, ChangedFn changed);
    /* Stops and joins the thread; pending commands are dropped */
    void stop();
    bool dry_run() const { return dry_run_; }

    /* Latest state (any thread) */
    MediaInfo info();

    /* Carries out a Media Control Point opcode on the backend thread (param:
     * Move Relative offset in 0.01 s) and calls done(result code) there */
    void command(uint8_t opcode, int32_t param, DoneFn done);
    /* Track Position write: seek to position (0.01 s from the start) */
    void seek(int32_t position);

    ~MediaBackend();

private:
    struct Impl;
    Impl *impl_ = nullptr;
    bool dry_run_ = false;
};

MediaBackend &media_backend();

} /* namespace mcp */
} /* namespace le_audio */

#endif /* LE_AUDIO_MCP_MEDIA_H */
