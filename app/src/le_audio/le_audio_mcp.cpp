/*
 * LE Audio (experimental) - MCP: Media Control Server (GMCS) on the PC
 *
 * The PC is the Media Control Server; the headset (Media Control Client)
 * reads the player state from the Generic Media Control Service and sends
 * play / pause / stop / next / previous / move relative through the Media
 * Control Point.
 *
 * GMCS (0x1849, all characteristics need an encrypted link):
 *   Media Player Name (0x2B93)   read, notify   Windows player ("Windows" without one)
 *   Track Changed (0x2B96)       notify         new track
 *   Track Title (0x2B97)         read, notify   UTF-8, "" when unknown
 *   Track Duration (0x2B98)      read, notify   int32 0.01 s, -1 unknown
 *   Track Position (0x2B99)      read, write, write w/o response, notify
 *   Playback Speed (0x2B9A)      read, notify   64 * log2(rate)
 *   Seeking Speed (0x2B9B)       read, notify   always 0 (no seeking state)
 *   Media State (0x2BA3)         read, notify   inactive / playing / paused
 *   Media Control Point (0x2BA4) write, write w/o response, notify (opcode, result)
 *   Opcodes Supported (0x2BA5)   read, notify   Play, Pause, Stop, Previous / Next
 *                                               Track, + Move Relative if seekable
 *   Content Control ID (0x2BBA)  read           CCID_GMCS, also in the CCID List
 *                                               of a media stream's Enable metadata
 * Long values: reads with offsets are served by GattServer (Read Blob); a
 * long read of the player name or track title whose value changes between
 * its parts fails with "Value Changed During Read Long" (ATT 0x80, an MCS
 * application error); notifications are cut to ATT_MTU - 3 at a UTF-8
 * character boundary.
 *
 * The Windows side (player state from the System Media Transport Controls,
 * commands through them or as media keys) is in le_audio_mcp_media.cpp, on a
 * thread of its own. Commands are refused (Command Cannot Be Completed) while
 * the "media keys" setting is off, as AVRCP buttons are ignored then.
 * --mcp-dry-run (implied by --hci-tcp, the emulator transport) replaces
 * Windows by a simulated player and only logs the commands.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_mcp.h"
#include "le_audio_mcp_media.h"
#include "le_audio_core.h"
#include "le_audio_gatt_server.h"
#include "btstack_transport.h"
#include "debug_log.h"

#include <shellapi.h>

#include <algorithm>
#include <cstdlib>
#include <map>

extern "C" {
#include "btstack_config.h"
#include "btstack.h"
}

namespace le_audio {

namespace {

using namespace mcp;

constexpr uint16_t CHR_MEDIA_PLAYER_NAME = 0x2B93;
constexpr uint16_t CHR_TRACK_CHANGED = 0x2B96;
constexpr uint16_t CHR_TRACK_TITLE = 0x2B97;
constexpr uint16_t CHR_TRACK_DURATION = 0x2B98;
constexpr uint16_t CHR_TRACK_POSITION = 0x2B99;
constexpr uint16_t CHR_PLAYBACK_SPEED = 0x2B9A;
constexpr uint16_t CHR_SEEKING_SPEED = 0x2B9B;
constexpr uint16_t CHR_MEDIA_STATE = 0x2BA3;
constexpr uint16_t CHR_MEDIA_CONTROL_POINT = 0x2BA4;
constexpr uint16_t CHR_OPCODES_SUPPORTED = 0x2BA5;
constexpr uint16_t CHR_CONTENT_CONTROL_ID = 0x2BBA;

constexpr size_t MAX_TEXT = 512;               /* longest attribute value (ATT) */
constexpr uint8_t ATT_ERROR_VALUE_CHANGED_DURING_READ_LONG = 0x80;   /* MCS application error */
constexpr int32_t POSITION_JUMP = 200;         /* 2 s off the expected position: notify */

/* Media Control Point Opcodes Supported bit of an opcode (MCS 3.19) */
uint32_t opcode_bit(uint8_t op) {
    switch (op) {
    case OP_PLAY:           return 1u << 0;
    case OP_PAUSE:          return 1u << 1;
    case OP_FAST_REWIND:    return 1u << 2;
    case OP_FAST_FORWARD:   return 1u << 3;
    case OP_STOP:           return 1u << 4;
    case OP_MOVE_RELATIVE:  return 1u << 5;
    case OP_PREVIOUS_TRACK: return 1u << 11;
    case OP_NEXT_TRACK:     return 1u << 12;
    default:                return 0;
    }
}

