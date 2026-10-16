/*
 * LE Audio (experimental) - VCP Volume Controller
 *
 * The headset volume follows the Windows volume and the other way round, as
 * AVRCP absolute volume does for A2DP (volume_sync.h): the LE streaming
 * thread runs VolumeSync with a target that calls Core::remote_volume() /
 * Core::request_volume(), which this module answers.
 *
 * Per device with a Volume Control Service (VCS, 0x1844), on_gatt_ready():
 * notifications of Volume State (0x2B7D: Volume_Setting 0-255, Mute, Change
 * Counter) on, Volume State and Volume Flags (0x2B7F) read; the Volume
 * Control Point (0x2B7E) is written with the last known Change Counter. An
 * "Invalid Change Counter" (ATT 0x80) answer re-reads Volume State and sends
 * the operation once more. VOCS / AICS (included services) are only logged.
 *
 * The group: every connected device with VCS (A2DPWB connects only the
 * devices it streams to, e.g. both earbuds of a set) shares one volume, the
 * "group volume" (Volume_Setting 0-255):
 *  - It starts as the volume of the first device whose Volume State is read
 *    (the -d device). Devices read later (the other earbud) are set to it.
 *    VolumeSync then makes Windows take this volume over, as it does with
 *    the AVRCP volume on an A2DP connect: the headset's volume is kept.
 *  - request_volume() (VolumeSync: the Windows volume changed) sets it and
 *    writes Set Absolute Volume to every device; a muted device is unmuted
 *    too when the level is above 0 (the user turned the volume up).
 *  - A device that reports another volume by notification while A2DPWB has
 *    no write of its own pending (the earbud's buttons) sets it: the other
 *    members get it written, and remote_volume() reports it so VolumeSync
 *    moves the Windows volume.
 *  - remote_volume() is the group volume mapped to 0-127, -1 while no VCS
 *    device is known. Mute is not mapped (VolumeSync syncs the level only).
 * Requests are coalesced per device: one Control Point write at a time,
 * followed by a read of Volume State; a newer level waits for it.
 *
 * Bumble side: tools/emu/le_audio/vcp.py (bumble.profiles.vcs on the sink).
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_vcp.h"
#include "le_audio_core.h"
#include "le_audio_gatt_client.h"

#include "debug_log.h"
#include "volume_sync.h"

#include <atomic>
#include <cstdlib>
#include <map>
#include <windows.h>

namespace le_audio {

namespace {

constexpr uint16_t UUID_VOLUME_STATE = 0x2B7D;
constexpr uint16_t UUID_VOLUME_CONTROL_POINT = 0x2B7E;
constexpr uint16_t UUID_VOLUME_FLAGS = 0x2B7F;

constexpr uint8_t OP_SET_ABSOLUTE_VOLUME = 0x04;
constexpr uint8_t OP_UNMUTE = 0x05;

constexpr uint8_t ATT_INVALID_CHANGE_COUNTER = 0x80;   /* VCS application error */
constexpr uint8_t ATT_OPCODE_NOT_SUPPORTED = 0x81;
constexpr uint8_t ATT_DISCONNECTED = 0x1F;             /* ATT_ERROR_HCI_DISCONNECT_RECEIVED */
constexpr uint16_t PROPERTY_NOTIFY = 0x10;             /* ATT_PROPERTY_NOTIFY */

constexpr bool round_trip_stable() {
    for (int level = 0; level <= 127; level++)
        if (vcp::to_level(vcp::to_setting(level)) != level) return false;
    return true;
}
static_assert(round_trip_stable(), "0-127 -> 0-255 -> 0-127 must give the same level");
static_assert(vcp::to_setting(127) == 255 && vcp::to_level(255) == 127 && vcp::to_level(0) == 0, "ends");

const char *att_error_text(uint8_t status) {
    switch (status) {
    case ATT_INVALID_CHANGE_COUNTER: return "Invalid Change Counter";
    case ATT_OPCODE_NOT_SUPPORTED: return "Opcode Not Supported";
    case ATT_DISCONNECTED: return "disconnected";
    default: return "error";
    }
}

/* One device with VCS (BTstack thread) */
struct VcsDevice {
    int id = -1;
    uint16_t con = CON_HANDLE_INVALID;
    std::string addr;
    uint16_t state_handle = 0;
    uint16_t cp_handle = 0;

    bool known = false;          /* Volume State read */
    uint8_t setting = 0;         /* Volume_Setting 0-255 */
    uint8_t mute = 0;
    uint8_t counter = 0;         /* Change_Counter */

