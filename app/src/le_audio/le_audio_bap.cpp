/*
 * LE Audio (experimental) - BAP unicast client (audio source to sink ASEs)
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_bap.h"
#include "le_audio_core.h"
#include "le_audio_gatt_client.h"
#include "le_audio_module.h"
#include "debug_log.h"

#include <algorithm>
#include <map>

extern "C" {
#include "btstack_config.h"
#include "btstack.h"
}

namespace le_audio {

namespace {
/* The Unicast of the Core (one), for the PACS listeners below */
Unicast *g_unicast = nullptr;
} /* namespace */

/* ======================================================================== */
/* PACS / ASCS read                                                          */
/* ======================================================================== */

namespace bap {
namespace {

/* Sink PAC records per characteristic (a device may have several) */
std::map<uint32_t, std::vector<PacRecord>> g_sink_pacs;  /* con << 16 | value handle */

void rebuild_sink_pacs(LeAudioDevice &dev) {
    dev.sink_pacs.clear();
    for (const auto &e : g_sink_pacs)
        if ((e.first >> 16) == dev.acl)
            dev.sink_pacs.insert(dev.sink_pacs.end(), e.second.begin(), e.second.end());
}

std::string pac_summary(const LeAudioDevice &dev) {
    std::string s;
    for (const auto &r : dev.sink_pacs) {
        char buf[96];
        if (r.lc3)
            snprintf(buf, sizeof(buf), "LC3 rates 0x%04x durations 0x%02x channels 0x%02x octets %u-%u frames/SDU %u",
                     r.caps.sampling_frequencies, r.caps.frame_durations, r.caps.channel_counts,
                     r.caps.min_octets, r.caps.max_octets, r.caps.max_frames_per_sdu);
        else
            snprintf(buf, sizeof(buf), "codec 0x%02x", r.codec_id[0]);
        if (!s.empty()) s += "; ";
        s += buf;
    }
    return s.empty() ? "none" : s;
}

struct Counter {
    int pending = 0;
    bool started = false;
    std::function<void(bool)> done;
    void begin() { pending++; }
    void end() {
        if (--pending == 0 && started && done) {
            auto d = std::move(done);
            done = nullptr;
            d(true);
        }
    }
};

} /* namespace */

void read_device(Core &core, LeAudioDevice &dev, std::function<void(bool)> done) {
    const int id = dev.id;
    const uint16_t con = dev.acl;
    for (auto it = g_sink_pacs.begin(); it != g_sink_pacs.end();)
        it = (it->first >> 16) == con ? g_sink_pacs.erase(it) : std::next(it);
    dev.sink_pacs.clear();
    dev.ases.clear();
    dev.ase_cp_handle = 0;

    auto counter = std::make_shared<Counter>();
    counter->done = std::move(done);
    Core *c = &core;
    auto dev_for = [c, id, con]() -> LeAudioDevice * {
        LeAudioDevice *d = c->device(id);
        return (d && d->acl == con) ? d : nullptr;
    };
    auto subscribe = [&](const GattChar &chr) {
        if (!(chr.properties & (ATT_PROPERTY_NOTIFY | ATT_PROPERTY_INDICATE))) return;
        counter->begin();
        bool ind = !(chr.properties & ATT_PROPERTY_NOTIFY);
        uint16_t h = chr.value_handle;
        gatt_client::subscribe(con, chr, ind, [counter, h](uint8_t status) {
            if (status) LOG_WARN("LE Audio: enabling notifications of 0x%04x failed (0x%02x)", h, status);
            counter->end();
        });
    };

    /* ---- PACS ---- */
    dev.has_pacs = dev.find_service(UUID_SERVICE_PACS) != nullptr;
    for (const GattChar *chr : dev.find_chars(UUID_SERVICE_PACS, UUID_CHR_SINK_PAC)) {
        uint16_t h = chr->value_handle;
        auto on_value = [dev_for, con, h](const uint8_t *v, uint16_t n) {
            LeAudioDevice *d = dev_for();
            if (!d) return;
            std::vector<PacRecord> recs;
            if (!parse_pac_records(v, n, recs)) {
                LOG_WARN("LE Audio: %s: malformed Sink PAC (%u bytes)", d->addr.str().c_str(), n);
                return;
            }
            g_sink_pacs[((uint32_t)con << 16) | h] = std::move(recs);
            rebuild_sink_pacs(*d);
        };
        counter->begin();
        gatt_client::read(con, h, [counter, on_value](uint8_t status, const uint8_t *v, uint16_t n) {
            if (status == 0) on_value(v, n);
            counter->end();
        });
        gatt_client::listen(con, h, on_value);
        subscribe(*chr);
    }
    if (const GattChar *chr = dev.find_char(UUID_SERVICE_PACS, UUID_CHR_SINK_AUDIO_LOCATIONS)) {
        auto on_value = [dev_for](const uint8_t *v, uint16_t n) {
            if (LeAudioDevice *d = dev_for()) if (n >= 4) d->sink_locations = rd32(v);
        };
        counter->begin();
        gatt_client::read(con, chr->value_handle, [counter, on_value](uint8_t status, const uint8_t *v, uint16_t n) {
            if (status == 0) on_value(v, n);
            counter->end();
        });
        gatt_client::listen(con, chr->value_handle, on_value);
        subscribe(*chr);
    }
    if (const GattChar *chr = dev.find_char(UUID_SERVICE_PACS, UUID_CHR_AVAILABLE_AUDIO_CONTEXTS)) {
        auto on_value = [dev_for](const uint8_t *v, uint16_t n) {
            if (LeAudioDevice *d = dev_for()) if (n >= 4) {
                d->available_sink_contexts = rd16(v);
                d->available_source_contexts = rd16(v + 2);
                if (g_unicast) g_unicast->on_available_contexts(*d);
            }
        };
        counter->begin();
        gatt_client::read(con, chr->value_handle, [counter, on_value](uint8_t status, const uint8_t *v, uint16_t n) {
            if (status == 0) on_value(v, n);
            counter->end();
        });
        gatt_client::listen(con, chr->value_handle, on_value);
        subscribe(*chr);
    }
    if (const GattChar *chr = dev.find_char(UUID_SERVICE_PACS, UUID_CHR_SUPPORTED_AUDIO_CONTEXTS)) {
        counter->begin();
        gatt_client::read(con, chr->value_handle, [counter, dev_for](uint8_t status, const uint8_t *v, uint16_t n) {
            if (LeAudioDevice *d = dev_for()) if (status == 0 && n >= 4) {
                d->supported_sink_contexts = rd16(v);
                d->supported_source_contexts = rd16(v + 2);
            }
            counter->end();
        });
    }

    /* ---- ASCS ---- */
    dev.has_ascs = dev.find_service(UUID_SERVICE_ASCS) != nullptr;
    auto add_ases = [&](uint16_t uuid, bool sink) {
        for (const GattChar *chr : dev.find_chars(UUID_SERVICE_ASCS, uuid)) {
            Ase a;
            a.sink = sink;
            a.value_handle = chr->value_handle;
            uint16_t h = chr->value_handle;
            /* The ASE ID is only known from the value: keep the entry by
             * handle until the read fills it in */
            dev.ases.push_back(a);
            auto on_value = [c, dev_for, h](const uint8_t *v, uint16_t n) {
                LeAudioDevice *d = dev_for();
                if (!d) return;
                for (auto &ase : d->ases) {
                    if (ase.value_handle != h) continue;
                    Ase parsed = ase;
                    if (!parse_ase_value(v, n, parsed)) {
                        LOG_WARN("LE Audio: %s: malformed ASE value (%u bytes)", d->addr.str().c_str(), n);
                        return;
                    }
                    AseState old = ase.state;
                    ase = parsed;
                    if (old != ase.state)
                        LOG_INFO("LE Audio: %s ASE %u (%s): %s", d->addr.str().c_str(), ase.id,
                                 ase.sink ? "sink" : "source", ase_state_name(ase.state));
                    c->on_ase_update(*d, ase);
                    return;
                }
            };
            counter->begin();
            gatt_client::read(con, h, [counter, on_value](uint8_t status, const uint8_t *v, uint16_t n) {
                if (status == 0) on_value(v, n);
                counter->end();
            });
            gatt_client::listen(con, h, on_value);
            if (sink) subscribe(*chr);
        }
    };
    add_ases(UUID_CHR_SINK_ASE, true);
    add_ases(UUID_CHR_SOURCE_ASE, false);
    if (const GattChar *chr = dev.find_char(UUID_SERVICE_ASCS, UUID_CHR_ASE_CONTROL_POINT)) {
        dev.ase_cp_handle = chr->value_handle;
        gatt_client::listen(con, chr->value_handle, [c, dev_for](const uint8_t *v, uint16_t n) {
            if (LeAudioDevice *d = dev_for()) c->on_cp_response(*d, v, n);
        });
        subscribe(*chr);
    }

    /* Summary once everything queued above completed */
    counter->begin();
    gatt_client::custom(con,
        [](void (*)(uint8_t, uint16_t, uint8_t *, uint16_t), uint16_t) -> uint8_t { return 0xFF; },
        nullptr,
        [counter, dev_for](uint8_t) {
            if (LeAudioDevice *d = dev_for()) {
                LOG_INFO("LE Audio: %s: PACS %s, sink locations 0x%08x, contexts available 0x%04x "
                         "supported 0x%04x; sink PAC: %s",
                         d->addr.str().c_str(), d->has_pacs ? "yes" : "no", d->sink_locations,
                         d->available_sink_contexts, d->supported_sink_contexts, pac_summary(*d).c_str());
                LOG_INFO("LE Audio: %s: ASCS %s, %d sink ASE(s), %u ASE(s) in all, control point 0x%04x",
                         d->addr.str().c_str(), d->has_ascs ? "yes" : "no", d->sink_ase_count(),
                         (unsigned)d->ases.size(), d->ase_cp_handle);
            }
            counter->end();
        });
    counter->started = true;
}

} /* namespace bap */

