/*
 * LE Audio (experimental) - serialized GATT client operations
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_gatt_client.h"
#include "debug_log.h"

#include <deque>
#include <map>
#include <memory>

extern "C" {
#include "btstack_config.h"
#include "btstack.h"
}

namespace le_audio {
namespace gatt_client {
namespace {

void packet_handler(uint8_t packet_type, uint16_t channel, uint8_t *packet, uint16_t size);

/* One queued operation */
struct Op {
    virtual ~Op() = default;
    /* Start the (next step of the) query; returns a BTstack status */
    virtual uint8_t start(uint16_t con) = 0;
    /* A GATT event of the running query (not QUERY_COMPLETE) */
    virtual void on_event(const uint8_t *packet, uint16_t size) { (void)packet; (void)size; }
    /* QUERY_COMPLETE: return true when the operation is finished, false when
     * it started another step (multi-step operations) */
    virtual bool on_complete(uint16_t con, uint8_t status) { (void)con; (void)status; return true; }
    /* Final result to the caller */
    virtual void finish(uint8_t status) = 0;
    /* Completes without a QUERY_COMPLETE event (write without response) */
    virtual bool immediate() const { return false; }
};

struct Conn {
    std::deque<std::unique_ptr<Op>> queue;
    bool running = false;          /* queue.front() started, waiting for its completion */
    gatt_client_notification_t listener = {};
    bool listening = false;
    btstack_timer_source_t retry_timer = {};
    bool retry_armed = false;
};

std::map<uint16_t, std::unique_ptr<Conn>> g_conns;
std::multimap<uint32_t, GattNotify> g_listeners;  /* key: con << 16 | value handle */
bool g_pumping = false;

uint32_t key(uint16_t con, uint16_t handle) { return ((uint32_t)con << 16) | handle; }

Conn *conn_for(uint16_t con) {
    auto it = g_conns.find(con);
    return it == g_conns.end() ? nullptr : it->second.get();
}

void pump(uint16_t con);

void retry_timer_handler(btstack_timer_source_t *ts) {
    uint16_t con = (uint16_t)(uintptr_t)btstack_run_loop_get_timer_context(ts);
    Conn *c = conn_for(con);
    if (!c) return;
    c->retry_armed = false;
    pump(con);
}

void arm_retry(uint16_t con, Conn *c, uint32_t ms) {
    if (c->retry_armed) return;
    c->retry_armed = true;
    btstack_run_loop_set_timer_handler(&c->retry_timer, &retry_timer_handler);
    btstack_run_loop_set_timer_context(&c->retry_timer, (void *)(uintptr_t)con);
    btstack_run_loop_set_timer(&c->retry_timer, ms);
    btstack_run_loop_add_timer(&c->retry_timer);
}

/* Start queued operations until one is running (or the queue is empty) */
void pump(uint16_t con) {
    if (g_pumping) return;   /* re-entered from a callback: the outer loop continues */
    g_pumping = true;
    for (;;) {
        Conn *c = conn_for(con);
        if (!c || c->running || c->queue.empty()) break;
        Op *op = c->queue.front().get();
        uint8_t status = op->start(con);
        if (status == GATT_CLIENT_BUSY || status == GATT_CLIENT_IN_WRONG_STATE) {
            /* Another gatt_client user (or a query BTstack runs itself, e.g.
             * the MTU exchange) is active: try again shortly */
            arm_retry(con, c, 5);
            break;
        }
        if (status != ERROR_CODE_SUCCESS || op->immediate()) {
            std::unique_ptr<Op> done = std::move(c->queue.front());
            c->queue.pop_front();
            done->finish(status);
            continue;
        }
        c->running = true;
    }
    g_pumping = false;
}

void complete_front(uint16_t con, uint8_t status) {
    Conn *c = conn_for(con);
    if (!c || !c->running || c->queue.empty()) return;
    Op *op = c->queue.front().get();
    if (status == ATT_ERROR_SUCCESS && !op->on_complete(con, status)) {
        /* The next step of a multi-step operation is running */
        return;
    }
    c->running = false;
    std::unique_ptr<Op> done = std::move(c->queue.front());
    c->queue.pop_front();
    done->finish(status);
    pump(con);
}