    bool busy = false;           /* a Control Point write (and the read after it) runs */
    int want = -1;               /* Volume_Setting still to write, -1 none */
    bool want_unmute = false;
    bool retried = false;        /* the current operation was sent again after 0x80 */
};

class VcpModule : public Module {
public:
    const char *name() const override { return "VCP"; }

    void on_stack_deinit() override {
        devices_.clear();
        target_ = -1;
        publish();
    }

    void on_gatt_ready(LeAudioDevice &dev) override {
        const GattService *svc = dev.find_service(UUID_SERVICE_VCS);
        if (!svc) return;
        for (const auto &s : dev.services) {
            if (s.uuid16 != UUID_SERVICE_VOCS && s.uuid16 != UUID_SERVICE_AICS) continue;
            LOG_INFO("VCP: %s: %s %s(not used)", dev.addr.str().c_str(),
                     s.uuid16 == UUID_SERVICE_VOCS ? "Volume Offset Control" : "Audio Input Control",
                     s.included_by == svc->start_handle ? "included in VCS " : "");
        }
        const GattChar *state = dev.find_char(UUID_SERVICE_VCS, UUID_VOLUME_STATE);
        const GattChar *cp = dev.find_char(UUID_SERVICE_VCS, UUID_VOLUME_CONTROL_POINT);
        const GattChar *flags = dev.find_char(UUID_SERVICE_VCS, UUID_VOLUME_FLAGS);
        if (!state || !cp) {
            LOG_WARN("VCP: %s: Volume Control Service without Volume State / Control Point", dev.addr.str().c_str());
            return;
        }
        VcsDevice &v = devices_[dev.id];
        v = VcsDevice();
        v.id = dev.id;
        v.con = dev.acl;
        v.addr = dev.addr.str();
        v.state_handle = state->value_handle;
        v.cp_handle = cp->value_handle;
        count_.store(static_cast<int>(devices_.size()));
        LOG_INFO("VCP: %s: Volume Control Service (Volume State 0x%04x, Control Point 0x%04x)", v.addr.c_str(),
                 v.state_handle, v.cp_handle);

        const int id = dev.id;
        const uint16_t con = dev.acl;
        gatt_client::listen(con, state->value_handle, [this, id, con](const uint8_t *value, uint16_t len) {
            on_state(id, con, value, len, true);
        });
        gatt_client::subscribe(con, *state, false, [this, id](uint8_t status) {
            if (status && status != ATT_DISCONNECTED)
                LOG_WARN("VCP: %s: Volume State notifications not enabled (0x%02x)", addr_of(id), status);
        });
        gatt_client::read(con, state->value_handle, [this, id, con](uint8_t status, const uint8_t *value, uint16_t len) {
            if (status == 0) on_state(id, con, value, len, false);
            else if (status != ATT_DISCONNECTED)
                LOG_WARN("VCP: %s: reading Volume State failed (0x%02x)", addr_of(id), status);
        });
        if (flags) {
            auto on_flags = [this, id](const uint8_t *value, uint16_t len) {
                if (len < 1 || !find(id)) return;
                LOG_INFO("VCP: %s: Volume Flags 0x%02x (%s)", addr_of(id), value[0],
                         (value[0] & 0x01) ? "volume set by the user" : "default volume");
            };
            gatt_client::read(con, flags->value_handle, [on_flags](uint8_t status, const uint8_t *value, uint16_t len) {
                if (status == 0) on_flags(value, len);
            });
            if (flags->properties & PROPERTY_NOTIFY) {
                gatt_client::listen(con, flags->value_handle, on_flags);
                gatt_client::subscribe(con, *flags, false);
            }
        }
    }

    void on_disconnected(LeAudioDevice &dev, uint8_t reason) override {
        (void)reason;
        if (devices_.erase(dev.id) == 0) return;
        count_.store(static_cast<int>(devices_.size()));
        if (devices_.empty()) {
            /* Unknown until a device is read again: VolumeSync starts over */
            target_ = -1;
            publish();
        }
    }

    int remote_volume() override { return level_.load(); }