const char *result_name(uint8_t r) {
    switch (r) {
    case RESULT_SUCCESS:                     return "Success";
    case RESULT_OPCODE_NOT_SUPPORTED:        return "Opcode Not Supported";
    case RESULT_MEDIA_PLAYER_INACTIVE:       return "Media Player Inactive";
    case RESULT_COMMAND_CANNOT_BE_COMPLETED: return "Command Cannot Be Completed";
    default:                                 return "?";
    }
}

/* UTF-8 text cut to at most max bytes without splitting a character */
std::vector<uint8_t> text_value(const std::string &s, size_t max) {
    size_t n = std::min(s.size(), max);
    if (n < s.size())
        while (n > 0 && ((uint8_t)s[n] & 0xC0) == 0x80) n--;
    return std::vector<uint8_t>(s.begin(), s.begin() + n);
}

std::vector<uint8_t> le32(int32_t v) {
    std::vector<uint8_t> out;
    put32(out, (uint32_t)v);
    return out;
}

/* A flag on A2DPWB's command line (main.cpp ignores options it does not know) */
bool command_line_has(const wchar_t *flag) {
    int argc = 0;
    LPWSTR *argv = CommandLineToArgvW(GetCommandLineW(), &argc);
    if (!argv) return false;
    bool found = false;
    for (int i = 1; i < argc && !found; i++) found = wcscmp(argv[i], flag) == 0;
    LocalFree(argv);
    return found;
}

class McpModule : public Module {
public:
    const char *name() const override { return "MCP"; }

    void on_stack_init(Core &core) override {
        (void)core;
        dry_run_ = command_line_has(L"--mcp-dry-run") || command_line_has(L"--hci-tcp");
        last_ = MediaInfo();
        media_backend().start(dry_run_, [this]() {
            Core::instance().post([this]() { publish(); });
        });
    }

    void on_stack_deinit() override {
        media_backend().stop();
        last_ = MediaInfo();
        long_reads_.clear();
    }

    void on_disconnected(LeAudioDevice &dev, uint8_t reason) override {
        (void)dev;
        (void)reason;
        /* dev.acl is already invalid: keep the long reads of the links still up */
        std::vector<uint16_t> up;
        for (LeAudioDevice *d : Core::instance().all_devices())
            if (d->acl != CON_HANDLE_INVALID) up.push_back(d->acl);
        for (auto it = long_reads_.begin(); it != long_reads_.end();) {
            const uint16_t con = (uint16_t)(it->first >> 16);
            it = std::find(up.begin(), up.end(), con) == up.end() ? long_reads_.erase(it) : std::next(it);
        }
    }

    void add_gatt_services(GattServer &server) override {
        const uint8_t enc = ATT_SECURITY_ENCRYPTED;
        server.begin_service(UUID_SERVICE_GMCS);
        h_name_ = server.add_characteristic(CHR_MEDIA_PLAYER_NAME, ATT_PROPERTY_READ | ATT_PROPERTY_NOTIFY,
            [this](uint16_t con) { return serve(con, h_name_, name_value(media_backend().info(), MAX_TEXT)); },
            nullptr, enc, enc);
        h_track_changed_ = server.add_characteristic(CHR_TRACK_CHANGED, ATT_PROPERTY_NOTIFY,
            nullptr, nullptr, enc, enc);
        h_title_ = server.add_characteristic(CHR_TRACK_TITLE, ATT_PROPERTY_READ | ATT_PROPERTY_NOTIFY,
            [this](uint16_t con) { return serve(con, h_title_, text_value(media_backend().info().title, MAX_TEXT)); },
            nullptr, enc, enc);
        h_duration_ = server.add_characteristic(CHR_TRACK_DURATION, ATT_PROPERTY_READ | ATT_PROPERTY_NOTIFY,
            [](uint16_t) { return le32(media_backend().info().duration); }, nullptr, enc, enc);
        h_position_ = server.add_characteristic(CHR_TRACK_POSITION,
            ATT_PROPERTY_READ | ATT_PROPERTY_WRITE | ATT_PROPERTY_WRITE_WITHOUT_RESPONSE | ATT_PROPERTY_NOTIFY,
            [](uint16_t) { return le32(media_backend().info().position_now()); },
            [this](uint16_t con, const uint8_t *v, uint16_t n) { return on_position_write(con, v, n); }, enc, enc);
        h_speed_ = server.add_characteristic(CHR_PLAYBACK_SPEED, ATT_PROPERTY_READ | ATT_PROPERTY_NOTIFY,
            [](uint16_t) { return std::vector<uint8_t>{(uint8_t)media_backend().info().playback_speed()}; },
            nullptr, enc, enc);
        server.add_characteristic(CHR_SEEKING_SPEED, ATT_PROPERTY_READ | ATT_PROPERTY_NOTIFY,
            [](uint16_t) { return std::vector<uint8_t>{0}; }, nullptr, enc, enc);
        h_state_ = server.add_characteristic(CHR_MEDIA_STATE, ATT_PROPERTY_READ | ATT_PROPERTY_NOTIFY,
            [](uint16_t) { return std::vector<uint8_t>{media_backend().info().state}; }, nullptr, enc, enc);
        h_cp_ = server.add_characteristic(CHR_MEDIA_CONTROL_POINT,
            ATT_PROPERTY_WRITE | ATT_PROPERTY_WRITE_WITHOUT_RESPONSE | ATT_PROPERTY_NOTIFY, nullptr,
            [this](uint16_t con, const uint8_t *v, uint16_t n) { return on_control_point(con, v, n); }, enc, enc);
        h_opcodes_ = server.add_characteristic(CHR_OPCODES_SUPPORTED, ATT_PROPERTY_READ | ATT_PROPERTY_NOTIFY,
            [](uint16_t) { return le32((int32_t)opcodes(media_backend().info())); }, nullptr, enc, enc);
        server.add_characteristic(CHR_CONTENT_CONTROL_ID, ATT_PROPERTY_READ,
            [](uint16_t) { return std::vector<uint8_t>{CCID_GMCS}; }, nullptr, enc, enc);
        server.end_service();
        LOG_INFO("MCP: GMCS on the PC (CCID %u)%s", CCID_GMCS,
                 dry_run_ ? ", dry run: media commands are only logged" : "");
    }

