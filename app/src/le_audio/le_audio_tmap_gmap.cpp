/*
 * LE Audio (experimental) - TMAP and GMAP roles
 *
 * TMAP server (TMAS 0x1855 on the PC): TMAP Role = Unicast Media Sender
 * only. A2DPWB sends media to unicast sink ASEs, which is what UMS is. It
 * does not claim:
 *  - Call Gateway: needs CCP (a TBS / GTBS call control server) and
 *    conversational, bidirectional audio (source ASEs, the headset's
 *    microphone), which A2DPWB does not have;
 *  - Call Terminal / Unicast Media Receiver / Broadcast Media Receiver: the
 *    PC never renders LE Audio;
 *  - Broadcast Media Sender: no broadcast (BIS) support.
 *
 * TMAP client: the headset's TMAP Role (UMR, CT, ...) is read once the GATT
 * cache is ready and logged as "TMAP: UMR". It does not change the stream:
 * the media presets the core picks already are what UMS / UMR need (see
 * docs/le-audio-dev.md, "TMAP / GMAP", for the conformance notes).
 *
 * GMAP server (GMAS 0x1858 on the PC): GMAP Role = Unicast Game Gateway,
 * UGG Features = Multiplex (A2DPWB sends both channels of a stereo headset
 * as two LC3 frames in one SDU when the headset allows it) | 96 kbps Source
 * (it can send 48_3_gr / 48_4_gr). Not Multisink: that is about receiving
 * audio from the headsets, and A2DPWB has no source ASEs.
 *
 * GMAP client: the headset's GMAP Role and UGT Features are read with the
 * TMAP Role. When "Low latency (GMAP)" is on and every device of the group
 * is a UGT with the Sink feature, adjust_stream_request() puts the GMAP
 * UGG -> UGT presets (le_audio_presets.cpp) first and asks for the Game
 * context; plan_stream() still takes only what the sink PAC records allow
 * and falls back to the media presets behind them. Otherwise the request is
 * left alone (media presets, Media context).
 *
 * State per device id under a mutex: the worker thread reads it.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_tmap_gmap.h"
#include "le_audio_core.h"
#include "le_audio_gatt_client.h"
#include "le_audio_gatt_server.h"
#include "debug_log.h"

#include <algorithm>
#include <atomic>
#include <iterator>
#include <map>
#include <memory>
#include <mutex>

extern "C" {
#include "btstack_config.h"
#include "btstack.h"
}

namespace le_audio {

namespace {

std::string bit_names(uint32_t value, const char *const *names, int count, const char *sep) {
    std::string s;
    for (int i = 0; i < count; i++) {
        if (!(value & (1u << i))) continue;
        if (!s.empty()) s += sep;
        s += names[i];
    }
    uint32_t rest = value & ~((1u << count) - 1u);
    if (rest) {
        char buf[24];
        snprintf(buf, sizeof(buf), "%s0x%X", s.empty() ? "" : sep, rest);
        s += buf;
    }
    return s.empty() ? "none" : s;
}

/* GMAP presets for a UGT, best first: 48 kHz before 32 kHz, the 96 kbps
 * settings (UGG 96 kbps Source) before 80 kbps, 7.5 ms before 10 ms frames
 * at the same rate. plan_stream() skips what the PAC records do not allow. */
const char *const GMAP_PRESET_ORDER[] = {"48_3_gr", "48_4_gr", "48_1_gr", "48_2_gr", "32_1_gr", "32_2_gr"};

std::atomic<bool> g_low_latency{false};

/* How long adjust_stream_request() waits for role reads still queued */
constexpr uint32_t ROLE_READ_WAIT_MS = 5000;

} /* namespace */

void set_gmap_low_latency(bool on) { g_low_latency.store(on); }
bool gmap_low_latency() { return g_low_latency.load(); }

std::string describe_tmap_role(uint16_t role) {
    static const char *const names[] = {"CG", "CT", "UMS", "UMR", "BMS", "BMR"};
    return bit_names(role, names, 6, "+");
}

std::string describe_gmap_role(uint8_t role) {
    static const char *const names[] = {"UGG", "UGT", "BGS", "BGR"};
    return bit_names(role, names, 4, "+");
}

std::string describe_ugt_features(uint8_t features) {
    static const char *const names[] = {"source", "80 kbps source", "sink", "64 kbps sink", "multiplex",
                                        "multisink", "multisource"};
    return bit_names(features, names, 7, ", ");
}

namespace {

class TmapGmapModule : public Module {
public:
    const char *name() const override { return "TMAP/GMAP"; }