void enqueue(uint16_t con, std::unique_ptr<Op> op) {
    Conn *c = conn_for(con);
    if (!c) {
        op->finish(ATT_ERROR_HCI_DISCONNECT_RECEIVED);
        return;
    }
    c->queue.push_back(std::move(op));
    pump(con);
}

/* ---- Operations ---- */

struct ReadOp : Op {
    uint16_t handle;
    GattValue done;
    std::vector<uint8_t> value;
    ReadOp(uint16_t h, GattValue d) : handle(h), done(std::move(d)) {}
    uint8_t start(uint16_t con) override {
        value.clear();
        return gatt_client_read_long_value_of_characteristic_using_value_handle(&packet_handler, con, handle);
    }
    void on_event(const uint8_t *packet, uint16_t) override {
        if (hci_event_packet_get_type(packet) != GATT_EVENT_LONG_CHARACTERISTIC_VALUE_QUERY_RESULT) return;
        uint16_t off = gatt_event_long_characteristic_value_query_result_get_value_offset(packet);
        uint16_t len = gatt_event_long_characteristic_value_query_result_get_value_length(packet);
        const uint8_t *v = gatt_event_long_characteristic_value_query_result_get_value(packet);
        if (value.size() < (size_t)off + len) value.resize((size_t)off + len);
        memcpy(value.data() + off, v, len);
    }
    void finish(uint8_t status) override {
        if (done) done(status, value.data(), (uint16_t)value.size());
    }
};

struct WriteOp : Op {
    uint16_t handle;
    std::vector<uint8_t> value;
    GattDone done;
    WriteOp(uint16_t h, std::vector<uint8_t> v, GattDone d) : handle(h), value(std::move(v)), done(std::move(d)) {}
    uint8_t start(uint16_t con) override {
        uint16_t mtu = ATT_DEFAULT_MTU;
        gatt_client_get_mtu(con, &mtu);
        if (value.size() + 3u > mtu)
            return gatt_client_write_long_value_of_characteristic(&packet_handler, con, handle,
                                                                  (uint16_t)value.size(), value.data());
        return gatt_client_write_value_of_characteristic(&packet_handler, con, handle,
                                                         (uint16_t)value.size(), value.data());
    }
    void finish(uint8_t status) override {
        if (done) done(status);
    }
};

struct WriteNoRspOp : Op {
    uint16_t handle;
    std::vector<uint8_t> value;
    WriteNoRspOp(uint16_t h, std::vector<uint8_t> v) : handle(h), value(std::move(v)) {}
    uint8_t start(uint16_t con) override {
        return gatt_client_write_value_of_characteristic_without_response(con, handle,
                                                                          (uint16_t)value.size(), value.data());
    }
    bool immediate() const override { return true; }
    void finish(uint8_t status) override {
        if (status != ERROR_CODE_SUCCESS)
            LOG_WARN("LE Audio: write without response to 0x%04x failed (0x%02x)", handle, status);
    }
};

struct SubscribeOp : Op {
    gatt_client_characteristic_t chr = {};
    uint16_t config;
    GattDone done;
    SubscribeOp(const GattChar &c, bool indications, GattDone d) : done(std::move(d)) {
        chr.start_handle = c.start_handle;
        chr.value_handle = c.value_handle;
        chr.end_handle = c.end_handle;
        chr.properties = c.properties;
        chr.uuid16 = c.uuid16;
        memcpy(chr.uuid128, c.uuid128, 16);
        config = indications ? GATT_CLIENT_CHARACTERISTICS_CONFIGURATION_INDICATION
                             : GATT_CLIENT_CHARACTERISTICS_CONFIGURATION_NOTIFICATION;
    }
    uint8_t start(uint16_t con) override {
        return gatt_client_write_client_characteristic_configuration(&packet_handler, con, &chr, config);
    }
    void finish(uint8_t status) override {
        if (done) done(status);
    }
};

struct CustomOp : Op {
    StartFn start_fn;
    EventFn event_fn;
    GattDone done;
    CustomOp(StartFn s, EventFn e, GattDone d) : start_fn(std::move(s)), event_fn(std::move(e)), done(std::move(d)) {}
    uint8_t start(uint16_t con) override { return start_fn(&packet_handler, con); }
    void on_event(const uint8_t *packet, uint16_t size) override { if (event_fn) event_fn(packet, size); }
    void finish(uint8_t status) override { if (done) done(status); }
};

