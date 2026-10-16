/*
 * LE Audio (experimental) - core: LE scan, connection, security, streaming
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_core.h"
#include "le_audio_bap.h"
#include "le_audio_gatt_client.h"
#include "le_audio_module.h"
#include "debug_log.h"

#include <chrono>
#include <condition_variable>
#include <thread>
#include <windows.h>

extern "C" {
#include "btstack_config.h"
#include "btstack.h"
#include "btstack_tlv_windows.h"
#include "ble/le_device_db_tlv.h"
}

namespace le_audio {

namespace {

btstack_packet_callback_registration_t g_hci_reg;
btstack_packet_callback_registration_t g_sm_reg;

void hci_trampoline(uint8_t packet_type, uint16_t channel, uint8_t *packet, uint16_t size) {
    (void)channel;
    if (packet_type == HCI_EVENT_PACKET) Core::instance().on_hci_event(packet, size);
}

void sm_trampoline(uint8_t packet_type, uint16_t channel, uint8_t *packet, uint16_t size) {
    (void)channel;
    if (packet_type == HCI_EVENT_PACKET) Core::instance().on_sm_event(packet, size);
}

void iso_trampoline(uint8_t packet_type, uint16_t channel, uint8_t *packet, uint16_t size) {
    (void)channel;
    Core::instance().on_iso_packet(packet_type, packet, size);
}

/* Addresses resolved / looked up are used for connecting at most this long
 * after they were seen (RPAs change every ~15 min); at most this many kept */
constexpr uint32_t RPA_FRESH_MS = 5 * 60 * 1000;
constexpr size_t RPA_CACHE_MAX = 512;

std::string hex_status(const char *what, uint8_t status) {
    char buf[96];
    snprintf(buf, sizeof(buf), "%s (0x%02x)", what, status);
    return buf;
}

} /* namespace */

Core &Core::instance() {
    static Core core;
    return core;
}

Core::Core() {}

/* A function for the BTstack thread (btstack_run_loop_execute_on_main_thread) */
struct Core::PostedFn {
    btstack_context_callback_registration_t reg;
    std::function<void()> fn;
};

void Core::run_posted(void *context) {
    auto *p = static_cast<PostedFn *>(context);
    Core &c = instance();
    std::function<void()> fn;
    {
        std::lock_guard<std::mutex> lock(c.posted_mutex_);
        auto it = c.posted_.find(p);
        /* Not ours any more: of an ended session, freed by stack_init() */
        if (it == c.posted_.end()) return;
        c.posted_.erase(it);
        fn = std::move(p->fn);
    }
    delete p;
    if (fn) fn();
}

void Core::set_storage_dir(const std::string &dir) {
    storage_dir_ = dir;
}

/* ======================================================================== */
/* Stack session                                                             */
/* ======================================================================== */

void Core::stack_init() {
    if (!enabled_.load() || initialized_.load()) return;
    LOG_INFO("LE Audio (experimental): initializing (SM, GATT client / server, ISO)");
    {
        /* Callbacks the previous session left queued: the run loop was
         * initialized again since, so nothing refers to them any more */
        std::lock_guard<std::mutex> lock(posted_mutex_);
        for (PostedFn *p : posted_dead_) delete p;
        posted_dead_.clear();
        for (PostedFn *p : posted_) delete p;
        posted_.clear();
        posts_open_ = true;
    }

    /* LE bonds: le_device_db in a TLV file next to the classic link keys */
    std::string dir = storage_dir_;
    if (dir.empty()) {
        char tmp[MAX_PATH];
        DWORD n = GetTempPathA(MAX_PATH, tmp);
        dir = n ? std::string(tmp, n) : std::string(".");
        if (!dir.empty() && (dir.back() == '\\' || dir.back() == '/')) dir.pop_back();
    }
    std::string path = dir + "\\le_audio_bonds.tlv";
    auto *ctx = new btstack_tlv_windows_t();
    memset(ctx, 0, sizeof(*ctx));
    const btstack_tlv_t *tlv = btstack_tlv_windows_init_instance(ctx, path.c_str());
    tlv_context_ = ctx;
    btstack_tlv_set_instance(tlv, ctx);
    le_device_db_tlv_configure(tlv, ctx);
    LOG_INFO("LE Audio: bonds in %s", path.c_str());

    /* SM: LE Secure Connections, Just Works, bonding */
    sm_init();
    sm_set_io_capabilities(IO_CAPABILITY_NO_INPUT_NO_OUTPUT);
    sm_set_authentication_requirements(SM_AUTHREQ_SECURE_CONNECTION | SM_AUTHREQ_BONDING);
    g_sm_reg.callback = &sm_trampoline;
    sm_add_event_handler(&g_sm_reg);

    gatt_client_init();
    gatt_client::init();

    g_hci_reg.callback = &hci_trampoline;
    hci_add_event_handler(&g_hci_reg);
    hci_register_iso_packet_handler(&iso_trampoline);

    devices_.clear();
    scan_results_.clear();
    scanning_ = false;
    connecting_ = -1;
    connect_id_.reset();
    connect_done_ = nullptr;
    rpa_cache_.clear();
    iso_len_.store(0);
    iso_count_.store(0);
    unicast_ = std::make_unique<Unicast>(*this);

    for (Module *m : modules()) m->on_stack_init(*this);

    server_.reset();
    server_.build_base();
    for (Module *m : modules()) m->add_gatt_services(server_);
    server_.start();

    initialized_.store(true);
}

