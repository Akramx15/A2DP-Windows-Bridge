/*
 * LE Audio (experimental) - CCP Call Control Server: GTBS on the PC
 *
 * GTBS (TBS 1.0, 0x184C) with all mandatory characteristics plus the
 * optional Bearer Signal Strength (+ Reporting Interval), Incoming Call
 * Target Bearer URI and Call Friendly Name:
 *
 *   0x2BB3 Bearer Provider Name            Read, Notify
 *   0x2BB4 Bearer UCI                      Read
 *   0x2BB5 Bearer Technology               Read, Notify
 *   0x2BB6 Bearer URI Schemes Supp. List   Read, Notify
 *   0x2BB7 Bearer Signal Strength          Read, Notify   (255: unavailable)
 *   0x2BB8 ... Reporting Interval          Read, Write, Write w/o Response
 *   0x2BB9 Bearer List Current Calls       Read, Notify
 *   0x2BBA Content Control ID              Read           (ccp::GTBS_CCID)
 *   0x2BBB Status Flags                    Read, Notify   (0: no in-band ringtone, not silent)
 *   0x2BBC Incoming Call Target Bearer URI Read, Notify
 *   0x2BBD Call State                      Read, Notify
 *   0x2BBE Call Control Point              Write, Write w/o Response, Notify
 *   0x2BBF Call Control Point Opt. Opcodes Read           (bit 0: Local Hold / Retrieve)
 *   0x2BC0 Termination Reason              Notify
 *   0x2BC1 Incoming Call                   Read, Notify
 *   0x2BC2 Call Friendly Name              Read, Notify
 *
 * The call model (calls_, BTstack thread) is changed by the Call Control
 * Point (the headset) and by the ccp:: events (the telephony side: today the
 * simulation of --ccp-sim / --ccp-console). Every change notifies Call State
 * and Bearer List Current Calls to the subscribed clients.
 *
 * Long values: notifications are cut to ATT_MTU - 3 (TBS: the client reads
 * the rest); a long read whose value changes between its parts gets the TBS
 * error "Value Changed During Read Long" (0x80).
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_ccp.h"
#include "le_audio_core.h"
#include "le_audio_defs.h"
#include "le_audio_gatt_server.h"
#include "debug_log.h"

#include <algorithm>
#include <cctype>
#include <cstdlib>
#include <map>
#include <sstream>
#include <thread>

#include <windows.h>
#include <shellapi.h>

extern "C" {
#include "btstack_config.h"
#include "btstack.h"
}

namespace le_audio {

namespace {

using namespace ccp;

constexpr uint16_t UUID_CHR_BEARER_PROVIDER_NAME = 0x2BB3;
constexpr uint16_t UUID_CHR_BEARER_UCI = 0x2BB4;
constexpr uint16_t UUID_CHR_BEARER_TECHNOLOGY = 0x2BB5;
constexpr uint16_t UUID_CHR_BEARER_URI_SCHEMES = 0x2BB6;
constexpr uint16_t UUID_CHR_BEARER_SIGNAL_STRENGTH = 0x2BB7;
constexpr uint16_t UUID_CHR_BEARER_SIGNAL_INTERVAL = 0x2BB8;
constexpr uint16_t UUID_CHR_BEARER_LIST_CURRENT_CALLS = 0x2BB9;
constexpr uint16_t UUID_CHR_CONTENT_CONTROL_ID = 0x2BBA;
constexpr uint16_t UUID_CHR_STATUS_FLAGS = 0x2BBB;
constexpr uint16_t UUID_CHR_INCOMING_TARGET_URI = 0x2BBC;
constexpr uint16_t UUID_CHR_CALL_STATE = 0x2BBD;
constexpr uint16_t UUID_CHR_CALL_CONTROL_POINT = 0x2BBE;
constexpr uint16_t UUID_CHR_CALL_CP_OPTIONAL_OPCODES = 0x2BBF;
constexpr uint16_t UUID_CHR_TERMINATION_REASON = 0x2BC0;
constexpr uint16_t UUID_CHR_INCOMING_CALL = 0x2BC1;
constexpr uint16_t UUID_CHR_CALL_FRIENDLY_NAME = 0x2BC2;

constexpr uint8_t SIGNAL_STRENGTH_UNAVAILABLE = 255;
/* An item of Bearer List Current Calls has a 1-byte length: 3 + URI */
constexpr size_t MAX_URI = 252;

const char *state_name(CallState s) {
    switch (s) {
    case CallState::Incoming: return "Incoming";
    case CallState::Dialing: return "Dialing";
    case CallState::Alerting: return "Alerting";
    case CallState::Active: return "Active";
    case CallState::LocallyHeld: return "Locally Held";
    case CallState::RemotelyHeld: return "Remotely Held";
    case CallState::LocallyAndRemotelyHeld: return "Locally and Remotely Held";
    }
    return "?";
}