/* Primary services -> included services of each -> characteristics of each */
struct DiscoverOp : Op {
    GattDiscovered done;
    std::vector<GattService> services;
    enum class Step { Primary, Included, Chars } step = Step::Primary;
    size_t index = 0;
    explicit DiscoverOp(GattDiscovered d) : done(std::move(d)) {}

    static GattService from_btstack(const gatt_client_service_t &s) {
        GattService out;
        out.uuid16 = s.uuid16;
        memcpy(out.uuid128, s.uuid128, 16);
        out.start_handle = s.start_group_handle;
        out.end_handle = s.end_group_handle;
        return out;
    }
    static gatt_client_service_t to_btstack(const GattService &s) {
        gatt_client_service_t out = {};
        out.uuid16 = s.uuid16;
        memcpy(out.uuid128, s.uuid128, 16);
        out.start_group_handle = s.start_handle;
        out.end_group_handle = s.end_handle;
        return out;
    }
    uint8_t start(uint16_t con) override {
        return gatt_client_discover_primary_services(&packet_handler, con);
    }
    void on_event(const uint8_t *packet, uint16_t) override {
        switch (hci_event_packet_get_type(packet)) {
        case GATT_EVENT_SERVICE_QUERY_RESULT: {
            gatt_client_service_t s;
            gatt_event_service_query_result_get_service(packet, &s);
            services.push_back(from_btstack(s));
            break;
        }
        case GATT_EVENT_INCLUDED_SERVICE_QUERY_RESULT: {
            gatt_client_service_t s;
            gatt_event_included_service_query_result_get_service(packet, &s);
            bool known = false;
            for (const auto &e : services) known |= e.start_handle == s.start_group_handle;
            if (!known) {
                GattService g = from_btstack(s);
                g.primary = false;
                g.included_by = services[index].start_handle;
                services.push_back(g);
            }
            break;
        }
        case GATT_EVENT_CHARACTERISTIC_QUERY_RESULT: {
            gatt_client_characteristic_t c;
            gatt_event_characteristic_query_result_get_characteristic(packet, &c);
            GattChar g;
            g.uuid16 = c.uuid16;
            memcpy(g.uuid128, c.uuid128, 16);
            g.start_handle = c.start_handle;
            g.value_handle = c.value_handle;
            g.end_handle = c.end_handle;
            g.properties = c.properties;
            services[index].chars.push_back(g);
            break;
        }
        default:
            break;
        }
    }
    /* Starts the next step that has work; false when all is done */
    bool next(uint16_t con, uint8_t *status) {
        size_t primaries = 0;
        for (const auto &s : services) primaries += s.primary ? 1 : 0;
        for (;;) {
            if (step == Step::Included) {
                if (index >= primaries) { step = Step::Chars; index = 0; continue; }
                gatt_client_service_t s = to_btstack(services[index]);
                *status = gatt_client_find_included_services_for_service(&packet_handler, con, &s);
                return true;
            }
            if (step == Step::Chars) {
                if (index >= services.size()) return false;
                gatt_client_service_t s = to_btstack(services[index]);
                *status = gatt_client_discover_characteristics_for_service(&packet_handler, con, &s);
                return true;
            }
            return false;
        }
    }
    bool on_complete(uint16_t con, uint8_t) override {
        if (step == Step::Primary) { step = Step::Included; index = 0; }
        else index++;
        uint8_t status = ERROR_CODE_SUCCESS;
        if (!next(con, &status)) return true;
        if (status != ERROR_CODE_SUCCESS) {
            LOG_WARN("LE Audio: GATT discovery step failed to start (0x%02x)", status);
            return true;
        }
        return false;
    }
    void finish(uint8_t status) override {
        if (done) done(status, std::move(services));
    }
};