void Core::stack_deinit() {
    if (!initialized_.load()) return;
    {
        /* The run loop has stopped: callbacks still queued never run in this
         * session. Drop their functions (and what they captured) now; the
         * registrations stay allocated until the run loop is set up again. */
        std::vector<std::function<void()>> dropped;
        {
            std::lock_guard<std::mutex> lock(posted_mutex_);
            posts_open_ = false;
            for (PostedFn *p : posted_) {
                dropped.push_back(std::move(p->fn));
                p->fn = nullptr;
                posted_dead_.push_back(p);
            }
            posted_.clear();
        }
        if (!dropped.empty()) LOG_INFO("LE Audio: %u queued call(s) dropped", (unsigned)dropped.size());
    }
    for (Module *m : modules()) m->on_stack_deinit();
    stream_.stop_sending();
    stream_.clear();
    if (connect_done_) {
        auto f = std::move(connect_done_);
        connect_done_ = nullptr;
        connect_error_ = "Bluetooth stopped";
        f(false);
    }
    unicast_.reset();
    gatt_client::deinit();
    att_server_deinit();
    sm_deinit();
    btstack_tlv_set_instance(nullptr, nullptr);
    if (tlv_context_) {
        auto *ctx = static_cast<btstack_tlv_windows_t *>(tlv_context_);
        btstack_tlv_windows_deinit(ctx);
        delete ctx;
        tlv_context_ = nullptr;
    }
    devices_.clear();
    scan_results_.clear();
    rpa_cache_.clear();
    connecting_ = -1;
    connect_id_.reset();
    {
        std::lock_guard<std::mutex> lock(status_mutex_);
        status_ = StreamStatus();
    }
    hci_working_.store(false);
    initialized_.store(false);
    LOG_INFO("LE Audio: stopped");
}

/* ======================================================================== */
/* Thread helpers                                                            */
/* ======================================================================== */

void Core::post(std::function<void()> fn) {
    std::lock_guard<std::mutex> lock(posted_mutex_);
    if (!posts_open_) return;
    auto *p = new PostedFn();
    p->fn = std::move(fn);
    p->reg.callback = &Core::run_posted;
    p->reg.context = p;
    posted_.insert(p);
    btstack_run_loop_execute_on_main_thread(&p->reg);
}

bool Core::call(std::function<void(std::function<void(bool)>)> fn, uint32_t timeout_ms,
                const Cancelled &cancelled) {
    if (!initialized_.load()) return false;
    struct Sync {
        std::mutex m;
        std::condition_variable cv;
        bool done = false;
        bool ok = false;
    };
    auto s = std::make_shared<Sync>();
    post([s, fn]() {
        fn([s](bool ok) {
            std::lock_guard<std::mutex> lock(s->m);
            if (s->done) return;
            s->done = true;
            s->ok = ok;
            s->cv.notify_all();
        });
    });
    auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout_ms);
    std::unique_lock<std::mutex> lock(s->m);
    while (!s->done) {
        if (cancelled && cancelled()) return false;
        if (std::chrono::steady_clock::now() >= deadline) return false;
        s->cv.wait_for(lock, std::chrono::milliseconds(50));
    }
    return s->ok;
}

bool Core::run(std::function<void()> fn, uint32_t timeout_ms) {
    return call([fn](std::function<void(bool)> done) { fn(); done(true); }, timeout_ms);
}

/* ======================================================================== */
/* Devices                                                                   */
/* ======================================================================== */

LeAudioDevice *Core::device(int id) {
    if (id < 0 || id >= (int)devices_.size()) return nullptr;
    return devices_[id].get();
}

LeAudioDevice *Core::device_by_handle(uint16_t acl) {
    if (acl == CON_HANDLE_INVALID) return nullptr;
    for (auto &d : devices_)
        if (d->acl == acl) return d.get();
    return nullptr;
}

std::vector<LeAudioDevice *> Core::all_devices() {
    std::vector<LeAudioDevice *> out;
    for (auto &d : devices_) out.push_back(d.get());
    return out;
}

LeAudioDevice *Core::device_for_addr(const LeAddr &addr, bool create) {
    /* An RPA SM resolved stands for its identity */
    LeAddr key = addr;
    LeAddr identity;
    bool resolved = resolved_identity(addr.addr, identity);
    if (resolved) key = identity;
    for (auto &d : devices_)
        if (d->addr.same_bytes(key.addr) || d->peer_addr.same_bytes(addr.addr)) {
            if (!d->identity_known && !resolved) d->addr.type = addr.type;
            return d.get();
        }
    if (!create) return nullptr;
    auto d = std::make_unique<LeAudioDevice>();
    d->id = (int)devices_.size();
    d->addr = key;
    d->identity_known = resolved;
    d->peer_addr = addr;
    devices_.push_back(std::move(d));
    return devices_.back().get();
}

/* ======================================================================== */
/* Resolvable private addresses                                              */
/* ======================================================================== */

uint64_t Core::addr_key(const uint8_t addr[6]) {
    uint64_t k = 0;
    for (int i = 0; i < 6; i++) k = (k << 8) | addr[i];
    return k;
}

void Core::lookup_rpa(const LeAddr &addr, bool seen) {
    if (!addr.is_rpa()) return;
    uint64_t k = addr_key(addr.addr);
    auto it = rpa_cache_.find(k);
    if (it == rpa_cache_.end()) {
        if (rpa_cache_.size() >= RPA_CACHE_MAX) {
            /* Forget the addresses that resolved to nothing */
            for (auto e = rpa_cache_.begin(); e != rpa_cache_.end();)
                e = e->second.done && !e->second.ok ? rpa_cache_.erase(e) : std::next(e);
            if (rpa_cache_.size() >= RPA_CACHE_MAX) return;
        }
        it = rpa_cache_.emplace(k, RpaEntry()).first;
        /* SM checks it against the IRKs of all bonds (software AES); the
         * answer is SM_EVENT_IDENTITY_RESOLVING_SUCCEEDED / _FAILED */
        bd_addr_t a;
        memcpy(a, addr.addr, 6);
        int status = sm_address_resolution_lookup(BD_ADDR_TYPE_LE_RANDOM, a);
        if (status != 0 && status != BTSTACK_BUSY) it->second.done = true;
    }
    if (seen) it->second.seen_ms = btstack_run_loop_get_time_ms();
}