/* ======================================================================== */
/* Unicast stream                                                            */
/* ======================================================================== */

namespace {
const uint32_t START_STEP_TIMEOUT_MS = 8000;
const uint32_t STOP_STEP_TIMEOUT_MS = 3000;
} /* namespace */

struct Unicast::Impl {
    Unicast *self = nullptr;
    std::function<void(bool, const std::string &)> start_done;
    std::function<void()> stop_done;
    btstack_timer_source_t timer = {};
    bool timer_armed = false;
    /* CIG storage handed to BTstack (gap_cig_create keeps the pointers
     * until the CIG is removed). If a removal never finishes, the storage is
     * abandoned (leaked on purpose) instead of being reused under BTstack. */
    std::unique_ptr<le_audio_cig_t> cig;
    std::unique_ptr<le_audio_cig_params_t> cig_params;
    void abandon_cig() {
        cig.release();
        cig_params.release();
    }
    bool cig_exists = false;          /* created in BTstack, not removed yet */
    bool cig_pending = false;         /* gap_cig_create() waiting for GAP_SUBEVENT_CIG_CREATED */
    bool cig_removing = false;
    int cis_pending = 0;              /* GAP_SUBEVENT_CIS_CREATED still to come */
    int cis_up = 0;
    bool starting = false;            /* start() not reported yet */
    std::string fail_reason;
    uint16_t latency_ms = 0;
    uint8_t phy = PHY_2M;
    uint8_t framing = 0;              /* CIG / Config QoS: 0 unframed, 1 framed */
    bool waiting_contexts = false;    /* Enable waits for an available context */
    std::vector<uint16_t> cis_connected;  /* CIS handles established, not down yet */
    std::vector<uint16_t> cis_to_drop;    /* HCI_Disconnect still to send */
    std::vector<uint16_t> cis_dropping;   /* HCI_Disconnect sent, waiting for the event */
    btstack_timer_source_t drop_timer = {};
    bool drop_armed = false;