    void add_gatt_services(GattServer &server) override {
        /* TMAS: TMAP Role; GMAS: GMAP Role + UGG Features. Read only, no
         * security beyond the link's. */
        server.begin_service(UUID_SERVICE_TMAS);
        server.add_characteristic(UUID_CHR_TMAP_ROLE, ATT_PROPERTY_READ, [](uint16_t) {
            return std::vector<uint8_t>{(uint8_t)PC_TMAP_ROLE, (uint8_t)(PC_TMAP_ROLE >> 8)};
        }, nullptr);
        server.end_service();
        server.begin_service(UUID_SERVICE_GMAS);
        server.add_characteristic(UUID_CHR_GMAP_ROLE, ATT_PROPERTY_READ, [](uint16_t) {
            return std::vector<uint8_t>{PC_GMAP_ROLE};
        }, nullptr);
        server.add_characteristic(UUID_CHR_UGG_FEATURES, ATT_PROPERTY_READ, [](uint16_t) {
            return std::vector<uint8_t>{PC_UGG_FEATURES};
        }, nullptr);
        server.end_service();
        LOG_INFO("LE Audio: TMAS: TMAP Role %s; GMAS: GMAP Role %s, UGG Features 0x%02x",
                 describe_tmap_role(PC_TMAP_ROLE).c_str(), describe_gmap_role(PC_GMAP_ROLE).c_str(),
                 PC_UGG_FEATURES);
    }

    void on_stack_deinit() override {
        std::lock_guard<std::mutex> lock(mutex_);
        devices_.clear();
        waiters_.clear();
    }

    void on_gatt_ready(LeAudioDevice &dev) override {
        const int id = dev.id;
        const uint16_t con = dev.acl;
        {
            std::lock_guard<std::mutex> lock(mutex_);
            devices_[id] = State();
        }
        const GattChar *tmap = dev.find_char(UUID_SERVICE_TMAS, UUID_CHR_TMAP_ROLE);
        if (tmap) {
            read_u(id, con, tmap->value_handle, 2, "TMAP Role", [](State &s, uint32_t v) {
                s.info.has_tmas = true;
                s.info.tmap_role = (uint16_t)v;
            });
        }
        const GattChar *gmap = dev.find_char(UUID_SERVICE_GMAS, UUID_CHR_GMAP_ROLE);
        if (gmap) {
            read_u(id, con, gmap->value_handle, 1, "GMAP Role", [](State &s, uint32_t v) {
                s.info.has_gmas = true;
                s.info.gmap_role = (uint8_t)v;
            });
            const GattChar *ugt = dev.find_char(UUID_SERVICE_GMAS, UUID_CHR_UGT_FEATURES);
            if (ugt) {
                read_u(id, con, ugt->value_handle, 1, "UGT Features", [](State &s, uint32_t v) {
                    s.info.ugt_features = (uint8_t)v;
                });
            }
        }
        if (!tmap && !gmap)
            LOG_INFO("LE Audio: %s: no TMAS / GMAS (neither TMAP nor GMAP)", dev.addr.str().c_str());
        finish_if_done(id);
    }

    void on_disconnected(LeAudioDevice &dev, uint8_t reason) override {
        (void)reason;
        {
            std::lock_guard<std::mutex> lock(mutex_);
            devices_.erase(dev.id);
        }
        wake_waiters();
    }

    void on_stream_state(const StreamStatus &status, bool streaming) override {
        if (streaming && status.plan.preset.name.find("_gr") != std::string::npos)
            LOG_INFO("LE Audio: GMAP: streaming %s, contexts 0x%04x, presentation delay %u us",
                     status.plan.preset.name.c_str(), status.plan.contexts, status.plan.presentation_delay_us);
    }

    void adjust_stream_request(Core &core, const std::vector<int> &ids, StreamRequest &request) override {
        if (!gmap_low_latency()) {
            LOG_INFO("LE Audio: GMAP: low latency off, media configuration");
            return;
        }
        /* The role reads were queued in on_gatt_ready() and may still run:
         * wait for them (BTstack thread re-checks when a read finishes) */
        auto wanted = std::make_shared<std::vector<int>>(ids);
        if (!core.call([this, wanted](std::function<void(bool)> done) { wait_reads(wanted, done); },
                       ROLE_READ_WAIT_MS))
            LOG_WARN("LE Audio: GMAP: role reads not finished, deciding with what is known");

        std::string why;
        for (int id : ids) {
            TmapGmapInfo i;
            bool known = info(id, i);
            if (known && i.ugt_sink()) continue;
            char buf[96];
            if (!known || !i.has_gmas)
                snprintf(buf, sizeof(buf), "device %d has no GMAP role", id);
            else if (!(i.gmap_role & GMAP_ROLE_UGT))
                snprintf(buf, sizeof(buf), "device %d is not a Unicast Game Terminal (GMAP %s)", id,
                         describe_gmap_role(i.gmap_role).c_str());
            else
                snprintf(buf, sizeof(buf), "device %d is a UGT without the Sink feature", id);
            why = buf;
            break;
        }
        if (!why.empty()) {
            LOG_INFO("LE Audio: GMAP: low latency not used: %s; media configuration", why.c_str());
            return;
        }
        std::vector<std::string> order(std::begin(GMAP_PRESET_ORDER), std::end(GMAP_PRESET_ORDER));
        std::string list;
        for (const auto &n : order) list += (list.empty() ? "" : ", ") + n;
        for (const auto &n : request.preset_order)
            if (std::find(order.begin(), order.end(), n) == order.end()) order.push_back(n);
        request.preset_order = std::move(order);
        request.contexts = CONTEXT_GAME;
        LOG_INFO("LE Audio: GMAP: low latency for %u Unicast Game Terminal(s): %s first, Game context",
                 (unsigned)ids.size(), list.c_str());
    }