bool Core::resolved_identity(const uint8_t addr[6], LeAddr &identity) const {
    auto it = rpa_cache_.find(addr_key(addr));
    if (it == rpa_cache_.end() || !it->second.ok) return false;
    identity = it->second.identity;
    return true;
}

LeAddr Core::connect_address(const LeAudioDevice &dev, const LeAddr &given) const {
    /* The RPA the device advertised most recently (scan() / find() just
     * before): a saved RPA or the identity is not what it answers to now */
    const RpaEntry *best = nullptr;
    uint64_t best_key = 0;
    uint32_t now = btstack_run_loop_get_time_ms();
    for (const auto &e : rpa_cache_) {
        const RpaEntry &r = e.second;
        if (!r.ok || !r.seen_ms || now - r.seen_ms > RPA_FRESH_MS || !r.identity.same_bytes(dev.addr.addr)) continue;
        if (!best || (int32_t)(r.seen_ms - best->seen_ms) > 0) {
            best = &r;
            best_key = e.first;
        }
    }
    if (!best) return given;
    LeAddr a;
    for (int i = 5; i >= 0; i--, best_key >>= 8) a.addr[i] = (uint8_t)best_key;
    a.type = BD_ADDR_TYPE_LE_RANDOM;
    return a;
}

void Core::on_identity(LeAudioDevice &dev, const LeAddr &identity) {
    if (dev.identity_known && dev.addr == identity) return;
    LeAudioDevice *other = nullptr;
    for (auto &d : devices_)
        if (d.get() != &dev && d->addr.same_bytes(identity.addr)) other = d.get();
    if (!other || other->connected() || dev.secured) {
        if (!(dev.addr == identity))
            LOG_INFO("LE Audio: %s has the identity address %s (type %u)", dev.addr.str().c_str(),
                     identity.str().c_str(), identity.type);
        dev.addr = identity;
        dev.identity_known = true;
        return;
    }
    /* A device known under its identity came back with an address that was
     * not resolved yet: the link belongs to the known device (its id, which
     * CSIP and the worker hold, survives). Nothing but on_connected() has
     * run for the link yet (security is not done). */
    LOG_INFO("LE Audio: %s is %s (device %d): link moved to it", dev.peer_addr.str().c_str(),
             identity.str().c_str(), other->id);
    other->addr = identity;
    other->identity_known = true;
    other->peer_addr = dev.peer_addr;
    other->acl = dev.acl;
    other->encrypted = dev.encrypted;
    other->pairing = dev.pairing;
    other->pair_attempts = dev.pair_attempts;
    other->secured = false;
    other->gatt_ready = false;
    other->ready = false;
    other->last_error = 0;
    other->services.clear();
    if (other->name.empty()) other->name = dev.name;
    dev.acl = CON_HANDLE_INVALID;
    dev.encrypted = false;
    dev.pairing = false;
    dev.secured = false;
    dev.gatt_ready = false;
    dev.ready = false;
    if (connecting_ == dev.id) {
        connecting_ = other->id;
        if (connect_id_) *connect_id_ = other->id;
    }
    for (Module *m : modules()) m->on_connected(*other);
}

std::vector<DeviceInfo> Core::devices() {
    auto out = std::make_shared<std::vector<DeviceInfo>>();
    run([this, out]() {
        for (auto &d : devices_) {
            DeviceInfo i;
            i.id = d->id;
            i.addr = d->addr;
            i.name = d->name;
            i.connected = d->connected();
            i.ready = d->ready;
            i.sink_locations = d->sink_locations;
            i.sink_ases = d->sink_ase_count();
            for (const auto &r : d->sink_pacs) {
                if (!r.lc3) continue;
                char buf[64];
                snprintf(buf, sizeof(buf), "LC3 rates 0x%04x ch 0x%02x octets %u-%u",
                         r.caps.sampling_frequencies, r.caps.channel_counts, r.caps.min_octets, r.caps.max_octets);
                if (!i.pacs.empty()) i.pacs += "; ";
                i.pacs += buf;
            }
            out->push_back(i);
        }
    });
    return *out;
}

/* ======================================================================== */
/* Scanning                                                                  */
/* ======================================================================== */

namespace {

/* Fill the parsed fields of r from AD data */
void parse_ad(ScanResult &r, const uint8_t *data, uint8_t len) {
    ad_context_t it;
    for (ad_iterator_init(&it, len, data); ad_iterator_has_more(&it); ad_iterator_next(&it)) {
        uint8_t type = ad_iterator_get_data_type(&it);
        uint8_t n = ad_iterator_get_data_len(&it);
        const uint8_t *v = ad_iterator_get_data(&it);
        auto add_uuid = [&r](uint16_t u) {
            for (uint16_t x : r.uuids16) if (x == u) return;
            r.uuids16.push_back(u);
        };
        switch (type) {
        case AD_NAME_SHORT:
        case AD_NAME_COMPLETE:
            if (type == AD_NAME_COMPLETE || r.name.empty()) r.name.assign((const char *)v, n);
            break;
        case AD_APPEARANCE:
            if (n >= 2) r.appearance = rd16(v);
            break;
        case AD_UUID16_INCOMPLETE:
        case AD_UUID16_COMPLETE:
            for (uint8_t i = 0; i + 1 < n; i += 2) add_uuid(rd16(v + i));
            break;
        case AD_SERVICE_DATA_UUID16:
            if (n >= 2) {
                uint16_t u = rd16(v);
                add_uuid(u);
                if (u == UUID_SERVICE_ASCS && n >= 5) {
                    r.targeted = v[2] == 0x01;
                    r.ascs_sink_contexts = rd16(v + 3);
                } else if (u == UUID_SERVICE_CAS && n >= 3) {
                    r.targeted |= v[2] == 0x01;
                }
            }
            break;
        case AD_RSI:
            if (n >= 6) r.rsi.assign(v, v + 6);
            break;
        default:
            break;
        }
    }
    for (uint16_t u : r.uuids16)
        if (u == UUID_SERVICE_ASCS || u == UUID_SERVICE_PACS || u == UUID_SERVICE_CAS ||
            u == UUID_SERVICE_TMAS || u == UUID_SERVICE_GMAS)
            r.le_audio = true;
    /* No LE Audio UUID, but an audio appearance (Audio Sink 0x0840-0x087F,
     * Wearable Audio Device 0x0940-0x097F, e.g. earbuds): list it as a
     * candidate; connecting tells (no ASCS: "no sink ASE") */
    uint16_t category = (uint16_t)(r.appearance >> 6);
    if (!r.le_audio && r.connectable && (category == 0x021 || category == 0x025)) {
        r.le_audio = true;
        if (r.note.empty()) r.note = "audio appearance, LE Audio not advertised";
    }
}

} /* namespace */