    void add_ccids(uint16_t contexts, std::vector<uint8_t> &ccids) override {
        /* Every stream but a call is the PC's playback the player controls */
        if (contexts & ~CONTEXT_CONVERSATIONAL) ccids.push_back(CCID_GMCS);
    }

private:
    bool dry_run_ = false;
    uint16_t h_name_ = 0, h_track_changed_ = 0, h_title_ = 0, h_duration_ = 0, h_position_ = 0;
    uint16_t h_speed_ = 0, h_state_ = 0, h_cp_ = 0, h_opcodes_ = 0;
    MediaInfo last_;                       /* BTstack thread: what clients were told */
    /* Long reads: value given at offset 0, per (con << 16 | handle) */
    std::map<uint32_t, std::vector<uint8_t>> long_reads_;

    /* Read callback of a value that may be longer than one ATT PDU: remember
     * what offset 0 returned; a later part (Read Blob) of a changed value
     * fails with Value Changed During Read Long */
    std::vector<uint8_t> serve(uint16_t con, uint16_t handle, std::vector<uint8_t> value) {
        GattServer &server = Core::instance().gatt_server();
        const uint32_t key = ((uint32_t)con << 16) | handle;
        if (server.read_offset() == 0) {
            long_reads_[key] = value;
        } else {
            auto it = long_reads_.find(key);
            if (it != long_reads_.end() && it->second != value) {
                LOG_INFO("MCP: 0x%04x changed during a long read by 0x%04x", handle, con);
                server.fail_read(ATT_ERROR_VALUE_CHANGED_DURING_READ_LONG);
            }
        }
        return value;
    }

    static uint32_t opcodes(const MediaInfo &mi) {
        uint32_t ops = opcode_bit(OP_PLAY) | opcode_bit(OP_PAUSE) | opcode_bit(OP_STOP) |
                       opcode_bit(OP_PREVIOUS_TRACK) | opcode_bit(OP_NEXT_TRACK);
        if (mi.session && mi.can_seek) ops |= opcode_bit(OP_MOVE_RELATIVE);
        return ops;
    }

    static std::vector<uint8_t> name_value(const MediaInfo &mi, size_t max) {
        return text_value(mi.session && !mi.player.empty() ? mi.player : std::string("Windows"), max);
    }

    /* Notification to every client that enabled it; text is cut to its MTU */
    void send(uint16_t handle, const std::vector<uint8_t> &value, bool text = false) {
        GattServer &server = Core::instance().gatt_server();
        for (uint16_t con : server.clients()) {
            const uint16_t mtu = att_server_get_mtu(con);
            const size_t room = mtu > 3 ? mtu - 3u : 20u;
            if (value.size() <= room)
                server.notify(con, handle, value);
            else if (text)
                server.notify(con, handle, text_value(std::string(value.begin(), value.end()), room));
            else
                server.notify(con, handle, std::vector<uint8_t>(value.begin(), value.begin() + room));
        }
    }