    bool request_volume(uint8_t level) override {
        if (count_.load() == 0) return false;
        if (level > 127) level = 127;
        level_.store(level);
        Core::instance().post([this, level]() { set_group_volume(vcp::to_setting(level)); });
        return true;
    }

private:
    VcsDevice *find(int id) {
        auto it = devices_.find(id);
        return it == devices_.end() ? nullptr : &it->second;
    }
    /* The device, if it is still on the link con */
    VcsDevice *find(int id, uint16_t con) {
        VcsDevice *v = find(id);
        if (!v || v->con != con) return nullptr;
        LeAudioDevice *d = Core::instance().device(id);
        return d && d->acl == con ? v : nullptr;
    }
    const char *addr_of(int id) {
        VcsDevice *v = find(id);
        return v ? v->addr.c_str() : "?";
    }

    void publish() { level_.store(target_ < 0 ? -1 : vcp::to_level(target_)); }

    /* Volume State read (from_notification false) or notified */
    void on_state(int id, uint16_t con, const uint8_t *value, uint16_t len, bool from_notification) {
        VcsDevice *v = find(id, con);
        if (!v) return;
        if (len < 3) {
            LOG_WARN("VCP: %s: malformed Volume State (%u bytes)", v->addr.c_str(), len);
            return;
        }
        const bool first = !v->known;
        const bool changed = first || value[0] != v->setting || value[1] != v->mute;
        const uint8_t old_mute = v->mute;
        v->known = true;
        v->setting = value[0];
        v->mute = value[1];
        v->counter = value[2];
        if (changed)
            LOG_INFO("VCP: %s: Volume State %u/255 (%d/127)%s, change counter %u%s", v->addr.c_str(), v->setting,
                     vcp::to_level(v->setting), v->mute ? ", muted" : "", v->counter,
                     from_notification ? " (notified)" : "");

        if (first) {
            if (target_ < 0) {
                /* The first device decides: Windows takes its volume over */
                target_ = v->setting;
                publish();
                LOG_INFO("VCP: group volume %d/255 (%d/127) from %s", target_, vcp::to_level(target_),
                         v->addr.c_str());
            } else if (v->setting != target_ && v->want < 0) {
                LOG_INFO("VCP: %s: setting it to the group volume %d/255", v->addr.c_str(), target_);
                v->want = target_;
            }
            sync(*v);
            return;
        }
        if (!from_notification || v->busy || v->want >= 0 || v->want_unmute) return;
        if (v->mute != old_mute)
            LOG_INFO("VCP: %s %s on the device", v->addr.c_str(), v->mute ? "muted" : "unmuted");
        if (v->setting == target_) return;
        /* Changed on the device (buttons): the group and Windows follow */
        LOG_INFO("VCP: %s: volume changed on the device to %u/255 (%d/127)", v->addr.c_str(), v->setting,
                 vcp::to_level(v->setting));
        target_ = v->setting;
        publish();
        for (auto &kv : devices_) {
            VcsDevice &o = kv.second;
            if (&o == v || (o.known && o.setting == target_ && o.want < 0)) continue;
            o.want = target_;
            sync(o);
        }
    }

    /* request_volume(): the Windows volume changed */
    void set_group_volume(uint8_t setting) {
        if (target_ >= 0 && vcp::to_level(target_) == vcp::to_level(setting)) {
            publish();
            return;   /* the same level: keep the devices' exact setting */
        }
        target_ = setting;
        publish();
        LOG_INFO("VCP: setting volume %d/127 (%u/255) on %u device(s)", vcp::to_level(setting), setting,
                 (unsigned)devices_.size());
        for (auto &kv : devices_) {
            kv.second.want = setting;
            kv.second.want_unmute = setting > 0;
            sync(kv.second);
        }
    }