void Core::on_adv_report(const LeAddr &addr, int8_t rssi, uint8_t event_type, bool extended,
                         const uint8_t *data, uint8_t len) {
    if (!scanning_) return;
    ScanResult *r = nullptr;
    for (auto &e : scan_results_)
        if (memcmp(e.addr.addr, addr.addr, 6) == 0 && e.addr.type == addr.type) r = &e;
    bool fresh = r == nullptr;
    if (!r) {
        scan_results_.push_back(ScanResult());
        r = &scan_results_.back();
        r->addr = addr;
    }
    lookup_rpa(addr, true);
    if (!r->resolved) r->resolved = resolved_identity(addr.addr, r->identity);
    bool scan_rsp = extended ? (event_type & 0x08) != 0 : event_type == 4;
    bool connectable = extended ? (event_type & 0x01) != 0 : (event_type == 0 || event_type == 1);
    if (scan_rsp) r->scan_rsp.assign(data, data + len);
    else {
        r->adv.assign(data, data + len);
        r->connectable = connectable;
    }
    r->rssi = rssi;
    bool was_le_audio = r->le_audio;
    parse_ad(*r, r->adv.data(), (uint8_t)r->adv.size());
    parse_ad(*r, r->scan_rsp.data(), (uint8_t)r->scan_rsp.size());
    for (Module *m : modules()) m->on_scan_result(*r, data, len);
    if ((fresh || !was_le_audio) && r->le_audio)
        LOG_INFO("LE Audio: found %s (%s, type %u) RSSI %d%s%s%s", r->name.empty() ? "?" : r->name.c_str(),
                 r->addr.str().c_str(), r->addr.type, r->rssi,
                 r->resolved ? (", identity " + r->identity.str()).c_str() : "",
                 r->rsi.empty() ? "" : ", set member (RSI)", r->note.empty() ? "" : (", " + r->note).c_str());
}

std::vector<ScanResult> Core::scan(uint32_t duration_ms, const Cancelled &cancelled) {
    auto out = std::make_shared<std::vector<ScanResult>>();
    if (!ready()) return *out;
    LOG_INFO("LE Audio: scanning for %u ms", duration_ms);
    run([this]() {
        scan_results_.clear();
        scanning_ = true;
        gap_set_scan_params(1 /* active */, 0x0030, 0x0030, 0);
        gap_start_scan();
    });
    auto end = std::chrono::steady_clock::now() + std::chrono::milliseconds(duration_ms);
    while (std::chrono::steady_clock::now() < end && !(cancelled && cancelled()))
        std::this_thread::sleep_for(std::chrono::milliseconds(50));
    run([this, out]() {
        gap_stop_scan();
        scanning_ = false;
        *out = scan_results_;
    });
    return *out;
}

bool Core::find(const uint8_t addr[6], uint32_t timeout_ms, ScanResult &result, const Cancelled &cancelled) {
    if (!ready()) return false;
    LeAddr want;
    memcpy(want.addr, addr, 6);
    want.type = BD_ADDR_TYPE_LE_RANDOM;
    run([this, want]() {
        /* A saved RPA of a bonded device: look for its identity */
        lookup_rpa(want, false);
        scan_results_.clear();
        scanning_ = true;
        gap_set_scan_params(1, 0x0030, 0x0030, 0);
        gap_start_scan();
    });
    auto found = std::make_shared<ScanResult>();
    auto ok = std::make_shared<bool>(false);
    auto end = std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout_ms);
    while (!*ok && std::chrono::steady_clock::now() < end && !(cancelled && cancelled())) {
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
        run([this, want, found, ok]() {
            LeAddr want_identity;
            bool want_resolved = resolved_identity(want.addr, want_identity);
            for (auto &r : scan_results_) {
                if (!r.resolved) r.resolved = resolved_identity(r.addr.addr, r.identity);
                bool match = r.addr.same_bytes(want.addr) ||
                             (r.resolved && (r.identity.same_bytes(want.addr) ||
                                             (want_resolved && r.identity.same_bytes(want_identity.addr))));
                if (match && (!r.name.empty() || !r.scan_rsp.empty() || r.le_audio)) {
                    if (!r.addr.same_bytes(want.addr))
                        LOG_INFO("LE Audio: %s advertises as %s now (identity %s)", want.str().c_str(),
                                 r.addr.str().c_str(), r.identity.str().c_str());
                    *found = r;
                    *ok = true;
                }
            }
        });
    }
    run([this]() {
        gap_stop_scan();
        scanning_ = false;
    });
    if (*ok) result = *found;
    return *ok;
}

/* ======================================================================== */
/* Connection                                                                */
/* ======================================================================== */