    static void drop_handler(btstack_timer_source_t *ts) {
        auto *impl = static_cast<Impl *>(btstack_run_loop_get_timer_context(ts));
        impl->drop_armed = false;
        impl->self->drop_cis();
    }
    void drop_arm(uint32_t ms) {
        drop_disarm();
        btstack_run_loop_set_timer_handler(&drop_timer, &drop_handler);
        btstack_run_loop_set_timer_context(&drop_timer, this);
        btstack_run_loop_set_timer(&drop_timer, ms);
        btstack_run_loop_add_timer(&drop_timer);
        drop_armed = true;
    }
    void drop_disarm() {
        if (drop_armed) btstack_run_loop_remove_timer(&drop_timer);
        drop_armed = false;
    }
    static void erase(std::vector<uint16_t> &v, uint16_t h) {
        v.erase(std::remove(v.begin(), v.end(), h), v.end());
    }
    static bool has(const std::vector<uint16_t> &v, uint16_t h) {
        return std::find(v.begin(), v.end(), h) != v.end();
    }

    static void timer_handler(btstack_timer_source_t *ts) {
        auto *impl = static_cast<Impl *>(btstack_run_loop_get_timer_context(ts));
        impl->timer_armed = false;
        impl->self->on_timeout();
    }
    void arm(uint32_t ms) {
        disarm();
        btstack_run_loop_set_timer_handler(&timer, &timer_handler);
        btstack_run_loop_set_timer_context(&timer, this);
        btstack_run_loop_set_timer(&timer, ms);
        btstack_run_loop_add_timer(&timer);
        timer_armed = true;
    }
    void disarm() {
        if (timer_armed) btstack_run_loop_remove_timer(&timer);
        timer_armed = false;
    }
};

const char *Unicast::phase_name(Phase p) {
    switch (p) {
    case Phase::Idle:          return "idle";
    case Phase::ConfigCodec:   return "Config Codec";
    case Phase::CreateCig:     return "Set CIG Parameters";
    case Phase::ConfigQos:     return "Config QoS";
    case Phase::Enable:        return "Enable";
    case Phase::CreateCis:     return "Create CIS";
    case Phase::WaitStreaming: return "Receiver Start Ready";
    case Phase::Streaming:     return "streaming";
    case Phase::Disable:       return "Disable";
    case Phase::Release:       return "Release";
    case Phase::RemoveCig:     return "Remove CIG";
    }
    return "?";
}

Unicast::Unicast(Core &core) : impl_(new Impl()), core_(core) {
    impl_->self = this;
    g_unicast = this;
}

Unicast::~Unicast() {
    impl_->disarm();
    impl_->drop_disarm();
    if (g_unicast == this) g_unicast = nullptr;
}

bool Unicast::uses_handle(uint16_t handle) const {
    if (phase_ == Phase::Idle) return false;
    for (const auto &c : plan_.cis)
        if (c.acl == handle || c.cis_handle == handle) return true;
    return false;
}

/* ---- helpers ---- */

namespace {

/* CP writes, one per device: the ASEs of that device in the plan */
std::map<int, std::vector<const CisPlan *>> by_device(const StreamPlan &plan) {
    std::map<int, std::vector<const CisPlan *>> m;
    for (const auto &c : plan.cis) m[c.device_id].push_back(&c);
    return m;
}

void cp_write(Core &core, LeAudioDevice &dev, std::vector<uint8_t> value, const char *what) {
    if (!dev.ase_cp_handle) {
        LOG_ERROR("LE Audio: %s has no ASE control point", dev.addr.str().c_str());
        return;
    }
    (void)core;
    std::string name = what;
    std::string addr = dev.addr.str();
    gatt_client::write(dev.acl, dev.ase_cp_handle, std::move(value), [name, addr](uint8_t status) {
        if (status) LOG_WARN("LE Audio: %s: ASE control point write (%s) failed (ATT 0x%02x)",
                             addr.c_str(), name.c_str(), status);
    });
}

} /* namespace */

/* ---- start ---- */

void Unicast::start(const StreamPlan &plan, std::function<void(bool, const std::string &)> done) {
    plan_ = plan;
    impl_->start_done = std::move(done);
    impl_->starting = true;
    impl_->fail_reason.clear();
    impl_->cis_up = 0;
    impl_->cis_pending = 0;
    impl_->waiting_contexts = false;
    impl_->cis_connected.clear();
    impl_->cis_to_drop.clear();
    impl_->cis_dropping.clear();
    impl_->drop_disarm();
    LOG_INFO("LE Audio: starting unicast stream: %s", plan_.describe().c_str());
    for (const auto &c : plan_.cis) {
        char locs[64] = "";
        for (const auto &ch : c.channels)
            snprintf(locs + strlen(locs), sizeof(locs) - strlen(locs), "%s0x%x", locs[0] ? "," : "", ch.allocation);
        LOG_INFO("LE Audio:   CIS %u: device %d ASE %u, locations %s, SDU %u bytes",
                 c.cis_id, c.device_id, c.ase_id, locs, c.sdu_len(plan_.preset.octets));
    }
    enter(Phase::ConfigCodec);
}