    bool info(int id, TmapGmapInfo &out) {
        std::lock_guard<std::mutex> lock(mutex_);
        auto it = devices_.find(id);
        if (it == devices_.end()) return false;
        out = it->second.info;
        return true;
    }

private:
    struct State {
        TmapGmapInfo info;
        int pending = 0;           /* reads queued */
    };

    static bool same_link(int id, uint16_t con) {
        LeAudioDevice *d = Core::instance().device(id);
        return d && d->acl == con;
    }

    /* Queue a read of a little-endian value of `size` bytes; apply() stores it */
    void read_u(int id, uint16_t con, uint16_t handle, uint16_t size, const char *what,
                std::function<void(State &, uint32_t)> apply) {
        {
            std::lock_guard<std::mutex> lock(mutex_);
            devices_[id].pending++;
        }
        gatt_client::read(con, handle, [this, id, con, size, what, apply](uint8_t status, const uint8_t *v, uint16_t n) {
            if (!same_link(id, con)) return;      /* link gone: on_disconnected() dropped the state */
            {
                std::lock_guard<std::mutex> lock(mutex_);
                auto it = devices_.find(id);
                if (it == devices_.end()) return;
                if (status == 0 && n >= size) {
                    uint32_t x = 0;
                    for (uint16_t i = 0; i < size; i++) x |= (uint32_t)v[i] << (8 * i);
                    apply(it->second, x);
                } else {
                    LOG_WARN("LE Audio: %s read failed (status 0x%02x, %u bytes)", what, status, n);
                }
                if (it->second.pending > 0) it->second.pending--;
            }
            finish_if_done(id);
        });
    }

    /* BTstack thread: all reads of id finished -> log, wake adjust_stream_request() */
    void finish_if_done(int id) {
        {
            std::lock_guard<std::mutex> lock(mutex_);
            auto it = devices_.find(id);
            if (it == devices_.end() || it->second.pending > 0 || it->second.info.done) return;
            it->second.info.done = true;
        }
        std::string summary = tmap_gmap_summary(id);
        LeAudioDevice *d = Core::instance().device(id);
        if (!summary.empty())
            LOG_INFO("LE Audio: %s: %s", d ? d->addr.str().c_str() : "?", summary.c_str());
        wake_waiters();
    }

    /* BTstack thread: done(true) once no read of these devices is pending
     * (a device the module does not know counts as finished) */
    void wait_reads(std::shared_ptr<std::vector<int>> ids, std::function<void(bool)> done) {
        {
            std::lock_guard<std::mutex> lock(mutex_);
            bool all = true;
            for (int id : *ids) {
                auto it = devices_.find(id);
                if (it != devices_.end() && !it->second.info.done) all = false;
            }
            if (!all) {
                waiters_.push_back([this, ids, done]() { wait_reads(ids, done); });
                return;
            }
        }
        done(true);
    }

    void wake_waiters() {
        std::vector<std::function<void()>> w;
        {
            std::lock_guard<std::mutex> lock(mutex_);
            w.swap(waiters_);
        }
        for (auto &f : w) f();
    }

    std::mutex mutex_;
    std::map<int, State> devices_;
    std::vector<std::function<void()>> waiters_;   /* BTstack thread */
};

TmapGmapModule &module_instance() {
    static TmapGmapModule module;
    return module;
}

} /* namespace */

Module *tmap_gmap_module() { return &module_instance(); }

bool tmap_gmap_info(int device_id, TmapGmapInfo &out) { return module_instance().info(device_id, out); }

std::string tmap_gmap_summary(int device_id) {
    TmapGmapInfo i;
    if (!tmap_gmap_info(device_id, i) || !i.done) return "";
    std::string s;
    if (i.has_tmas) s = "TMAP: " + describe_tmap_role(i.tmap_role);
    if (i.has_gmas) {
        if (!s.empty()) s += ", ";
        s += "GMAP: " + describe_gmap_role(i.gmap_role);
        if (i.gmap_role & GMAP_ROLE_UGT) {
            char buf[16];
            snprintf(buf, sizeof(buf), "0x%02X", i.ugt_features);
            s += std::string(" (UGT features ") + buf + ": " + describe_ugt_features(i.ugt_features) + ")";
        }
    }
    return s;
}

} /* namespace le_audio */