int Core::connect(const LeAddr &addr, const std::string &name, uint32_t timeout_ms, std::string *error,
                  const Cancelled &cancelled) {
    struct Res {
        int id = -1;
        std::string error;
    };
    auto res = std::make_shared<Res>();
    if (!ready()) {
        if (error) *error = "LE Audio is not running";
        return -1;
    }
    LOG_INFO("LE Audio: connecting to %s (address type %u)", addr.str().c_str(), addr.type);
    bool ok = call([this, addr, name, res](std::function<void(bool)> done) {
        if (connecting_ >= 0) {
            res->error = "another LE connection is being set up";
            done(false);
            return;
        }
        LeAudioDevice *d = device_for_addr(addr, true);
        if (!name.empty()) d->name = name;
        res->id = d->id;
        LeAddr target = connect_address(*d, addr);
        auto finish = [this, res, done](bool ok) {
            if (!ok) res->error = connect_error_;
            done(ok);
        };
        if (d->connected() && d->ready) {
            done(true);
            return;
        }
        connect_error_.clear();
        connecting_ = d->id;
        connect_id_ = std::shared_ptr<int>(res, &res->id);
        connect_done_ = finish;
        if (d->connected()) return;   /* set up in progress: wait for it */
        d->pair_attempts = 0;
        d->last_error = 0;
        if (!(target == addr) || !(d->addr == addr))
            LOG_INFO("LE Audio: device %d (%s): connecting with %s (address type %u)", d->id, d->addr.str().c_str(),
                     target.str().c_str(), target.type);
        connect_target_ = target;
        uint8_t status = gap_connect(target.addr, (bd_addr_type_t)target.type);
        if (status != ERROR_CODE_SUCCESS) {
            connecting_ = -1;
            connect_id_.reset();
            connect_done_ = nullptr;
            res->error = hex_status("LE connect could not start", status);
            done(false);
        }
    }, timeout_ms, cancelled);

    if (!ok) {
        if (res->error.empty())
            res->error = (cancelled && cancelled()) ? "cancelled" : "no answer (timeout)";
        LOG_WARN("LE Audio: connecting to %s failed: %s", addr.str().c_str(), res->error.c_str());
        /* Abandon the attempt: cancel the connect or drop the link. A link
         * that comes up after this is dropped by on_le_connected(). */
        run([this, res]() {
            const int id = res->id;   /* read here: on_identity() may move it */
            if (connecting_ == id) {
                connecting_ = -1;
                connect_id_.reset();
                connect_done_ = nullptr;
            }
            LeAudioDevice *d = device(id);
            if (d && d->connected()) gap_disconnect(d->acl);
            else gap_connect_cancel();
        });
        if (error) *error = res->error;
        return -1;
    }
    LOG_INFO("LE Audio: %s ready (device %d)", addr.str().c_str(), res->id);
    return res->id;
}

void Core::disconnect(int device_id, uint32_t timeout_ms) {
    call([this, device_id](std::function<void(bool)> done) {
        LeAudioDevice *d = device(device_id);
        if (!d || !d->connected()) { done(true); return; }
        if (gap_disconnect(d->acl) != ERROR_CODE_SUCCESS) { done(false); return; }
        /* Disconnection Complete arrives later; poll for it */
        auto *timer = new btstack_timer_source_t();
        struct Ctx { Core *core; int id; std::function<void(bool)> done; int ticks; };
        auto *ctx = new Ctx{this, device_id, done, 0};
        btstack_run_loop_set_timer_context(timer, ctx);
        btstack_run_loop_set_timer_handler(timer, [](btstack_timer_source_t *t) {
            auto *c = static_cast<Ctx *>(btstack_run_loop_get_timer_context(t));
            LeAudioDevice *dev = c->core->device(c->id);
            if (!dev || !dev->connected() || ++c->ticks > 100) {
                c->done(true);
                delete c;
                delete t;
                return;
            }
            btstack_run_loop_set_timer(t, 20);
            btstack_run_loop_add_timer(t);
        });
        btstack_run_loop_set_timer(timer, 20);
        btstack_run_loop_add_timer(timer);
    }, timeout_ms);
}

void Core::disconnect_all(uint32_t timeout_ms) {
    for (const auto &d : devices())
        if (d.connected) disconnect(d.id, timeout_ms);
}

void Core::on_le_connected(uint8_t status, uint16_t handle, const LeAddr &addr, uint8_t role) {
    LeAudioDevice *d = connecting_ >= 0 ? device(connecting_) : nullptr;
    bool stray = !d || (status == ERROR_CODE_SUCCESS && (d->connected() || !connect_target_.same_bytes(addr.addr)));
    if (stray) {
        /* A2DPWB never advertises: this is the connect of an attempt given
         * up (timeout / cancel) that completed before the cancel did. Nobody
         * manages the link: drop it. */
        if (status == ERROR_CODE_SUCCESS) {
            LOG_WARN("LE Audio: unexpected LE connection 0x%04x with %s (role %u): disconnecting", handle,
                     addr.str().c_str(), role);
            gap_disconnect(handle);
        }
        return;
    }
    if (status != ERROR_CODE_SUCCESS) {
        fail_connect(*d, hex_status(status == ERROR_CODE_UNKNOWN_CONNECTION_IDENTIFIER
                                        ? "LE connection cancelled" : "LE connection failed", status));
        return;
    }
    d->acl = handle;
    d->peer_addr = addr;
    d->encrypted = false;
    d->pairing = false;
    d->secured = false;
    d->gatt_ready = false;
    d->ready = false;
    d->services.clear();
    LOG_INFO("LE Audio: LE link to %s up (handle 0x%04x), securing it", d->addr.str().c_str(), handle);
    gatt_client::attach(handle);
    for (Module *m : modules()) m->on_connected(*d);
    sm_request_pairing(handle);
}

