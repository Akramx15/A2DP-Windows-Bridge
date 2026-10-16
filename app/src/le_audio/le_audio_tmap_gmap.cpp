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
 * State per device id under a mutex: the worker thread reads it.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_tmap_gmap.h"
#include "le_audio_core.h"
#include "le_audio_gatt_client.h"
#include "le_audio_gatt_server.h"
#include "debug_log.h"

#include <map>
#include <mutex>

extern "C" {
#include "btstack_config.h"
#include "btstack.h"
}

namespace le_audio {

std::string describe_tmap_role(uint16_t role) {
    static const char *const names[] = {"CG", "CT", "UMS", "UMR", "BMS", "BMR"};
    std::string s;
    for (int i = 0; i < 6; i++) {
        if (!(role & (1u << i))) continue;
        if (!s.empty()) s += '+';
        s += names[i];
    }
    if (role & 0xFFC0u) {
        char buf[16];
        snprintf(buf, sizeof(buf), "%s0x%04X", s.empty() ? "" : "+", role & 0xFFC0u);
        s += buf;
    }
    return s.empty() ? "none" : s;
}

namespace {

class TmapGmapModule : public Module {
public:
    const char *name() const override { return "TMAP/GMAP"; }

    void add_gatt_services(GattServer &server) override {
        /* TMAS: TMAP Role, read only, no security beyond the link's */
        server.begin_service(UUID_SERVICE_TMAS);
        server.add_characteristic(UUID_CHR_TMAP_ROLE, ATT_PROPERTY_READ, [](uint16_t) {
            return std::vector<uint8_t>{(uint8_t)PC_TMAP_ROLE, (uint8_t)(PC_TMAP_ROLE >> 8)};
        }, nullptr);
        server.end_service();
        LOG_INFO("LE Audio: TMAS: TMAP Role %s", describe_tmap_role(PC_TMAP_ROLE).c_str());
    }

    void on_stack_deinit() override {
        std::lock_guard<std::mutex> lock(mutex_);
        devices_.clear();
    }

    void on_gatt_ready(LeAudioDevice &dev) override {
        const int id = dev.id;
        const uint16_t con = dev.acl;
        const std::string addr = dev.addr.str();
        {
            std::lock_guard<std::mutex> lock(mutex_);
            devices_[id] = State();
        }
        const GattChar *tmap = dev.find_char(UUID_SERVICE_TMAS, UUID_CHR_TMAP_ROLE);
        if (tmap) {
            add_pending(id);
            gatt_client::read(con, tmap->value_handle, [this, id, con](uint8_t status, const uint8_t *v, uint16_t n) {
                if (!same_link(id, con)) return;
                if (status == 0 && n >= 2) {
                    std::lock_guard<std::mutex> lock(mutex_);
                    devices_[id].info.has_tmas = true;
                    devices_[id].info.tmap_role = rd16(v);
                } else {
                    LOG_WARN("LE Audio: TMAP Role read failed (0x%02x, %u bytes)", status, n);
                }
                read_done(id);
            });
        }
        if (!tmap) {
            LOG_INFO("LE Audio: %s: no TMAS (not a TMAP device)", addr.c_str());
        }
        finish_if_done(id);
    }

    void on_disconnected(LeAudioDevice &dev, uint8_t reason) override {
        (void)reason;
        std::lock_guard<std::mutex> lock(mutex_);
        devices_.erase(dev.id);
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

    void add_pending(int id) {
        std::lock_guard<std::mutex> lock(mutex_);
        devices_[id].pending++;
    }

    void read_done(int id) {
        {
            std::lock_guard<std::mutex> lock(mutex_);
            auto it = devices_.find(id);
            if (it == devices_.end()) return;
            if (it->second.pending > 0) it->second.pending--;
        }
        finish_if_done(id);
    }

    /* BTstack thread: all reads of id finished -> log */
    void finish_if_done(int id) {
        std::string summary;
        {
            std::lock_guard<std::mutex> lock(mutex_);
            auto it = devices_.find(id);
            if (it == devices_.end() || it->second.pending > 0 || it->second.info.done) return;
            it->second.info.done = true;
        }
        summary = tmap_gmap_summary(id);
        LeAudioDevice *d = Core::instance().device(id);
        if (!summary.empty())
            LOG_INFO("LE Audio: %s: %s", d ? d->addr.str().c_str() : "?", summary.c_str());
    }

    std::mutex mutex_;
    std::map<int, State> devices_;
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
    if (!i.has_tmas) return "";
    return "TMAP: " + describe_tmap_role(i.tmap_role);
}

} /* namespace le_audio */