void Unicast::enter(Phase p) {
    phase_ = p;
    Impl &I = *impl_;
    bool stopping = p == Phase::Disable || p == Phase::Release || p == Phase::RemoveCig;
    I.arm(stopping ? STOP_STEP_TIMEOUT_MS : (p == Phase::Streaming ? 0 : START_STEP_TIMEOUT_MS));
    if (p == Phase::Streaming || p == Phase::Idle) I.disarm();
    LOG_INFO("LE Audio: unicast: %s", phase_name(p));

    switch (p) {
    case Phase::ConfigCodec:
        for (auto &e : by_device(plan_)) {
            LeAudioDevice *d = core_.device(e.first);
            if (!d || !d->connected()) { fail("a device is not connected"); return; }
            std::vector<uint8_t> v = {ASE_OP_CONFIG_CODEC, (uint8_t)e.second.size()};
            for (const CisPlan *c : e.second) {
                std::vector<uint8_t> cfg = codec_config_ltvs(plan_, *c);
                put8(v, c->ase_id);
                put8(v, plan_.preset.target_latency);
                put8(v, plan_.phy);
                v.insert(v.end(), {CODING_FORMAT_LC3, 0, 0, 0, 0});
                put8(v, (uint8_t)cfg.size());
                v.insert(v.end(), cfg.begin(), cfg.end());
            }
            cp_write(core_, *d, std::move(v), "Config Codec");
        }
        break;

    case Phase::CreateCig: {
        /* QoS from the preset within what the servers asked for */
        uint32_t pd = plan_.preset.presentation_delay_us, lower = 0, upper = 0xFFFFFF;
        uint16_t latency = plan_.preset.max_transport_latency_ms;
        uint8_t phy = plan_.phy;
        uint8_t framing = plan_.preset.framing;
        for (const auto &c : plan_.cis) {
            LeAudioDevice *d = core_.device(c.device_id);
            Ase *a = d ? d->ase(c.ase_id) : nullptr;
            if (!a) continue;
            /* Preferred_Framing 0x01: unframed ISO PDUs not supported */
            if (a->pref_framing == 0x01) framing = 1;
            if (a->pd_min_us) lower = std::max(lower, a->pd_min_us);
            if (a->pd_max_us) upper = std::min(upper, a->pd_max_us);
            if (a->pref_max_latency_ms) latency = std::min(latency, a->pref_max_latency_ms);
            if (a->pref_phy && !(a->pref_phy & phy)) phy = PHY_1M;
        }
        if (pd > upper) pd = upper;
        if (pd < lower) pd = lower;
        if (latency < 5) latency = 5;
        plan_.presentation_delay_us = pd;
        plan_.phy = phy;
        I.latency_ms = latency;
        I.phy = phy;
        I.framing = framing;
        LOG_INFO("LE Audio: QoS: SDU interval %u us, %s, max latency %u ms, RTN %u, PHY 0x%02x, "
                 "presentation delay %u us", plan_.preset.sdu_interval_us(), framing ? "framed" : "unframed",
                 latency, plan_.preset.rtn, phy, pd);

        if (!I.cig) {
            I.cig.reset(new le_audio_cig_t());
            I.cig_params.reset(new le_audio_cig_params_t());
        }
        le_audio_cig_params_t &cp = *I.cig_params;
        memset(&cp, 0, sizeof(cp));
        cp.cig_id = plan_.cig_id;
        cp.sdu_interval_c_to_p = plan_.preset.sdu_interval_us();
        cp.sdu_interval_p_to_c = plan_.preset.sdu_interval_us();
        cp.worst_case_sca = 0;
        cp.packing = 0;
        cp.framing = I.framing;
        cp.max_transport_latency_c_to_p = latency;
        cp.max_transport_latency_p_to_c = latency;
        cp.num_cis = (uint8_t)plan_.cis.size();
        for (size_t i = 0; i < plan_.cis.size(); i++) {
            le_audio_cis_params_t &cis = cp.cis_params[i];
            cis.cis_id = plan_.cis[i].cis_id;
            cis.max_sdu_c_to_p = plan_.cis[i].sdu_len(plan_.preset.octets);
            cis.max_sdu_p_to_c = 0;
            cis.phy_c_to_p = phy;
            cis.phy_p_to_c = phy;
            cis.rtn_c_to_p = plan_.preset.rtn;
            cis.rtn_p_to_c = plan_.preset.rtn;
        }
        uint8_t status = gap_cig_create(I.cig.get(), I.cig_params.get());
        for (int tries = 0; status == ERROR_CODE_ACL_CONNECTION_ALREADY_EXISTS && tries < 3; tries++) {
            /* BTstack still holds a CIG with this id (its removal never
             * finished, e.g. a controller that did not report a CIS down):
             * use another id */
            LOG_WARN("LE Audio: CIG %u still exists in BTstack, using CIG %u", plan_.cig_id,
                     plan_.cig_id % 4 + 1);
            plan_.cig_id = (uint8_t)(plan_.cig_id % 4 + 1);
            cp.cig_id = plan_.cig_id;
            status = gap_cig_create(I.cig.get(), I.cig_params.get());
        }
        if (status != ERROR_CODE_SUCCESS) {
            char why[80];
            snprintf(why, sizeof(why), "LE Set CIG Parameters could not start (0x%02x)", status);
            fail(why);
            return;
        }
        I.cig_pending = true;
        I.cig_exists = true;
        break;
    }

    case Phase::ConfigQos:
        for (auto &e : by_device(plan_)) {
            LeAudioDevice *d = core_.device(e.first);
            if (!d || !d->connected()) { fail("a device is not connected"); return; }
            std::vector<uint8_t> v = {ASE_OP_CONFIG_QOS, (uint8_t)e.second.size()};
            for (const CisPlan *c : e.second) {
                put8(v, c->ase_id);
                put8(v, plan_.cig_id);
                put8(v, c->cis_id);
                put24(v, plan_.preset.sdu_interval_us());
                put8(v, I.framing);
                put8(v, I.phy);
                put16(v, c->sdu_len(plan_.preset.octets));
                put8(v, plan_.preset.rtn);
                put16(v, I.latency_ms);
                put24(v, plan_.presentation_delay_us);
            }
            cp_write(core_, *d, std::move(v), "Config QoS");
        }
        break;

    case Phase::Enable:
        I.waiting_contexts = false;
        if (!send_enable()) return;
        break;

    case Phase::CreateCis: {
        hci_con_handle_t cis[MAX_NR_CIS], acl[MAX_NR_CIS];
        for (size_t i = 0; i < plan_.cis.size(); i++) {
            LeAudioDevice *d = core_.device(plan_.cis[i].device_id);
            plan_.cis[i].acl = d ? d->acl : CON_HANDLE_INVALID;
            cis[i] = plan_.cis[i].cis_handle;
            acl[i] = plan_.cis[i].acl;
        }
        I.cis_up = 0;
        uint8_t status = gap_cis_create(plan_.cig_id, cis, acl);
        if (status != ERROR_CODE_SUCCESS) {
            char why[80];
            snprintf(why, sizeof(why), "LE Create CIS could not start (0x%02x)", status);
            fail(why);
            return;
        }
        I.cis_pending = (int)plan_.cis.size();
        break;
    }

    case Phase::WaitStreaming:
        /* Sink ASEs: the server (the audio receiver) sends Receiver Start
         * Ready itself; the client waits for Streaming */
        break;

    case Phase::Streaming: {
        std::vector<uint16_t> handles;
        for (const auto &c : plan_.cis) handles.push_back(c.cis_handle);
        /* BTstack queues up to `queue` SDUs per CIS and sends each as one
         * HCI ISO data packet (plan_stream() made sure none needs
         * fragmenting): queue x CIS must stay within the controller's
         * Total_Num_ISO_Data_Packets, which Number Of Completed Packets
         * frees again */
        int count = core_.iso_packet_count();
        if (count < (int)handles.size()) {
            fail("the adapter has fewer ISO data buffers than the stream has CIS");
            return;
        }
        uint8_t queue = (uint8_t)std::min(2, count / (int)handles.size());
        hci_set_num_iso_packets_to_queue(queue);
        core_.stream().start_sending(handles, queue);
        I.starting = false;
        core_.notify_stream_state(true);
        if (I.start_done) {
            auto d = std::move(I.start_done);
            I.start_done = nullptr;
            d(true, "");
        }
        break;
    }

    case Phase::Disable: {
        core_.stream().stop_sending();
        bool any = false;
        for (auto &e : by_device(plan_)) {
            LeAudioDevice *d = core_.device(e.first);
            if (!d || !d->connected()) continue;
            std::vector<uint8_t> ids;
            for (const CisPlan *c : e.second) {
                Ase *a = d->ase(c->ase_id);
                if (a && (a->state == AseState::Enabling || a->state == AseState::Streaming)) ids.push_back(c->ase_id);
            }
            if (ids.empty()) continue;
            std::vector<uint8_t> v = {ASE_OP_DISABLE, (uint8_t)ids.size()};
            v.insert(v.end(), ids.begin(), ids.end());
            cp_write(core_, *d, std::move(v), "Disable");
            any = true;
        }
        if (!any) { enter(Phase::Release); return; }
        break;
    }

    case Phase::Release: {
        /* The ASEs left Streaming (Disable): end their CIS now. Servers may
         * keep an ASE in Releasing (or not leave QoS Configured) until the
         * client drops the CIS (BAP 1.0.1 5.6.5 / 5.6.6); BTstack would do it
         * only when the CIG is removed, after Release. */
        for (uint16_t h : I.cis_connected)
            if (!Impl::has(I.cis_to_drop, h) && !Impl::has(I.cis_dropping, h)) I.cis_to_drop.push_back(h);
        drop_cis();
        bool any = false;
        for (auto &e : by_device(plan_)) {
            LeAudioDevice *d = core_.device(e.first);
            if (!d || !d->connected()) continue;
            std::vector<uint8_t> ids;
            for (const CisPlan *c : e.second) {
                Ase *a = d->ase(c->ase_id);
                if (a && a->state != AseState::Idle && a->state != AseState::Releasing &&
                    a->state != AseState::Unknown)
                    ids.push_back(c->ase_id);
            }
            if (ids.empty()) continue;
            std::vector<uint8_t> v = {ASE_OP_RELEASE, (uint8_t)ids.size()};
            v.insert(v.end(), ids.begin(), ids.end());
            cp_write(core_, *d, std::move(v), "Release");
            any = true;
        }
        if (!any) { enter(Phase::RemoveCig); return; }
        break;
    }

    case Phase::RemoveCig:
        try_remove_cig();
        return;

    case Phase::Idle:
        break;
    }
    advance();
}