    /* BTstack thread: tell the clients what changed since the last call */
    void publish() {
        const MediaInfo mi = media_backend().info();
        const MediaInfo &o = last_;
        const bool track = mi.track != o.track || mi.session != o.session;
        if (mi.session != o.session || mi.player != o.player) send(h_name_, name_value(mi, MAX_TEXT), true);
        if (track) send(h_track_changed_, {});
        if (track || mi.title != o.title) send(h_title_, text_value(mi.title, MAX_TEXT), true);
        if (track || mi.duration != o.duration) send(h_duration_, le32(mi.duration));
        if (mi.state != o.state) send(h_state_, {mi.state});
        const int32_t p = mi.position_now(), q = o.position_now();
        if (track || mi.state != o.state || (p < 0) != (q < 0) || std::abs(p - q) > POSITION_JUMP)
            send(h_position_, le32(p));
        if (mi.playback_speed() != o.playback_speed()) send(h_speed_, {(uint8_t)mi.playback_speed()});
        if (opcodes(mi) != opcodes(o)) send(h_opcodes_, le32((int32_t)opcodes(mi)));
        last_ = mi;
    }

    void reply(uint16_t con, uint8_t op, uint8_t result) {
        /* A client without control point notifications gets no result (MCS:
         * the write is still carried out) */
        const bool sent = Core::instance().gatt_server().notify(con, h_cp_, {op, result});
        LOG_INFO("MCP: control point %s (0x%02x) -> %s%s", opcode_name(op), op, result_name(result),
                 sent ? "" : " (not notified: the client did not enable it)");
    }

    /* Result notification after the ATT write response: post() runs it in a
     * later run loop turn */
    void reply_later(uint16_t con, uint8_t op, uint8_t result) {
        Core::instance().post([this, con, op, result]() { reply(con, op, result); });
    }

    uint8_t on_control_point(uint16_t con, const uint8_t *v, uint16_t n) {
        if (n < 1) return ATT_ERROR_INVALID_ATTRIBUTE_VALUE_LENGTH;
        /* Carried out even if the client did not enable control point
         * notifications; only the result notification is left out (reply()) */
        const uint8_t op = v[0];
        const MediaInfo mi = media_backend().info();
        LOG_INFO("MCP: control point %s (0x%02x) from 0x%04x", opcode_name(op), op, con);
        if (!(opcodes(mi) & opcode_bit(op))) {
            /* Move Relative is there while a seekable player is */
            const bool inactive = op == OP_MOVE_RELATIVE && (!mi.session || mi.state == STATE_INACTIVE);
            reply_later(con, op, inactive ? RESULT_MEDIA_PLAYER_INACTIVE : RESULT_OPCODE_NOT_SUPPORTED);
            return 0;
        }
        if (n != (op == OP_MOVE_RELATIVE ? 5 : 1)) {
            reply_later(con, op, RESULT_COMMAND_CANNOT_BE_COMPLETED);
            return 0;
        }
        if (!dry_run_ && !BtStackTransport::media_keys_enabled()) {
            LOG_INFO("MCP: %s ignored (media keys off)", opcode_name(op));
            reply_later(con, op, RESULT_COMMAND_CANNOT_BE_COMPLETED);
            return 0;
        }
        const int32_t param = op == OP_MOVE_RELATIVE ? (int32_t)rd32(v + 1) : 0;
        media_backend().command(op, param, [this, con, op](uint8_t result) {
            reply_later(con, op, result);    /* from the backend thread */
        });
        return 0;
    }

    uint8_t on_position_write(uint16_t con, const uint8_t *v, uint16_t n) {
        (void)con;
        if (n != 4) return ATT_ERROR_INVALID_ATTRIBUTE_VALUE_LENGTH;
        const MediaInfo mi = media_backend().info();
        if (!mi.session || mi.state == STATE_INACTIVE || !mi.can_seek) return 0;   /* ignored */
        if (!dry_run_ && !BtStackTransport::media_keys_enabled()) return 0;
        int32_t p = (int32_t)rd32(v);
        if (p < 0) p = mi.duration > 0 ? std::max(0, mi.duration + p) : 0;   /* negative: from the end */
        LOG_INFO("MCP: Track Position write %d", p);
        media_backend().seek(p);
        return 0;
    }
};

} /* namespace */

Module *mcp_module() {
    static McpModule module;
    return &module;
}

} /* namespace le_audio */