void packet_handler(uint8_t packet_type, uint16_t channel, uint8_t *packet, uint16_t size) {
    (void)channel;
    if (packet_type != HCI_EVENT_PACKET || size < 4) return;
    uint8_t type = hci_event_packet_get_type(packet);
    uint16_t con = little_endian_read_16(packet, 2);

    if (type == GATT_EVENT_NOTIFICATION || type == GATT_EVENT_INDICATION) {
        uint16_t handle, len;
        const uint8_t *value;
        if (type == GATT_EVENT_NOTIFICATION) {
            handle = gatt_event_notification_get_value_handle(packet);
            len = gatt_event_notification_get_value_length(packet);
            value = gatt_event_notification_get_value(packet);
        } else {
            handle = gatt_event_indication_get_value_handle(packet);
            len = gatt_event_indication_get_value_length(packet);
            value = gatt_event_indication_get_value(packet);
        }
        /* Copy the callbacks: one may register another listener */
        std::vector<GattNotify> cbs;
        auto range = g_listeners.equal_range(key(con, handle));
        for (auto it = range.first; it != range.second; ++it) cbs.push_back(it->second);
        for (auto &cb : cbs) cb(value, len);
        return;
    }

    Conn *c = conn_for(con);
    if (!c || !c->running || c->queue.empty()) return;
    if (type == GATT_EVENT_QUERY_COMPLETE) {
        complete_front(con, gatt_event_query_complete_get_att_status(packet));
        return;
    }
    c->queue.front()->on_event(packet, size);
}

} /* namespace */

void read(uint16_t con, uint16_t value_handle, GattValue done) {
    enqueue(con, std::make_unique<ReadOp>(value_handle, std::move(done)));
}

void write(uint16_t con, uint16_t value_handle, std::vector<uint8_t> value, GattDone done) {
    enqueue(con, std::make_unique<WriteOp>(value_handle, std::move(value), std::move(done)));
}

void write_without_response(uint16_t con, uint16_t value_handle, std::vector<uint8_t> value) {
    enqueue(con, std::make_unique<WriteNoRspOp>(value_handle, std::move(value)));
}

void subscribe(uint16_t con, const GattChar &chr, bool indications, GattDone done) {
    enqueue(con, std::make_unique<SubscribeOp>(chr, indications, std::move(done)));
}

void discover_all(uint16_t con, GattDiscovered done) {
    enqueue(con, std::make_unique<DiscoverOp>(std::move(done)));
}

void custom(uint16_t con, StartFn start, EventFn on_event, GattDone done) {
    enqueue(con, std::make_unique<CustomOp>(std::move(start), std::move(on_event), std::move(done)));
}

void listen(uint16_t con, uint16_t value_handle, GattNotify cb) {
    g_listeners.emplace(key(con, value_handle), std::move(cb));
}

size_t pending(uint16_t con) {
    Conn *c = conn_for(con);
    return c ? c->queue.size() : 0;
}

void attach(uint16_t con) {
    if (conn_for(con)) return;
    auto c = std::make_unique<Conn>();
    /* All notifications / indications of this connection */
    gatt_client_listen_for_characteristic_value_updates(&c->listener, &packet_handler, con, nullptr);
    c->listening = true;
    g_conns[con] = std::move(c);
}

void detach(uint16_t con) {
    auto it = g_conns.find(con);
    if (it == g_conns.end()) return;
    std::unique_ptr<Conn> c = std::move(it->second);
    g_conns.erase(it);
    if (c->listening) gatt_client_stop_listening_for_characteristic_value_updates(&c->listener);
    if (c->retry_armed) btstack_run_loop_remove_timer(&c->retry_timer);
    for (auto l = g_listeners.begin(); l != g_listeners.end();)
        l = (l->first >> 16) == con ? g_listeners.erase(l) : std::next(l);
    /* Fail what was running or queued */
    while (!c->queue.empty()) {
        std::unique_ptr<Op> op = std::move(c->queue.front());
        c->queue.pop_front();
        op->finish(ATT_ERROR_HCI_DISCONNECT_RECEIVED);
    }
}

void init() {
    g_conns.clear();
    g_listeners.clear();
    g_pumping = false;
}

void deinit() {
    std::vector<uint16_t> cons;
    for (auto &e : g_conns) cons.push_back(e.first);
    for (uint16_t con : cons) detach(con);
    g_listeners.clear();
}

} /* namespace gatt_client */
} /* namespace le_audio */