void Unicast::try_remove_cig() {
    Impl &I = *impl_;
    if (!I.cig_exists) { finish_stop(); return; }
    /* BTstack must not get gap_cig_remove() while it still waits for the
     * CIG or CIS of a running command: it would resume them afterwards */
    if (I.cig_pending || I.cis_pending > 0) return;
    /* ... nor while our own CIS disconnections are under way */
    if (!I.cis_to_drop.empty() || !I.cis_dropping.empty()) return;
    if (!I.cig_removing) {
        uint8_t status = gap_cig_remove(plan_.cig_id);
        if (status != ERROR_CODE_SUCCESS) {
            LOG_WARN("LE Audio: gap_cig_remove failed (0x%02x)", status);
            I.cig_exists = false;
            I.abandon_cig();
            finish_stop();
            return;
        }
        I.cig_removing = true;
    }
}

/* Streaming Audio Contexts for Enable from what the devices have available
 * now (Available Audio Contexts may have changed since plan_stream()) */
static uint16_t current_contexts(Core &core, const StreamPlan &plan) {
    std::vector<const LeAudioDevice *> devs;
    for (const auto &c : plan.cis) {
        const LeAudioDevice *d = core.device(c.device_id);
        if (d && std::find(devs.begin(), devs.end(), d) == devs.end()) devs.push_back(d);
    }
    return choose_streaming_contexts(devs, plan.requested_contexts);
}