void Core::fail_connect(LeAudioDevice &dev, const std::string &why) {
    connect_error_ = why;
    LOG_WARN("LE Audio: %s: %s", dev.addr.str().c_str(), why.c_str());
    if (dev.connected()) gap_disconnect(dev.acl);
    finish_connect(dev, false);
}

void Core::finish_connect(LeAudioDevice &dev, bool ok) {
    if (connecting_ != dev.id) return;
    connecting_ = -1;
    connect_id_.reset();
    auto f = std::move(connect_done_);
    connect_done_ = nullptr;
    if (f) f(ok);
}

void Core::on_encryption(uint16_t handle, uint8_t status, bool enabled) {
    LeAudioDevice *d = device_by_handle(handle);
    if (!d) return;
    d->encrypted = status == ERROR_CODE_SUCCESS && enabled;
    LOG_INFO("LE Audio: %s encryption %s (0x%02x)", d->addr.str().c_str(), d->encrypted ? "on" : "off", status);
    maybe_secured(*d);
}

void Core::maybe_secured(LeAudioDevice &dev) {
    if (!dev.encrypted || dev.pairing || dev.secured || !dev.connected()) return;
    dev.secured = true;
    for (Module *m : modules()) m->on_encrypted(dev);
    const int id = dev.id;
    const uint16_t con = dev.acl;
    gatt_client::discover_all(con, [this, id, con](uint8_t status, std::vector<GattService> services) {
        LeAudioDevice *d = device(id);
        if (!d || d->acl != con) return;
        if (status != 0) {
            fail_connect(*d, hex_status("GATT discovery failed", status));
            return;
        }
        d->services = std::move(services);
        size_t chars = 0;
        for (const auto &s : d->services) chars += s.chars.size();
        LOG_INFO("LE Audio: %s: %u services, %u characteristics", d->addr.str().c_str(),
                 (unsigned)d->services.size(), (unsigned)chars);
        bap::read_device(*this, *d, [this, id, con](bool) {
            LeAudioDevice *x = device(id);
            if (!x || x->acl != con) return;
            x->gatt_ready = true;
            for (Module *m : modules()) m->on_gatt_ready(*x);
            x->ready = true;
            finish_connect(*x, true);
        });
    });
}

void Core::on_disconnected(uint16_t handle, uint8_t reason) {
    LeAudioDevice *d = device_by_handle(handle);
    if (d) {
        LOG_INFO("LE Audio: %s disconnected (0x%02x)", d->addr.str().c_str(), reason);
        gatt_client::detach(handle);
        server_.on_disconnected(handle);
        d->acl = CON_HANDLE_INVALID;
        d->encrypted = false;
        d->pairing = false;
        d->secured = false;
        d->gatt_ready = false;
        d->ready = false;
        d->last_error = reason;
        for (auto &a : d->ases) a.state = AseState::Unknown;
        for (Module *m : modules()) m->on_disconnected(*d, reason);
        if (connecting_ == d->id) {
            if (connect_error_.empty()) connect_error_ = hex_status("the link went down during setup", reason);
            finish_connect(*d, false);
        }
    }
    if (unicast_) unicast_->on_link_lost(handle);
}

/* ======================================================================== */
/* Events                                                                    */
/* ======================================================================== */

void Core::on_hci_event(uint8_t *packet, uint16_t size) {
    switch (hci_event_packet_get_type(packet)) {
    case BTSTACK_EVENT_STATE:
        if (btstack_event_state_get_state(packet) == HCI_STATE_WORKING) {
            hci_working_.store(true);
            LOG_INFO("LE Audio: ready (controller ISO buffers: %u x %u bytes)", iso_count_.load(), iso_len_.load());
            if (iso_count_.load() == 0 || iso_len_.load() == 0)
                LOG_WARN("LE Audio: the controller reports no ISO buffers: it does not support LE Audio "
                         "streams (CIS); streaming will be refused");
        } else {
            hci_working_.store(false);
        }
        break;

    case HCI_EVENT_COMMAND_COMPLETE: {
        uint16_t op = hci_event_command_complete_get_command_opcode(packet);
        if (op == HCI_OPCODE_HCI_LE_READ_BUFFER_SIZE_V2 && size >= 12 && packet[5] == 0) {
            iso_len_.store(little_endian_read_16(packet, 9));
            iso_count_.store(packet[11]);
        } else if (op == HCI_OPCODE_HCI_LE_REMOVE_CIG) {
            if (unicast_) unicast_->on_cig_removed();
        }
        break;
    }

    case GAP_EVENT_ADVERTISING_REPORT: {
        LeAddr a;
        gap_event_advertising_report_get_address(packet, a.addr);
        a.type = gap_event_advertising_report_get_address_type(packet);
        on_adv_report(a, (int8_t)gap_event_advertising_report_get_rssi(packet),
                      gap_event_advertising_report_get_advertising_event_type(packet), false,
                      gap_event_advertising_report_get_data(packet),
                      gap_event_advertising_report_get_data_length(packet));
        break;
    }
    case GAP_EVENT_EXTENDED_ADVERTISING_REPORT: {
        LeAddr a;
        gap_event_extended_advertising_report_get_address(packet, a.addr);
        a.type = gap_event_extended_advertising_report_get_address_type(packet) & 0x01;
        on_adv_report(a, (int8_t)gap_event_extended_advertising_report_get_rssi(packet),
                      (uint8_t)gap_event_extended_advertising_report_get_advertising_event_type(packet), true,
                      gap_event_extended_advertising_report_get_data(packet),
                      gap_event_extended_advertising_report_get_data_length(packet));
        break;
    }

    case HCI_EVENT_META_GAP:
        switch (hci_event_gap_meta_get_subevent_code(packet)) {
        case GAP_SUBEVENT_LE_CONNECTION_COMPLETE: {
            LeAddr a;
            gap_subevent_le_connection_complete_get_peer_address(packet, a.addr);
            a.type = gap_subevent_le_connection_complete_get_peer_address_type(packet);
            on_le_connected(gap_subevent_le_connection_complete_get_status(packet),
                            gap_subevent_le_connection_complete_get_connection_handle(packet), a,
                            gap_subevent_le_connection_complete_get_role(packet));
            break;
        }
        case GAP_SUBEVENT_CIG_CREATED:
            if (unicast_) unicast_->on_cig_created(packet, size);
            break;
        case GAP_SUBEVENT_CIS_CREATED:
            if (unicast_) unicast_->on_cis_created(packet, size);
            break;
        default:
            break;
        }
        break;

    case HCI_EVENT_ENCRYPTION_CHANGE:
        on_encryption(hci_event_encryption_change_get_connection_handle(packet),
                      hci_event_encryption_change_get_status(packet),
                      hci_event_encryption_change_get_encryption_enabled(packet) != 0);
        break;
    case HCI_EVENT_ENCRYPTION_CHANGE_V2:
        on_encryption(hci_event_encryption_change_v2_get_connection_handle(packet),
                      hci_event_encryption_change_v2_get_status(packet),
                      hci_event_encryption_change_v2_get_encryption_enabled(packet) != 0);
        break;

    case HCI_EVENT_DISCONNECTION_COMPLETE:
        on_disconnected(hci_event_disconnection_complete_get_connection_handle(packet),
                        hci_event_disconnection_complete_get_reason(packet));
        break;

    default:
        break;
    }
}