const char *op_name(uint8_t op) {
    switch (op) {
    case OP_ACCEPT: return "Accept";
    case OP_TERMINATE: return "Terminate";
    case OP_LOCAL_HOLD: return "Local Hold";
    case OP_LOCAL_RETRIEVE: return "Local Retrieve";
    case OP_ORIGINATE: return "Originate";
    case OP_JOIN: return "Join";
    }
    return "unknown opcode";
}

const char *result_name(uint8_t r) {
    switch (r) {
    case RESULT_SUCCESS: return "Success";
    case RESULT_OPCODE_NOT_SUPPORTED: return "Opcode Not Supported";
    case RESULT_OPERATION_NOT_POSSIBLE: return "Operation Not Possible";
    case RESULT_INVALID_CALL_INDEX: return "Invalid Call Index";
    case RESULT_STATE_MISMATCH: return "State Mismatch";
    case RESULT_LACK_OF_RESOURCES: return "Lack Of Resources";
    case RESULT_INVALID_OUTGOING_URI: return "Invalid Outgoing URI";
    }
    return "?";
}

std::vector<uint8_t> bytes_of(const std::string &s) { return std::vector<uint8_t>(s.begin(), s.end()); }

std::string lower(std::string s) {
    for (char &c : s) c = (char)tolower((unsigned char)c);
    return s;
}

std::string trim(const std::string &s) {
    size_t a = s.find_first_not_of(" \t\r\n");
    if (a == std::string::npos) return {};
    size_t b = s.find_last_not_of(" \t\r\n");
    return s.substr(a, b - a + 1);
}

/* The simulation: allows every request */
class SimBackend : public Backend {};
SimBackend g_sim_backend;

class CcpModule : public Module {
public:
    const char *name() const override { return "CCP"; }

    void on_stack_init(Core &core) override;
    void add_gatt_services(GattServer &server) override;
    void on_stack_deinit() override;
    void on_disconnected(LeAudioDevice &dev, uint8_t reason) override;

    /* ---- Call model (BTstack thread) ---- */
    Backend *backend_ = &g_sim_backend;
    std::vector<Call> calls_;

    uint8_t incoming(const std::string &uri, const std::string &name, const std::string &target);
    bool alerting(uint8_t index);
    bool answered(uint8_t index);
    bool remote_hold(uint8_t index, bool held);
    bool ended(uint8_t index, uint8_t reason);
    bool simulate(const std::string &command, std::string *error);

private:
    bool built_ = false;
    uint16_t h_provider_ = 0, h_uci_ = 0, h_technology_ = 0, h_schemes_ = 0, h_signal_ = 0, h_interval_ = 0,
             h_list_ = 0, h_ccid_ = 0, h_status_ = 0, h_target_ = 0, h_state_ = 0, h_cp_ = 0, h_optional_ = 0,
             h_reason_ = 0, h_incoming_ = 0, h_friendly_ = 0;
    uint8_t signal_interval_ = 0;
    uint8_t next_index_ = 1;
    uint8_t friendly_index_ = 0;       /* the call Call Friendly Name shows */
    /* Pending notifications of one change (sent by flush()) */
    bool calls_dirty_ = false;
    std::vector<std::pair<uint8_t, uint8_t>> terminations_;   /* (call index, reason) */
    /* Long reads: value given at offset 0, per (con << 16 | handle) */
    std::map<uint32_t, std::vector<uint8_t>> long_reads_;

    /* Simulation script (--ccp-sim) and console (--ccp-console) */
    struct Step {
        uint32_t at_ms;
        std::string command;
    };
    bool args_parsed_ = false;
    std::vector<Step> script_;
    size_t script_pos_ = 0;
    bool script_armed_ = false;
    uint32_t armed_at_ms_ = 0;
    bool console_ = false;
    bool console_started_ = false;
    btstack_timer_source_t timer_{};
    bool timer_active_ = false;

    void parse_args();
    void start_timer();
    static void timer_handler(btstack_timer_source_t *ts);
    void on_timer();
    static void console_thread();

    Call *find(uint8_t index);
    Call *latest();
    uint8_t allocate_index();
    void hold_others(uint8_t except);
    void remove(uint8_t index, uint8_t reason);
    void log_calls(const char *why);

    std::vector<uint8_t> list_value() const;
    std::vector<uint8_t> state_value() const;
    std::vector<uint8_t> incoming_value() const;
    std::vector<uint8_t> target_value() const;
    std::vector<uint8_t> friendly_value() const;
    std::vector<uint8_t> serve(uint16_t con, uint16_t handle, std::vector<uint8_t> value);
    void notify_all(uint16_t handle, const std::vector<uint8_t> &value);
    void notify(uint16_t con, uint16_t handle, const std::vector<uint8_t> &value);
    void flush();