std::string Unicast::contexts_text() const {
    std::string s;
    char buf[96];
    snprintf(buf, sizeof(buf), "wanted 0x%04x", plan_.requested_contexts);
    s = buf;
    std::vector<int> seen;
    for (const auto &c : plan_.cis) {
        if (std::find(seen.begin(), seen.end(), c.device_id) != seen.end()) continue;
        seen.push_back(c.device_id);
        const LeAudioDevice *d = core_.device(c.device_id);
        if (!d) continue;
        snprintf(buf, sizeof(buf), ", %s has 0x%04x", d->addr.str().c_str(), d->available_sink_contexts);
        s += buf;
    }
    return s;
}

/* Enable with the Streaming Audio Contexts the devices have available.
 * False (nothing sent) while none is: waits for an Available Audio Contexts
 * notification (on_available_contexts) until the step times out. */
bool Unicast::send_enable() {
    Impl &I = *impl_;
    uint16_t contexts = current_contexts(core_, plan_);
    if (!contexts) {
        if (!I.waiting_contexts)
            LOG_WARN("LE Audio: no audio context available for Enable (%s): waiting", contexts_text().c_str());
        I.waiting_contexts = true;
        return false;
    }
    I.waiting_contexts = false;
    if (contexts != plan_.requested_contexts)
        LOG_INFO("LE Audio: streaming context 0x%04x (0x%04x is not available)", contexts,
                 plan_.requested_contexts);
    plan_.contexts = contexts;
    /* Metadata: Streaming Audio Contexts, plus the CCID List of the
     * content control services the modules tie to these contexts */
    std::vector<uint8_t> ccids;
    for (Module *m : modules()) m->add_ccids(plan_.contexts, ccids);
    std::sort(ccids.begin(), ccids.end());
    ccids.erase(std::unique(ccids.begin(), ccids.end()), ccids.end());
    std::vector<uint8_t> meta;
    const uint8_t ctx_le[2] = {(uint8_t)plan_.contexts, (uint8_t)(plan_.contexts >> 8)};
    ltv_put(meta, META_STREAMING_CONTEXTS, ctx_le, 2);
    if (!ccids.empty()) {
        ltv_put(meta, META_CCID_LIST, ccids.data(), (uint8_t)ccids.size());
        std::string list;
        for (uint8_t id : ccids) list += (list.empty() ? "" : ",") + std::to_string(id);
        LOG_INFO("LE Audio: Enable metadata: contexts 0x%04x, CCID list %s", plan_.contexts, list.c_str());
    }
    for (auto &e : by_device(plan_)) {
        LeAudioDevice *d = core_.device(e.first);
        if (!d || !d->connected()) { fail("a device is not connected"); return false; }
        std::vector<uint8_t> v = {ASE_OP_ENABLE, (uint8_t)e.second.size()};
        for (const CisPlan *c : e.second) {
            put8(v, c->ase_id);
            put8(v, (uint8_t)meta.size());               /* metadata length */
            v.insert(v.end(), meta.begin(), meta.end());
        }
        cp_write(core_, *d, std::move(v), "Enable");
    }
    return true;
}

void Unicast::on_available_contexts(LeAudioDevice &dev) {
    if (phase_ != Phase::Enable || !impl_->waiting_contexts) return;
    bool planned = false;
    for (const auto &c : plan_.cis) planned |= c.device_id == dev.id;
    if (planned && send_enable()) advance();
}

/* HCI_Disconnect for the CIS in cis_to_drop. BTstack has no call for a CIS
 * of a CIG that stays (gap_disconnect only knows ACL / SCO connections);
 * its Disconnection Complete handling sets the ISO stream idle, so the CIG
 * removal later skips it. */
void Unicast::drop_cis() {
    Impl &I = *impl_;
    I.drop_disarm();
    while (!I.cis_to_drop.empty()) {
        if (!hci_can_send_command_packet_now()) {
            I.drop_arm(2);
            return;
        }
        uint16_t h = I.cis_to_drop.front();
        I.cis_to_drop.erase(I.cis_to_drop.begin());
        uint8_t status = hci_send_cmd(&hci_disconnect, h, ERROR_CODE_REMOTE_USER_TERMINATED_CONNECTION);
        if (status == ERROR_CODE_SUCCESS) {
            LOG_INFO("LE Audio: disconnecting CIS 0x%04x", h);
            I.cis_dropping.push_back(h);
        } else {
            LOG_WARN("LE Audio: disconnecting CIS 0x%04x failed (0x%02x)", h, status);
        }
    }
}

void Unicast::on_cig_removed() {
    Impl &I = *impl_;
    if (!I.cig_removing) return;
    I.cig_removing = false;
    I.cig_exists = false;
    LOG_INFO("LE Audio: CIG %u removed", plan_.cig_id);
    if (phase_ == Phase::RemoveCig) finish_stop();
}