void Core::on_sm_event(uint8_t *packet, uint16_t size) {
    (void)size;
    uint8_t type = hci_event_packet_get_type(packet);
    if (type < SM_EVENT_JUST_WORKS_REQUEST || type > SM_EVENT_REENCRYPTION_COMPLETE) return;
    uint16_t handle = little_endian_read_16(packet, 2);
    switch (type) {
    case SM_EVENT_IDENTITY_RESOLVING_SUCCEEDED:
    case SM_EVENT_IDENTITY_CREATED: {
        /* An address resolved with a bond's IRK (an RPA seen while scanning,
         * or a link), or the identity a new pairing distributed */
        LeAddr a, identity;
        bool created = type == SM_EVENT_IDENTITY_CREATED;
        if (created) {
            a.type = sm_event_identity_created_get_addr_type(packet);
            sm_event_identity_created_get_address(packet, a.addr);
            identity.type = sm_event_identity_created_get_identity_addr_type(packet);
            sm_event_identity_created_get_identity_address(packet, identity.addr);
        } else {
            a.type = sm_event_identity_resolving_succeeded_get_addr_type(packet);
            sm_event_identity_resolving_succeeded_get_address(packet, a.addr);
            identity.type = sm_event_identity_resolving_succeeded_get_identity_addr_type(packet);
            sm_event_identity_resolving_succeeded_get_identity_address(packet, identity.addr);
        }
        identity.type &= 0x01;
        if (created) {
            /* A new IRK: addresses that resolved to nothing may resolve now */
            for (auto e = rpa_cache_.begin(); e != rpa_cache_.end();)
                e = e->second.done && !e->second.ok ? rpa_cache_.erase(e) : std::next(e);
        }
        if (a.is_rpa()) {
            RpaEntry &e = rpa_cache_[addr_key(a.addr)];
            if (!e.ok) LOG_INFO("LE Audio: %s resolves to %s", a.str().c_str(), identity.str().c_str());
            e.done = e.ok = true;
            e.identity = identity;
            for (auto &r : scan_results_)
                if (r.addr.same_bytes(a.addr)) {
                    r.resolved = true;
                    r.identity = identity;
                }
        }
        if (handle != HCI_CON_HANDLE_INVALID)
            if (LeAudioDevice *x = device_by_handle(handle)) on_identity(*x, identity);
        return;
    }
    case SM_EVENT_IDENTITY_RESOLVING_FAILED: {
        bd_addr_t a;
        sm_event_identity_resolving_failed_get_address(packet, a);
        auto it = rpa_cache_.find(addr_key(a));
        if (it != rpa_cache_.end() && !it->second.ok) it->second.done = true;
        return;
    }
    default:
        break;
    }
    LeAudioDevice *d = device_by_handle(handle);
    if (!d) return;
    switch (type) {
    case SM_EVENT_JUST_WORKS_REQUEST:
        LOG_INFO("LE Audio: %s: pairing (Just Works)", d->addr.str().c_str());
        sm_just_works_confirm(handle);
        break;
    case SM_EVENT_NUMERIC_COMPARISON_REQUEST:
        LOG_INFO("LE Audio: %s: pairing (numeric comparison, accepted)", d->addr.str().c_str());
        sm_numeric_comparison_confirm(handle);
        break;
    case SM_EVENT_PASSKEY_INPUT_NUMBER:
        /* No keyboard: such a device cannot pair with A2DPWB */
        LOG_WARN("LE Audio: %s asks for a passkey, which A2DPWB cannot enter", d->addr.str().c_str());
        sm_bonding_decline(handle);
        break;
    case SM_EVENT_PAIRING_STARTED:
        d->pairing = true;
        break;
    case SM_EVENT_PAIRING_COMPLETE: {
        d->pairing = false;
        uint8_t status = sm_event_pairing_complete_get_status(packet);
        if (status == ERROR_CODE_SUCCESS) {
            LOG_INFO("LE Audio: %s paired and bonded", d->addr.str().c_str());
            maybe_secured(*d);
        } else if (++d->pair_attempts < 2) {
            LOG_WARN("LE Audio: %s: pairing failed (0x%02x, reason 0x%02x), trying again", d->addr.str().c_str(),
                     status, sm_event_pairing_complete_get_reason(packet));
            sm_request_pairing(handle);
        } else {
            fail_connect(*d, hex_status("pairing failed", status));
        }
        break;
    }
    case SM_EVENT_REENCRYPTION_COMPLETE: {
        uint8_t status = sm_event_reencryption_complete_get_status(packet);
        if (status != ERROR_CODE_SUCCESS) {
            /* The device lost its bond (reset or paired elsewhere): pair anew */
            LOG_WARN("LE Audio: %s: encryption with the stored key failed (0x%02x), pairing again",
                     d->addr.str().c_str(), status);
            gap_delete_bonding((bd_addr_type_t)d->addr.type, d->addr.addr);
            if (++d->pair_attempts < 2 && d->connected()) sm_request_pairing(handle);
            else fail_connect(*d, hex_status("encryption failed", status));
        }
        break;
    }
    default:
        break;
    }
}