    uint8_t on_cp(uint16_t con, const uint8_t *v, uint16_t n);
    uint8_t do_accept(uint8_t index);
    uint8_t do_terminate(uint8_t index);
    uint8_t do_hold(uint8_t index);
    uint8_t do_retrieve(uint8_t index);
    uint8_t do_originate(const std::string &uri, uint8_t &index);
};

CcpModule &module_instance() {
    static CcpModule module;
    return module;
}

/* ---- Lifecycle ---- */

void CcpModule::on_stack_init(Core &core) {
    (void)core;
    built_ = false;
    calls_.clear();
    terminations_.clear();
    calls_dirty_ = false;
    long_reads_.clear();
    friendly_index_ = 0;
    timer_active_ = false;
    parse_args();
    if (console_ && !console_started_) {
        console_started_ = true;
        std::thread(&CcpModule::console_thread).detach();
        printf("CCP console: incoming <uri>[|<name>[|<target uri>]], alerting, answered, remote-hold, "
               "remote-retrieve, hangup [<index> [<reason>]], status\n");
        fflush(stdout);
    }
    if (script_pos_ < script_.size()) start_timer();
}

void CcpModule::on_stack_deinit() {
    /* The run loop has stopped: drop the state, no BTstack calls */
    built_ = false;
    timer_active_ = false;
    calls_.clear();
    terminations_.clear();
    calls_dirty_ = false;
    long_reads_.clear();
    friendly_index_ = 0;
    h_provider_ = h_uci_ = h_technology_ = h_schemes_ = h_signal_ = h_interval_ = h_list_ = h_ccid_ = h_status_ =
        h_target_ = h_state_ = h_cp_ = h_optional_ = h_reason_ = h_incoming_ = h_friendly_ = 0;
}