void Unicast::finish_stop() {
    Impl &I = *impl_;
    bool was_starting = I.starting;
    phase_ = Phase::Idle;
    I.disarm();
    I.drop_disarm();
    I.waiting_contexts = false;
    I.cis_connected.clear();
    I.cis_to_drop.clear();
    I.cis_dropping.clear();
    I.starting = false;
    core_.notify_stream_state(false);
    LOG_INFO("LE Audio: unicast stream ended");
    if (was_starting && I.start_done) {
        auto d = std::move(I.start_done);
        I.start_done = nullptr;
        d(false, I.fail_reason.empty() ? std::string("stopped") : I.fail_reason);
    }
    if (I.stop_done) {
        auto d = std::move(I.stop_done);
        I.stop_done = nullptr;
        d();
    }
}

void Unicast::fail(const std::string &why) {
    Impl &I = *impl_;
    if (!I.fail_reason.empty()) return;
    I.fail_reason = why;
    LOG_ERROR("LE Audio: unicast stream failed in %s: %s", phase_name(phase_), why.c_str());
    enter(Phase::Disable);
}

void Unicast::stop(std::function<void()> done) {
    Impl &I = *impl_;
    if (phase_ == Phase::Idle) {
        if (done) done();
        return;
    }
    if (I.stop_done) {
        /* Already stopping: chain */
        auto prev = std::move(I.stop_done);
        I.stop_done = [prev, done]() { prev(); if (done) done(); };
    } else {
        I.stop_done = std::move(done);
    }
    if (phase_ == Phase::Disable || phase_ == Phase::Release || phase_ == Phase::RemoveCig) return;
    if (I.starting && I.fail_reason.empty()) I.fail_reason = "stopped";
    enter(Phase::Disable);
}

void Unicast::on_timeout() {
    switch (phase_) {
    case Phase::Disable:   LOG_WARN("LE Audio: Disable timed out"); enter(Phase::Release); break;
    case Phase::Release:   LOG_WARN("LE Audio: Release timed out"); enter(Phase::RemoveCig); break;
    case Phase::RemoveCig:
        LOG_WARN("LE Audio: CIG removal timed out");
        if (!impl_->cig_removing && impl_->cig_exists) {
            /* Still waiting for a CIG / CIS event: remove it anyway */
            impl_->cig_pending = false;
            impl_->cis_pending = 0;
            impl_->drop_disarm();
            impl_->cis_to_drop.clear();
            impl_->cis_dropping.clear();
            try_remove_cig();
            if (phase_ == Phase::RemoveCig) impl_->arm(STOP_STEP_TIMEOUT_MS);
        } else {
            /* BTstack never finished removing it: leave its storage alone */
            if (impl_->cig_removing) impl_->abandon_cig();
            impl_->cig_removing = false;
            impl_->cig_exists = false;
            finish_stop();
        }
        break;
    case Phase::Idle:
    case Phase::Streaming:
        break;
    case Phase::Enable:
        if (impl_->waiting_contexts) {
            fail("no audio context available for the stream (" + contexts_text() + ")");
            break;
        }
        [[fallthrough]];
    default: {
        std::string why = std::string("no answer in ") + phase_name(phase_);
        /* Which ASEs did not get there */
        for (const auto &c : plan_.cis) {
            LeAudioDevice *d = core_.device(c.device_id);
            Ase *a = d ? d->ase(c.ase_id) : nullptr;
            char buf[64];
            snprintf(buf, sizeof(buf), "; ASE %u %s", c.ase_id, a ? ase_state_name(a->state) : "gone");
            why += buf;
        }
        fail(why);
        break;
    }
    }
}

/* All planned ASEs satisfy pred (devices that are gone count as done while
 * stopping) */
template <typename Pred>
static bool all_ases(Core &core, const StreamPlan &plan, bool stopping, Pred pred) {
    for (const auto &c : plan.cis) {
        LeAudioDevice *d = core.device(c.device_id);
        if (!d || !d->connected()) {
            if (stopping) continue;
            return false;
        }
        Ase *a = d->ase(c.ase_id);
        if (!a || !pred(a->state)) return false;
    }
    return true;
}

void Unicast::advance() {
    switch (phase_) {
    case Phase::ConfigCodec:
        if (all_ases(core_, plan_, false, [](AseState s) { return s == AseState::CodecConfigured; }))
            enter(Phase::CreateCig);
        break;
    case Phase::ConfigQos:
        if (all_ases(core_, plan_, false, [](AseState s) { return s == AseState::QosConfigured; }))
            enter(Phase::Enable);
        break;
    case Phase::Enable:
        if (all_ases(core_, plan_, false,
                     [](AseState s) { return s == AseState::Enabling || s == AseState::Streaming; }))
            enter(Phase::CreateCis);
        break;
    case Phase::CreateCis:
        if (impl_->cis_up == (int)plan_.cis.size()) enter(Phase::WaitStreaming);
        break;
    case Phase::WaitStreaming:
        if (all_ases(core_, plan_, false, [](AseState s) { return s == AseState::Streaming; }))
            enter(Phase::Streaming);
        break;
    case Phase::Disable:
        if (all_ases(core_, plan_, true, [](AseState s) {
                return s != AseState::Enabling && s != AseState::Streaming && s != AseState::Disabling; }))
            enter(Phase::Release);
        break;
    case Phase::Release:
        if (all_ases(core_, plan_, true, [](AseState s) {
                return s == AseState::Idle || s == AseState::CodecConfigured || s == AseState::Unknown; }))
            enter(Phase::RemoveCig);
        break;
    case Phase::RemoveCig:
        try_remove_cig();
        break;
    default:
        break;
    }
}

/* ---- events ---- */