void Core::on_iso_packet(uint8_t packet_type, uint8_t *packet, uint16_t size) {
    if (packet_type == HCI_EVENT_PACKET && size >= 2 && packet[0] == HCI_EVENT_CIS_CAN_SEND_NOW)
        stream_.on_cis_can_send_now(packet, size);
    /* Incoming ISO data (source ASEs) is not used yet */
}

void Core::on_ase_update(LeAudioDevice &dev, const Ase &ase) {
    if (unicast_) unicast_->on_ase_update(dev, ase);
}

void Core::on_cp_response(LeAudioDevice &dev, const uint8_t *value, uint16_t len) {
    if (unicast_) unicast_->on_cp_response(dev, value, len);
}

/* ======================================================================== */
/* Streaming                                                                 */
/* ======================================================================== */

void Core::notify_stream_state(bool streaming) {
    StreamStatus s;
    {
        std::lock_guard<std::mutex> lock(status_mutex_);
        status_.active = streaming;
        if (unicast_) {
            status_.plan = unicast_->plan();
            status_.summary = status_.plan.describe();
        }
        s = status_;
    }
    for (Module *m : modules()) m->on_stream_state(s, streaming);
}

void Core::set_stream_lost(const char *why) {
    if (!stream_lost_.exchange(true)) LOG_WARN("LE Audio: stream lost: %s", why);
    stream_.stop_sending();
}

StreamStatus Core::stream_status() {
    std::lock_guard<std::mutex> lock(status_mutex_);
    return status_;
}

bool Core::start_stream(std::vector<int> device_ids, StreamRequest request, StreamStatus *status,
                        std::string *error, const Cancelled &cancelled) {
    if (!ready()) {
        if (error) *error = "LE Audio is not running";
        return false;
    }
    if (iso_packet_length() == 0 || iso_packet_count() == 0) {
        /* BTstack's ISO send would loop for ever without ISO buffers */
        const char *why = "adapter does not support LE Audio (no ISO buffers)";
        if (error) *error = why;
        LOG_ERROR("LE Audio: cannot stream: %s", why);
        return false;
    }
    stream_lost_.store(false);
    for (Module *m : modules()) m->prepare_group(*this, device_ids, cancelled);
    for (Module *m : modules()) m->adjust_stream_request(*this, device_ids, request);

    struct Res {
        StreamPlan plan;
        std::string why;
        bool ok = false;
    };
    auto res = std::make_shared<Res>();
    run([this, device_ids, request, res]() {
        std::vector<const LeAudioDevice *> devs;
        for (int id : device_ids) {
            LeAudioDevice *d = device(id);
            if (!d || !d->ready) {
                res->why = "a device of the group is not connected";
                return;
            }
            devs.push_back(d);
        }
        res->ok = plan_stream(devs, request, res->plan, res->why);
    });
    if (!res->ok) {
        if (error) *error = res->why.empty() ? "planning failed" : res->why;
        LOG_ERROR("LE Audio: cannot stream: %s", res->why.c_str());
        return false;
    }
    if (!stream_.configure(res->plan)) {
        if (error) *error = "LC3 encoder setup failed";
        return false;
    }
    {
        std::lock_guard<std::mutex> lock(status_mutex_);
        status_ = StreamStatus();
        status_.plan = res->plan;
        status_.device_ids = device_ids;
        status_.summary = res->plan.describe();
    }
    StreamPlan plan = res->plan;
    auto why = std::make_shared<std::string>();
    bool ok = call([this, plan, why](std::function<void(bool)> done) {
        if (!unicast_) {   /* stack_deinit() ran */
            *why = "LE Audio stopped";
            done(false);
            return;
        }
        if (unicast_->busy()) {
            *why = "a stream is still being stopped";
            done(false);
            return;
        }
        unicast_->start(plan, [why, done](bool ok, const std::string &reason) {
            *why = reason;
            done(ok);
        });
    }, 45000, cancelled);
    if (!ok) {
        std::string reason = !why->empty() ? *why : (cancelled && cancelled()) ? "cancelled" : "timeout";
        if (error) *error = reason;
        LOG_ERROR("LE Audio: stream setup failed: %s", reason.c_str());
        stop_stream();
        return false;
    }
    if (status) *status = stream_status();
    LOG_INFO("LE Audio: streaming: %s", stream_status().summary.c_str());
    return true;
}

void Core::stop_stream(uint32_t timeout_ms) {
    if (!initialized_.load()) return;
    call([this](std::function<void(bool)> done) {
        stream_.stop_sending();
        if (!unicast_) { done(true); return; }
        unicast_->stop([done]() { done(true); });
    }, timeout_ms);
    stream_.clear();
}

/* ======================================================================== */
/* Volume (modules)                                                          */
/* ======================================================================== */

int Core::remote_volume() {
    for (Module *m : modules()) {
        int v = m->remote_volume();
        if (v >= 0) return v;
    }
    return -1;
}

bool Core::request_volume(uint8_t level) {
    for (Module *m : modules())
        if (m->request_volume(level)) return true;
    return false;
}

} /* namespace le_audio */