    /* Write what is still wanted, one Control Point operation at a time */
    void sync(VcsDevice &v) {
        if (v.busy || !v.known) return;
        std::vector<uint8_t> op;
        int sent_want = -1;
        bool sent_unmute = false;
        if (v.want >= 0 && v.want != v.setting) {
            sent_want = v.want;
            op = {OP_SET_ABSOLUTE_VOLUME, v.counter, static_cast<uint8_t>(v.want)};
        } else if (v.want_unmute && v.mute) {
            sent_unmute = true;
            op = {OP_UNMUTE, v.counter};
        }
        v.want = -1;
        if (op.empty()) {
            v.want_unmute = false;
            v.retried = false;
            return;
        }
        if (sent_unmute) v.want_unmute = false;
        v.busy = true;
        const int id = v.id;
        const uint16_t con = v.con;
        gatt_client::write(con, v.cp_handle, op, [this, id, con, sent_want, sent_unmute](uint8_t status) {
            VcsDevice *x = find(id, con);
            if (!x) return;
            if (status == ATT_DISCONNECTED) {
                x->busy = false;   /* the link is going away; on_disconnected() drops it */
                return;
            }
            const std::string what = sent_unmute ? std::string("Unmute")
                                                 : "Set Absolute Volume " + std::to_string(sent_want);
            bool retry = false;
            if (status == 0) {
                LOG_INFO("VCP: %s: %s done", x->addr.c_str(), what.c_str());
                x->retried = false;
            } else if (status == ATT_INVALID_CHANGE_COUNTER && !x->retried) {
                LOG_WARN("VCP: %s: %s: Invalid Change Counter (0x80), reading Volume State again and retrying",
                         x->addr.c_str(), what.c_str());
                x->retried = true;
                retry = true;
            } else {
                LOG_WARN("VCP: %s: %s failed: %s (0x%02x)", x->addr.c_str(), what.c_str(), att_error_text(status),
                         status);
                x->retried = false;
            }
            if (retry) {
                /* Send it again unless a newer request replaced it */
                if (sent_want >= 0 && x->want < 0) x->want = sent_want;
                if (sent_unmute) x->want_unmute = true;
            }
            /* Read the state the write left (also the Change Counter) */
            gatt_client::read(con, x->state_handle,
                              [this, id, con, status, retry](uint8_t st, const uint8_t *value, uint16_t len) {
                VcsDevice *y = find(id, con);
                if (!y) return;
                if (st == 0) on_state(id, con, value, len, false);
                y->busy = false;
                if (st == ATT_DISCONNECTED) return;
                if (status != 0 && !retry && y->want < 0 && y->known && y->setting != target_) {
                    /* Given up: report what the device really plays at */
                    target_ = y->setting;
                    publish();
                }
                sync(*y);
            });
        });
    }

    std::map<int, VcsDevice> devices_;   /* BTstack thread, by LeAudioDevice::id */
    int target_ = -1;                    /* group Volume_Setting, -1 unknown (BTstack thread) */
    std::atomic<int> level_{-1};         /* target_ as 0-127 for any thread */
    std::atomic<int> count_{0};          /* devices with VCS */
};

} /* namespace */

Module *vcp_module() {
    static VcpModule module;
    return &module;
}

/* ======================================================================== */
/* Test only: VolumeSync against a simulated Windows volume                 */
/* ======================================================================== */

namespace vcp {

SimulatedWindowsVolume::SimulatedWindowsVolume() = default;
SimulatedWindowsVolume::~SimulatedWindowsVolume() = default;

bool SimulatedWindowsVolume::parse(const std::string &spec) {
    steps_.clear();
    size_t pos = 0;
    bool first = true;
    while (pos <= spec.size()) {
        size_t end = spec.find(',', pos);
        if (end == std::string::npos) end = spec.size();
        std::string item = spec.substr(pos, end - pos);
        char *rest = nullptr;
        if (first) {
            long level = strtol(item.c_str(), &rest, 10);
            if (item.empty() || *rest || level < 0 || level > 127) return false;
            windows_ = static_cast<int>(level);
            first = false;
        } else {
            size_t colon = item.find(':');
            if (colon == std::string::npos) return false;
            double s = strtod(item.substr(0, colon).c_str(), &rest);
            if (*rest || s < 0) return false;
            long level = strtol(item.c_str() + colon + 1, &rest, 10);
            if (*rest || colon + 1 == item.size() || level < 0 || level > 127) return false;
            steps_.push_back({static_cast<uint32_t>(s * 1000.0), static_cast<int>(level)});
        }
        pos = end + 1;
    }
    return !first;
}

void SimulatedWindowsVolume::tick() {
    if (!sync_) {
        sync_.reset(new VolumeSync(L"", {[]() { return Core::instance().remote_volume(); },
                                         [](uint8_t level) { Core::instance().request_volume(level); }}));
        sync_->set_simulated({[this]() { return windows_; },
                              [this](int level) {
                                  windows_ = level;
                                  LOG_INFO("VCP test: simulated Windows volume set to %d/127 (by VolumeSync)", level);
                              }});
        start_ms_ = GetTickCount();
        LOG_INFO("VCP test: simulated Windows volume %d/127 (the real Windows volume is not used)", windows_);
    }
    uint32_t elapsed = GetTickCount() - start_ms_;
    while (next_ < steps_.size() && elapsed >= steps_[next_].first) {
        windows_ = steps_[next_].second;
        LOG_INFO("VCP test: simulated Windows volume changed to %d/127 (volume keys)", windows_);
        next_++;
    }
    sync_->tick(true);
}

} /* namespace vcp */
} /* namespace le_audio */