void Unicast::on_ase_update(LeAudioDevice &dev, const Ase &ase) {
    if (phase_ == Phase::Idle) return;
    bool planned = false;
    for (const auto &c : plan_.cis) planned |= c.device_id == dev.id && c.ase_id == ase.id;
    if (!planned) return;
    if (phase_ == Phase::Streaming && ase.state != AseState::Streaming) {
        /* The server disabled / released it (e.g. the headset switched to
         * another source) */
        LOG_WARN("LE Audio: %s ASE %u left Streaming (%s)", dev.addr.str().c_str(), ase.id,
                 ase_state_name(ase.state));
        core_.set_stream_lost("an ASE left the Streaming state");
        return;
    }
    advance();
}

void Unicast::on_cp_response(LeAudioDevice &dev, const uint8_t *v, uint16_t n) {
    if (n < 2) return;
    uint8_t op = v[0], count = v[1];
    if (count == 0xFF) {
        if (n >= 5 && v[3] != 0)
            LOG_WARN("LE Audio: %s: ASE control point op 0x%02x rejected: response 0x%02x reason 0x%02x",
                     dev.addr.str().c_str(), op, v[3], v[4]);
        if (phase_ != Phase::Idle && n >= 5 && v[3] != 0 && impl_->fail_reason.empty() &&
            phase_ != Phase::Disable && phase_ != Phase::Release && phase_ != Phase::RemoveCig) {
            char why[96];
            snprintf(why, sizeof(why), "ASE control point op 0x%02x rejected (0x%02x / 0x%02x)", op, v[3], v[4]);
            fail(why);
        }
        return;
    }
    for (uint8_t i = 0; i < count && 2u + 3u * (i + 1u) <= n; i++) {
        uint8_t ase_id = v[2 + 3 * i], code = v[3 + 3 * i], reason = v[4 + 3 * i];
        if (code == 0) continue;
        LOG_WARN("LE Audio: %s: ASE %u op 0x%02x: response 0x%02x reason 0x%02x",
                 dev.addr.str().c_str(), ase_id, op, code, reason);
        bool planned = false;
        for (const auto &c : plan_.cis) planned |= c.device_id == dev.id && c.ase_id == ase_id;
        bool start_phase = phase_ == Phase::ConfigCodec || phase_ == Phase::CreateCig ||
                           phase_ == Phase::ConfigQos || phase_ == Phase::Enable;
        if (planned && start_phase) {
            char why[96];
            snprintf(why, sizeof(why), "ASE %u: op 0x%02x rejected (response 0x%02x reason 0x%02x)",
                     ase_id, op, code, reason);
            fail(why);
            return;
        }
    }
}

void Unicast::on_cig_created(const uint8_t *packet, uint16_t size) {
    (void)size;
    Impl &I = *impl_;
    if (!I.cig_pending || gap_subevent_cig_created_get_cig_id(packet) != plan_.cig_id) return;
    I.cig_pending = false;
    uint8_t status = gap_subevent_cig_created_get_status(packet);
    if (status != ERROR_CODE_SUCCESS) {
        I.cig_exists = false;   /* BTstack dropped it */
        char why[80];
        snprintf(why, sizeof(why), "LE Set CIG Parameters failed (0x%02x)", status);
        fail(why);
        return;
    }
    uint8_t n = gap_subevent_cig_created_get_num_cis(packet);
    for (uint8_t i = 0; i < n && i < plan_.cis.size(); i++)
        plan_.cis[i].cis_handle = gap_subevent_cig_created_get_cis_con_handles(packet, i);
    LOG_INFO("LE Audio: CIG %u created, %u CIS", plan_.cig_id, n);
    if (phase_ == Phase::CreateCig) enter(Phase::ConfigQos);
    else advance();   /* stopping: may now remove it */
}

void Unicast::on_cis_created(const uint8_t *packet, uint16_t size) {
    (void)size;
    Impl &I = *impl_;
    if (gap_subevent_cis_created_get_cig_id(packet) != plan_.cig_id || I.cis_pending <= 0) return;
    I.cis_pending--;
    uint8_t status = gap_subevent_cis_created_get_status(packet);
    uint16_t handle = gap_subevent_cis_created_get_cis_con_handle(packet);
    if (status != ERROR_CODE_SUCCESS) {
        char why[80];
        snprintf(why, sizeof(why), "CIS 0x%04x could not be established (0x%02x)", handle, status);
        if (phase_ == Phase::CreateCis) fail(why);
        else LOG_WARN("LE Audio: %s", why);
    } else {
        I.cis_up++;
        I.cis_connected.push_back(handle);
        LOG_INFO("LE Audio: CIS 0x%04x established (ISO interval %u x 1.25 ms)", handle,
                 gap_subevent_cis_created_get_iso_interval_1250us(packet));
    }
    advance();
}

void Unicast::on_link_lost(uint16_t handle) {
    if (!uses_handle(handle)) return;
    Impl &I = *impl_;
    for (auto &c : plan_.cis) {
        if (c.cis_handle != handle) continue;
        if (Impl::has(I.cis_dropping, handle) || Impl::has(I.cis_to_drop, handle)) {
            LOG_INFO("LE Audio: CIS 0x%04x disconnected", handle);
        } else {
            LOG_WARN("LE Audio: CIS 0x%04x disconnected", handle);
            if (phase_ == Phase::CreateCis && I.cis_pending > 0) I.cis_pending--;
        }
        Impl::erase(I.cis_connected, handle);
        Impl::erase(I.cis_to_drop, handle);
        Impl::erase(I.cis_dropping, handle);
        core_.stream().on_cis_down(handle);
    }
    switch (phase_) {
    case Phase::Streaming:
        core_.set_stream_lost("a link of the stream went down");
        break;
    case Phase::Disable:
    case Phase::Release:
    case Phase::RemoveCig:
        advance();
        break;
    default:
        fail("a link went down");
        break;
    }
}

} /* namespace le_audio */