void CcpModule::on_disconnected(LeAudioDevice &dev, uint8_t reason) {
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

void CcpModule::add_gatt_services(GattServer &server) {
    const uint16_t RN = ATT_PROPERTY_READ | ATT_PROPERTY_NOTIFY;
    const uint8_t ENC = ATT_SECURITY_ENCRYPTED;   /* TBS: encrypted links only */
    auto reader = [this](uint16_t *handle, std::function<std::vector<uint8_t>()> make) {
        return [this, handle, make](uint16_t con) { return serve(con, *handle, make()); };
    };
    const uint16_t start = server.begin_service(UUID_SERVICE_GTBS);
    h_provider_ = server.add_characteristic(UUID_CHR_BEARER_PROVIDER_NAME, RN,
        reader(&h_provider_, [this]() { return bytes_of(backend_->provider_name()); }), nullptr, ENC, ENC);
    h_uci_ = server.add_characteristic(UUID_CHR_BEARER_UCI, ATT_PROPERTY_READ,
        reader(&h_uci_, [this]() { return bytes_of(backend_->uci()); }), nullptr, ENC, ENC);
    h_technology_ = server.add_characteristic(UUID_CHR_BEARER_TECHNOLOGY, RN,
        [this](uint16_t) { return std::vector<uint8_t>{backend_->technology()}; }, nullptr, ENC, ENC);
    h_schemes_ = server.add_characteristic(UUID_CHR_BEARER_URI_SCHEMES, RN,
        reader(&h_schemes_, [this]() { return bytes_of(backend_->uri_schemes()); }), nullptr, ENC, ENC);
    h_signal_ = server.add_characteristic(UUID_CHR_BEARER_SIGNAL_STRENGTH, RN,
        [](uint16_t) { return std::vector<uint8_t>{SIGNAL_STRENGTH_UNAVAILABLE}; }, nullptr, ENC, ENC);
    h_interval_ = server.add_characteristic(UUID_CHR_BEARER_SIGNAL_INTERVAL,
        ATT_PROPERTY_READ | ATT_PROPERTY_WRITE | ATT_PROPERTY_WRITE_WITHOUT_RESPONSE,
        [this](uint16_t) { return std::vector<uint8_t>{signal_interval_}; },
        [this](uint16_t, const uint8_t *v, uint16_t n) -> uint8_t {
            if (n != 1) return ATT_ERROR_INVALID_ATTRIBUTE_VALUE_LENGTH;
            signal_interval_ = v[0];   /* the strength never changes: nothing to report */
            return 0;
        }, ENC, ENC);
    h_list_ = server.add_characteristic(UUID_CHR_BEARER_LIST_CURRENT_CALLS, RN,
        reader(&h_list_, [this]() { return list_value(); }), nullptr, ENC, ENC);
    h_ccid_ = server.add_characteristic(UUID_CHR_CONTENT_CONTROL_ID, ATT_PROPERTY_READ,
        [](uint16_t) { return std::vector<uint8_t>{GTBS_CCID}; }, nullptr, ENC, ENC);
    h_status_ = server.add_characteristic(UUID_CHR_STATUS_FLAGS, RN,
        [](uint16_t) { return std::vector<uint8_t>{0x00, 0x00}; }, nullptr, ENC, ENC);
    h_target_ = server.add_characteristic(UUID_CHR_INCOMING_TARGET_URI, RN,
        reader(&h_target_, [this]() { return target_value(); }), nullptr, ENC, ENC);
    h_state_ = server.add_characteristic(UUID_CHR_CALL_STATE, RN,
        reader(&h_state_, [this]() { return state_value(); }), nullptr, ENC, ENC);
    h_cp_ = server.add_characteristic(UUID_CHR_CALL_CONTROL_POINT,
        ATT_PROPERTY_WRITE | ATT_PROPERTY_WRITE_WITHOUT_RESPONSE | ATT_PROPERTY_NOTIFY, nullptr,
        [this](uint16_t con, const uint8_t *v, uint16_t n) -> uint8_t { return on_cp(con, v, n); }, ENC, ENC);
    h_optional_ = server.add_characteristic(UUID_CHR_CALL_CP_OPTIONAL_OPCODES, ATT_PROPERTY_READ,
        [this](uint16_t) {
            std::vector<uint8_t> v;
            put16(v, backend_->supports_local_hold() ? 0x0001 : 0x0000);   /* bit 1 (Join): no */
            return v;
        }, nullptr, ENC, ENC);
    h_reason_ = server.add_characteristic(UUID_CHR_TERMINATION_REASON, ATT_PROPERTY_NOTIFY, nullptr, nullptr,
                                          ENC, ENC);
    h_incoming_ = server.add_characteristic(UUID_CHR_INCOMING_CALL, RN,
        reader(&h_incoming_, [this]() { return incoming_value(); }), nullptr, ENC, ENC);
    h_friendly_ = server.add_characteristic(UUID_CHR_CALL_FRIENDLY_NAME, RN,
        reader(&h_friendly_, [this]() { return friendly_value(); }), nullptr, ENC, ENC);
    uint16_t end = server.end_service();
    built_ = true;
    LOG_INFO("LE Audio: CCP: GTBS handles 0x%04x-0x%04x, CCID %u, call state 0x%04x, control point 0x%04x",
             start, end, GTBS_CCID, h_state_, h_cp_);
}

/* ---- Characteristic values ---- */

std::vector<uint8_t> CcpModule::list_value() const {
    std::vector<uint8_t> v;
    for (const Call &c : calls_) {
        v.push_back((uint8_t)(3 + c.uri.size()));
        v.push_back(c.index);
        v.push_back((uint8_t)c.state);
        v.push_back(c.flags);
        v.insert(v.end(), c.uri.begin(), c.uri.end());
    }
    return v;
}

std::vector<uint8_t> CcpModule::state_value() const {
    std::vector<uint8_t> v;
    for (const Call &c : calls_) {
        v.push_back(c.index);
        v.push_back((uint8_t)c.state);
        v.push_back(c.flags);
    }
    return v;
}

/* The latest call still ringing in, or empty */
std::vector<uint8_t> CcpModule::incoming_value() const {
    for (auto it = calls_.rbegin(); it != calls_.rend(); ++it) {
        if (it->state != CallState::Incoming) continue;
        std::vector<uint8_t> v{it->index};
        v.insert(v.end(), it->uri.begin(), it->uri.end());
        return v;
    }
    return {};
}

std::vector<uint8_t> CcpModule::target_value() const {
    for (auto it = calls_.rbegin(); it != calls_.rend(); ++it) {
        if (it->state != CallState::Incoming || it->target_uri.empty()) continue;
        std::vector<uint8_t> v{it->index};
        v.insert(v.end(), it->target_uri.begin(), it->target_uri.end());
        return v;
    }
    return {};
}

std::vector<uint8_t> CcpModule::friendly_value() const {
    for (const Call &c : calls_) {
        if (c.index != friendly_index_ || c.friendly_name.empty()) continue;
        std::vector<uint8_t> v{c.index};
        v.insert(v.end(), c.friendly_name.begin(), c.friendly_name.end());
        return v;
    }
    return {};
}

/* Read callback of a value that may be longer than one ATT PDU: remember
 * what offset 0 returned; a later part (Read Blob) of a changed value fails
 * with Value Changed During Read Long. */
std::vector<uint8_t> CcpModule::serve(uint16_t con, uint16_t handle, std::vector<uint8_t> value) {
    GattServer &server = Core::instance().gatt_server();
    const uint32_t key = ((uint32_t)con << 16) | handle;
    if (server.read_offset() == 0) {
        long_reads_[key] = value;
    } else {
        auto it = long_reads_.find(key);
        if (it != long_reads_.end() && it->second != value) {
            LOG_INFO("LE Audio: CCP: 0x%04x changed during a long read by 0x%04x", handle, con);
            server.fail_read(ATT_ERROR_VALUE_CHANGED_DURING_READ_LONG);
        }
    }
    return value;
}

/* Notifications carry at most ATT_MTU - 3 bytes (TBS: the client reads
 * the whole value) */
void CcpModule::notify(uint16_t con, uint16_t handle, const std::vector<uint8_t> &value) {
    if (!built_ || !handle) return;
    uint16_t mtu = att_server_get_mtu(con);
    size_t room = mtu > 3 ? (size_t)mtu - 3 : 20;
    if (value.size() > room)
        Core::instance().gatt_server().notify(con, handle, std::vector<uint8_t>(value.begin(), value.begin() + room));
    else
        Core::instance().gatt_server().notify(con, handle, value);
}

void CcpModule::notify_all(uint16_t handle, const std::vector<uint8_t> &value) {
    if (!built_) return;
    for (uint16_t con : Core::instance().gatt_server().clients()) notify(con, handle, value);
}

/* Send what the last change left: Termination Reason(s), then Call State and
 * Bearer List Current Calls */
void CcpModule::flush() {
    for (const auto &t : terminations_) notify_all(h_reason_, {t.first, t.second});
    terminations_.clear();
    if (calls_dirty_) {
        calls_dirty_ = false;
        notify_all(h_state_, state_value());
        notify_all(h_list_, list_value());
    }
}

/* ---- Call model ---- */

Call *CcpModule::find(uint8_t index) {
    if (index == 0) return nullptr;
    for (Call &c : calls_)
        if (c.index == index) return &c;
    return nullptr;
}

Call *CcpModule::latest() { return calls_.empty() ? nullptr : &calls_.back(); }

/* Call indices count up 1..255 and wrap, skipping those in use */
uint8_t CcpModule::allocate_index() {
    for (int i = 0; i < 255; i++) {
        uint8_t idx = next_index_;
        next_index_ = next_index_ == 255 ? 1 : (uint8_t)(next_index_ + 1);
        if (!find(idx)) return idx;
    }
    return 0;
}

/* A call becomes active: the other active calls go on hold */
void CcpModule::hold_others(uint8_t except) {
    for (Call &c : calls_) {
        if (c.index == except) continue;
        if (c.state == CallState::Active) c.state = CallState::LocallyHeld;
        else if (c.state == CallState::RemotelyHeld) c.state = CallState::LocallyAndRemotelyHeld;
    }
}

void CcpModule::remove(uint8_t index, uint8_t reason) {
    calls_.erase(std::remove_if(calls_.begin(), calls_.end(), [index](const Call &c) { return c.index == index; }),
                 calls_.end());
    if (friendly_index_ == index) friendly_index_ = 0;
    terminations_.push_back({index, reason});
    calls_dirty_ = true;
    LOG_INFO("LE Audio: CCP: call %u ended (reason 0x%02x)", index, reason);
}

void CcpModule::log_calls(const char *why) {
    std::string s;
    for (const Call &c : calls_) {
        char buf[64];
        snprintf(buf, sizeof(buf), "%s%u %s%s", s.empty() ? "" : ", ", c.index, state_name(c.state),
                 (c.flags & CALL_FLAG_OUTGOING) ? " (outgoing)" : "");
        s += buf;
    }
    LOG_INFO("LE Audio: CCP: calls after %s: %s", why, s.empty() ? "none" : s.c_str());
}

uint8_t CcpModule::incoming(const std::string &uri, const std::string &name, const std::string &target) {
    if (calls_.size() >= MAX_CALLS) {
        LOG_WARN("LE Audio: CCP: incoming call from %s refused, %u calls already", uri.c_str(), (unsigned)MAX_CALLS);
        return 0;
    }
    Call c;
    c.index = allocate_index();
    if (!c.index) return 0;
    c.state = CallState::Incoming;
    c.uri = uri.substr(0, MAX_URI);
    c.friendly_name = name.substr(0, 255);
    c.target_uri = target.substr(0, 254);
    calls_.push_back(c);
    if (!c.friendly_name.empty()) friendly_index_ = c.index;
    LOG_INFO("LE Audio: CCP: incoming call %u from %s%s%s", c.index, c.uri.c_str(),
             c.friendly_name.empty() ? "" : " ", c.friendly_name.c_str());
    /* Incoming Call Target Bearer URI, Friendly Name and Incoming Call first,
     * then Call State / the list */
    if (!c.target_uri.empty()) notify_all(h_target_, target_value());
    if (!c.friendly_name.empty()) notify_all(h_friendly_, friendly_value());
    notify_all(h_incoming_, incoming_value());
    calls_dirty_ = true;
    flush();
    log_calls("incoming call");
    return c.index;
}

bool CcpModule::alerting(uint8_t index) {
    Call *c = find(index);
    if (!c || c->state != CallState::Dialing) return false;
    c->state = CallState::Alerting;
    calls_dirty_ = true;
    flush();
    log_calls("remote alerting");
    return true;
}

bool CcpModule::answered(uint8_t index) {
    Call *c = find(index);
    if (!c || (c->state != CallState::Dialing && c->state != CallState::Alerting)) return false;
    c->state = CallState::Active;
    hold_others(index);
    calls_dirty_ = true;
    flush();
    log_calls("remote answer");
    return true;
}

bool CcpModule::remote_hold(uint8_t index, bool held) {
    Call *c = find(index);
    if (!c) return false;
    if (held) {
        if (c->state == CallState::Active) c->state = CallState::RemotelyHeld;
        else if (c->state == CallState::LocallyHeld) c->state = CallState::LocallyAndRemotelyHeld;
        else return false;
    } else {
        if (c->state == CallState::RemotelyHeld) {
            c->state = CallState::Active;
            hold_others(index);
        } else if (c->state == CallState::LocallyAndRemotelyHeld) {
            c->state = CallState::LocallyHeld;
        } else {
            return false;
        }
    }
    calls_dirty_ = true;
    flush();
    log_calls(held ? "remote hold" : "remote retrieve");
    return true;
}

bool CcpModule::ended(uint8_t index, uint8_t reason) {
    if (!find(index)) return false;
    remove(index, reason);
    flush();
    log_calls("call end");
    return true;
}

/* ---- Call Control Point (the headset) ---- */

uint8_t CcpModule::do_accept(uint8_t index) {
    Call *c = find(index);
    if (!c) return RESULT_INVALID_CALL_INDEX;
    if (c->state != CallState::Incoming) return RESULT_STATE_MISMATCH;
    const bool others_active = std::any_of(calls_.begin(), calls_.end(), [index](const Call &o) {
        return o.index != index && o.state == CallState::Active;
    });
    if (others_active && !backend_->supports_local_hold()) return RESULT_OPERATION_NOT_POSSIBLE;
    uint8_t r = backend_->accept(*c);
    if (r != RESULT_SUCCESS) return r;
    c->state = CallState::Active;
    hold_others(index);
    calls_dirty_ = true;
    return RESULT_SUCCESS;
}

uint8_t CcpModule::do_terminate(uint8_t index) {
    Call *c = find(index);
    if (!c) return RESULT_INVALID_CALL_INDEX;
    uint8_t r = backend_->terminate(*c);
    if (r != RESULT_SUCCESS) return r;
    remove(index, REASON_CLIENT_TERMINATED);
    return RESULT_SUCCESS;
}

uint8_t CcpModule::do_hold(uint8_t index) {
    Call *c = find(index);
    if (!c) return RESULT_INVALID_CALL_INDEX;
    CallState next;
    switch (c->state) {
    case CallState::Incoming:
    case CallState::Active: next = CallState::LocallyHeld; break;
    case CallState::RemotelyHeld: next = CallState::LocallyAndRemotelyHeld; break;
    default: return RESULT_STATE_MISMATCH;
    }
    uint8_t r = backend_->hold(*c);
    if (r != RESULT_SUCCESS) return r;
    c->state = next;
    calls_dirty_ = true;
    return RESULT_SUCCESS;
}

uint8_t CcpModule::do_retrieve(uint8_t index) {
    Call *c = find(index);
    if (!c) return RESULT_INVALID_CALL_INDEX;
    CallState next;
    switch (c->state) {
    case CallState::LocallyHeld: next = CallState::Active; break;
    case CallState::LocallyAndRemotelyHeld: next = CallState::RemotelyHeld; break;
    default: return RESULT_STATE_MISMATCH;
    }
    uint8_t r = backend_->retrieve(*c);
    if (r != RESULT_SUCCESS) return r;
    c->state = next;
    if (next == CallState::Active) hold_others(index);
    calls_dirty_ = true;
    return RESULT_SUCCESS;
}

uint8_t CcpModule::do_originate(const std::string &uri, uint8_t &index) {
    /* "<scheme>:<something>" with a scheme of the Bearer URI Schemes Supported List */
    size_t colon = uri.find(':');
    if (colon == std::string::npos || colon == 0 || colon + 1 >= uri.size() || uri.size() > MAX_URI)
        return RESULT_INVALID_OUTGOING_URI;
    const std::string scheme = lower(uri.substr(0, colon));
    bool known = false;
    std::stringstream list(backend_->uri_schemes());
    for (std::string s; std::getline(list, s, ',');)
        known |= lower(trim(s)) == scheme;
    if (!known) return RESULT_INVALID_OUTGOING_URI;
    for (unsigned char ch : uri)
        if (ch < 0x20) return RESULT_INVALID_OUTGOING_URI;
    if (calls_.size() >= MAX_CALLS) return RESULT_LACK_OF_RESOURCES;
    const bool others_active = std::any_of(calls_.begin(), calls_.end(),
                                           [](const Call &o) { return o.state == CallState::Active; });
    if (others_active && !backend_->supports_local_hold()) return RESULT_OPERATION_NOT_POSSIBLE;
    uint8_t r = backend_->originate(uri);
    if (r != RESULT_SUCCESS) return r;
    Call c;
    c.index = allocate_index();
    if (!c.index) return RESULT_LACK_OF_RESOURCES;
    c.state = CallState::Dialing;
    c.flags = CALL_FLAG_OUTGOING;
    c.uri = uri;
    hold_others(c.index);
    calls_.push_back(c);
    index = c.index;
    calls_dirty_ = true;
    return RESULT_SUCCESS;
}

uint8_t CcpModule::on_cp(uint16_t con, const uint8_t *v, uint16_t n) {
    if (n < 1) return ATT_ERROR_INVALID_ATTRIBUTE_VALUE_LENGTH;
    const uint8_t op = v[0];
    uint8_t index = 0;
    uint8_t result;
    switch (op) {
    case OP_ACCEPT:
    case OP_TERMINATE:
    case OP_LOCAL_HOLD:
    case OP_LOCAL_RETRIEVE:
        if (n != 2) return ATT_ERROR_INVALID_ATTRIBUTE_VALUE_LENGTH;
        index = v[1];
        if ((op == OP_LOCAL_HOLD || op == OP_LOCAL_RETRIEVE) && !backend_->supports_local_hold())
            result = RESULT_OPCODE_NOT_SUPPORTED;
        else if (op == OP_ACCEPT) result = do_accept(index);
        else if (op == OP_TERMINATE) result = do_terminate(index);
        else if (op == OP_LOCAL_HOLD) result = do_hold(index);
        else result = do_retrieve(index);
        break;
    case OP_ORIGINATE:
        if (n < 2) return ATT_ERROR_INVALID_ATTRIBUTE_VALUE_LENGTH;
        result = do_originate(std::string((const char *)v + 1, n - 1u), index);
        break;
    default:
        /* Join (optional, not supported) and unknown opcodes */
        result = RESULT_OPCODE_NOT_SUPPORTED;
        break;
    }
    LOG_INFO("LE Audio: CCP: control point %s (0x%02x) call %u from 0x%04x -> %s", op_name(op), op, index, con,
             result_name(result));
    /* The result to the writer first, then what changed */
    notify(con, h_cp_, {op, index, result});
    flush();
    if (result == RESULT_SUCCESS) log_calls(op_name(op));
    return 0;
}

/* ---- Simulation ---- */

bool CcpModule::simulate(const std::string &command, std::string *error) {
    auto fail = [error](const std::string &why) {
        if (error) *error = why;
        return false;
    };
    std::string line = trim(command);
    if (line.empty()) return fail("empty command");
    size_t sp = line.find_first_of(" \t");
    const std::string cmd = lower(line.substr(0, sp));
    const std::string rest = sp == std::string::npos ? std::string() : trim(line.substr(sp + 1));
    LOG_INFO("LE Audio: CCP: simulate \"%s\"", line.c_str());

    if (cmd == "incoming") {
        if (rest.empty()) return fail("incoming needs a URI");
        std::vector<std::string> parts;
        std::stringstream ss(rest);
        for (std::string p; std::getline(ss, p, '|');) parts.push_back(trim(p));
        parts.resize(3);
        return incoming(parts[0], parts[1], parts[2]) ? true : fail("no room for another call");
    }
    if (cmd == "status") {
        log_calls("status");
        return true;
    }
    /* [<index> [<reason>]] */
    std::stringstream ss(rest);
    std::string a, b;
    ss >> a >> b;
    Call *last = latest();
    uint8_t index = !a.empty() ? (uint8_t)strtoul(a.c_str(), nullptr, 0) : (last ? last->index : 0);
    if (!find(index)) return fail("no such call");
    bool ok;
    if (cmd == "alerting") ok = alerting(index);
    else if (cmd == "answered") ok = answered(index);
    else if (cmd == "remote-hold") ok = remote_hold(index, true);
    else if (cmd == "remote-retrieve") ok = remote_hold(index, false);
    else if (cmd == "hangup") ok = ended(index, b.empty() ? REASON_REMOTE_ENDED : (uint8_t)strtoul(b.c_str(), nullptr, 0));
    else return fail("unknown command");
    return ok ? true : fail("not possible in the call's state");
}

/* --ccp-sim / --ccp-console from the process command line */
void CcpModule::parse_args() {
    if (args_parsed_) return;
    args_parsed_ = true;
    int argc = 0;
    LPWSTR *argv = CommandLineToArgvW(GetCommandLineW(), &argc);
    if (!argv) return;
    auto utf8 = [](const wchar_t *w) {
        int len = WideCharToMultiByte(CP_UTF8, 0, w, -1, nullptr, 0, nullptr, nullptr);
        std::string s(len > 0 ? (size_t)len - 1 : 0, '\0');
        if (len > 1) WideCharToMultiByte(CP_UTF8, 0, w, -1, &s[0], len, nullptr, nullptr);
        return s;
    };
    for (int i = 1; i < argc; i++) {
        std::string arg = utf8(argv[i]);
        if (arg == "--ccp-console") {
            console_ = true;
        } else if (arg == "--ccp-sim" && i + 1 < argc) {
            std::stringstream ss(utf8(argv[++i]));
            for (std::string step; std::getline(ss, step, ';');) {
                step = trim(step);
                size_t colon = step.find(':');
                if (step.empty()) continue;
                if (colon == std::string::npos) {
                    LOG_WARN("LE Audio: CCP: --ccp-sim step \"%s\" has no time (<seconds>:<command>)", step.c_str());
                    continue;
                }
                double t = atof(step.substr(0, colon).c_str());
                script_.push_back({(uint32_t)(t * 1000.0), trim(step.substr(colon + 1))});
            }
            std::stable_sort(script_.begin(), script_.end(),
                             [](const Step &x, const Step &y) { return x.at_ms < y.at_ms; });
        }
    }
    LocalFree(argv);
    if (!script_.empty())
        LOG_INFO("LE Audio: CCP: simulation script of %u step(s), starts when a client subscribes to Call State",
                 (unsigned)script_.size());
}

void CcpModule::start_timer() {
    if (timer_active_) return;
    timer_active_ = true;
    btstack_run_loop_set_timer_handler(&timer_, &CcpModule::timer_handler);
    btstack_run_loop_set_timer(&timer_, 100);
    btstack_run_loop_add_timer(&timer_);
}

void CcpModule::timer_handler(btstack_timer_source_t *ts) {
    (void)ts;
    CcpModule &m = module_instance();
    m.timer_active_ = false;
    m.on_timer();
}

void CcpModule::on_timer() {
    if (!built_) {
        start_timer();   /* the GATT server is not up yet */
        return;
    }
    const uint32_t now = btstack_run_loop_get_time_ms();
    GattServer &server = Core::instance().gatt_server();
    if (!script_armed_) {
        for (uint16_t con : server.clients()) {
            if (server.client_config(con, h_state_) & GATT_CLIENT_CHARACTERISTICS_CONFIGURATION_NOTIFICATION) {
                script_armed_ = true;
                armed_at_ms_ = now;
                LOG_INFO("LE Audio: CCP: client 0x%04x subscribed to Call State, simulation starts", con);
                break;
            }
        }
    }
    while (script_armed_ && script_pos_ < script_.size() && now - armed_at_ms_ >= script_[script_pos_].at_ms) {
        std::string error;
        if (!simulate(script_[script_pos_].command, &error))
            LOG_WARN("LE Audio: CCP: simulation \"%s\" failed: %s", script_[script_pos_].command.c_str(),
                     error.c_str());
        script_pos_++;
    }
    if (script_pos_ < script_.size()) start_timer();
}

/* --ccp-console: lines from stdin to the BTstack thread. ReadFile, not the
 * CRT: a thread blocked in fgets() would hold the stdin lock at exit(). */
void CcpModule::console_thread() {
    HANDLE in = GetStdHandle(STD_INPUT_HANDLE);
    std::string buffer;
    char chunk[256];
    DWORD got = 0;
    while (in && in != INVALID_HANDLE_VALUE && ReadFile(in, chunk, sizeof(chunk), &got, nullptr) && got > 0) {
        buffer.append(chunk, got);
        size_t nl;
        while ((nl = buffer.find('\n')) != std::string::npos) {
            std::string line = trim(buffer.substr(0, nl));
            buffer.erase(0, nl + 1);
            if (line.empty()) continue;
            Core::instance().post([line]() {
                std::string error;
                if (!module_instance().simulate(line, &error)) {
                    printf("CCP: %s: %s\n", line.c_str(), error.c_str());
                    fflush(stdout);
                }
            });
        }
    }
}

} /* namespace */

Module *ccp_module() { return &module_instance(); }

namespace ccp {

void set_backend(Backend *backend) { module_instance().backend_ = backend ? backend : &g_sim_backend; }

uint8_t incoming_call(const std::string &uri, const std::string &friendly_name, const std::string &target_uri) {
    return module_instance().incoming(uri, friendly_name, target_uri);
}

bool remote_alerting(uint8_t index) { return module_instance().alerting(index); }
bool remote_answered(uint8_t index) { return module_instance().answered(index); }
bool remote_hold(uint8_t index, bool held) { return module_instance().remote_hold(index, held); }
bool call_ended(uint8_t index, uint8_t reason) { return module_instance().ended(index, reason); }
std::vector<Call> calls() { return module_instance().calls_; }

bool simulate(const std::string &command, std::string *error) {
    return module_instance().simulate(command, error);
}

} /* namespace ccp */

} /* namespace le_audio */
