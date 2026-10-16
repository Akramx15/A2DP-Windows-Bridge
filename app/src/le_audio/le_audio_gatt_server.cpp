/*
 * LE Audio (experimental) - GATT server of the PC (dynamic attribute DB)
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_gatt_server.h"
#include "le_audio_defs.h"
#include "debug_log.h"

#include <cstring>

extern "C" {
#include "btstack_config.h"
#include "btstack.h"
}

namespace le_audio {

namespace {
/* Can-send-now registrations, one per connection (stable addresses) */
std::map<uint16_t, btstack_context_callback_registration_t> g_send_regs;
GattServer *g_server = nullptr;

void can_send_trampoline(void *context) {
    if (g_server) g_server->can_send_now((uint16_t)(uintptr_t)context);
}

uint32_t ckey(uint16_t con, uint16_t handle) { return ((uint32_t)con << 16) | handle; }
} /* namespace */

void GattServer::reset() {
    attrs_.clear();
    ccc_.clear();
    pending_.clear();
    prepared_.clear();
    g_send_regs.clear();
    started_ = false;
    last_handle_ = 0;
    g_server = this;
    att_db_util_init();
}

uint16_t GattServer::begin_service(uint16_t uuid16, bool primary) {
    last_handle_ = primary ? att_db_util_add_service_uuid16(uuid16) : att_db_util_add_secondary_service_uuid16(uuid16);
    return last_handle_;
}

uint16_t GattServer::begin_service128(const uint8_t uuid128[16], bool primary) {
    last_handle_ = primary ? att_db_util_add_service_uuid128(uuid128)
                           : att_db_util_add_secondary_service_uuid128(uuid128);
    return last_handle_;
}

void GattServer::include_service(uint16_t start_handle, uint16_t end_handle, uint16_t uuid16) {
    last_handle_ = att_db_util_add_included_service_uuid16(start_handle, end_handle, uuid16);
}

uint16_t GattServer::add_characteristic(uint16_t uuid16, uint16_t properties, ReadFn read, WriteFn write,
                                        uint8_t read_security, uint8_t write_security) {
    uint16_t handle = att_db_util_add_characteristic_uuid16(uuid16, properties | ATT_PROPERTY_DYNAMIC,
                                                            read_security, write_security, nullptr, 0);
    Attr a;
    a.read = std::move(read);
    a.write = std::move(write);
    a.value_handle = handle;
    a.properties = properties;
    attrs_[handle] = std::move(a);
    last_handle_ = handle;
    if (properties & (ATT_PROPERTY_NOTIFY | ATT_PROPERTY_INDICATE)) {
        Attr c;
        c.ccc = true;
        c.value_handle = handle;
        attrs_[(uint16_t)(handle + 1)] = std::move(c);
        last_handle_ = (uint16_t)(handle + 1);
    }
    return handle;
}

uint16_t GattServer::add_characteristic128(const uint8_t uuid128[16], uint16_t properties, ReadFn read,
                                           WriteFn write, uint8_t read_security, uint8_t write_security) {
    uint16_t handle = att_db_util_add_characteristic_uuid128(uuid128, properties | ATT_PROPERTY_DYNAMIC,
                                                             read_security, write_security, nullptr, 0);
    Attr a;
    a.read = std::move(read);
    a.write = std::move(write);
    a.value_handle = handle;
    a.properties = properties;
    attrs_[handle] = std::move(a);
    last_handle_ = handle;
    if (properties & (ATT_PROPERTY_NOTIFY | ATT_PROPERTY_INDICATE)) {
        Attr c;
        c.ccc = true;
        c.value_handle = handle;
        attrs_[(uint16_t)(handle + 1)] = std::move(c);
        last_handle_ = (uint16_t)(handle + 1);
    }
    return handle;
}

uint16_t GattServer::end_service() {
    /* att_db_util has no explicit end: the next service starts the next group */
    return last_handle_;
}

void GattServer::build_base() {
    /* GAP: device name and appearance (Generic Computer) */
    begin_service(UUID_SERVICE_GAP);
    add_characteristic(UUID_CHR_DEVICE_NAME, ATT_PROPERTY_READ,
                       [](uint16_t) { const char *n = "A2DPWB"; return std::vector<uint8_t>(n, n + 6); },
                       nullptr);
    add_characteristic(UUID_CHR_APPEARANCE, ATT_PROPERTY_READ,
                       [](uint16_t) { return std::vector<uint8_t>{0x80, 0x00}; }, nullptr);
    /* GATT: Service Changed (the DB never changes while connected) */
    begin_service(UUID_SERVICE_GATT);
    add_characteristic(UUID_CHR_SERVICE_CHANGED, ATT_PROPERTY_INDICATE, nullptr, nullptr);
}

static uint16_t att_read_trampoline(hci_con_handle_t con, uint16_t handle, uint16_t offset,
                                    uint8_t *buffer, uint16_t buffer_size) {
    return g_server ? g_server->read_cb(con, handle, offset, buffer, buffer_size) : 0;
}

static int att_write_trampoline(hci_con_handle_t con, uint16_t handle, uint16_t mode, uint16_t offset,
                                uint8_t *buffer, uint16_t size) {
    return g_server ? g_server->write_cb(con, handle, mode, offset, buffer, size) : 0;
}

void GattServer::start() {
    g_server = this;
    att_server_init(att_db_util_get_address(), &att_read_trampoline, &att_write_trampoline);
    started_ = true;
    LOG_INFO("LE Audio: GATT server started (%u attributes with callbacks, %u bytes DB)",
             (unsigned)attrs_.size(), (unsigned)att_db_util_get_size());
}

uint16_t GattServer::read_cb(uint16_t con, uint16_t handle, uint16_t offset, uint8_t *buffer,
                             uint16_t buffer_size) {
    auto it = attrs_.find(handle);
    if (it == attrs_.end()) return 0;
    std::vector<uint8_t> value;
    if (it->second.ccc) {
        put16(value, client_config(con, it->second.value_handle));
    } else if (it->second.read) {
        value = it->second.read(con);
    }
    return att_read_callback_handle_blob(value.data(), (uint16_t)value.size(), offset, buffer, buffer_size);
}

int GattServer::write_cb(uint16_t con, uint16_t handle, uint16_t mode, uint16_t offset, uint8_t *buffer,
                         uint16_t size) {
    auto it = attrs_.find(handle);
    if (it == attrs_.end()) return 0;
    uint32_t k = ckey(con, handle);
    switch (mode) {
    case ATT_TRANSACTION_MODE_ACTIVE: {
        std::vector<uint8_t> &v = prepared_[k];
        if (v.size() < (size_t)offset + size) v.resize((size_t)offset + size);
        memcpy(v.data() + offset, buffer, size);
        return 0;
    }
    case ATT_TRANSACTION_MODE_VALIDATE:
        return 0;
    case ATT_TRANSACTION_MODE_CANCEL:
        prepared_.erase(k);
        return 0;
    case ATT_TRANSACTION_MODE_EXECUTE: {
        auto p = prepared_.find(k);
        if (p == prepared_.end()) return 0;
        std::vector<uint8_t> v = std::move(p->second);
        prepared_.erase(p);
        return write_cb(con, handle, ATT_TRANSACTION_MODE_NONE, 0, v.data(), (uint16_t)v.size());
    }
    default:
        break;
    }
    if (offset != 0) return ATT_ERROR_INVALID_OFFSET;
    if (it->second.ccc) {
        if (size < 2) return ATT_ERROR_INVALID_ATTRIBUTE_VALUE_LENGTH;
        uint16_t cfg = rd16(buffer);
        ccc_[ckey(con, it->second.value_handle)] = cfg;
        return 0;
    }
    if (!it->second.write) return ATT_ERROR_WRITE_NOT_PERMITTED;
    return it->second.write(con, buffer, size);
}

uint16_t GattServer::client_config(uint16_t con, uint16_t value_handle) const {
    auto it = ccc_.find(ckey(con, value_handle));
    return it == ccc_.end() ? 0 : it->second;
}

std::vector<uint16_t> GattServer::clients() const {
    std::vector<uint16_t> out;
    for (const auto &e : ccc_) {
        uint16_t con = (uint16_t)(e.first >> 16);
        if (out.empty() || out.back() != con) out.push_back(con);
    }
    return out;
}

bool GattServer::queue(uint16_t con, uint16_t handle, bool indication, const std::vector<uint8_t> &value) {
    if (!started_) return false;
    uint16_t cfg = client_config(con, handle);
    if (!(cfg & (indication ? GATT_CLIENT_CHARACTERISTICS_CONFIGURATION_INDICATION
                            : GATT_CLIENT_CHARACTERISTICS_CONFIGURATION_NOTIFICATION)))
        return false;
    pending_[con].push_back({handle, indication, value});
    send_pending(con);
    return true;
}

bool GattServer::notify(uint16_t con, uint16_t value_handle, const std::vector<uint8_t> &value) {
    return queue(con, value_handle, false, value);
}

bool GattServer::indicate(uint16_t con, uint16_t value_handle, const std::vector<uint8_t> &value) {
    return queue(con, value_handle, true, value);
}

void GattServer::notify_all(uint16_t value_handle, const std::vector<uint8_t> &value) {
    for (uint16_t con : clients()) notify(con, value_handle, value);
}

void GattServer::send_pending(uint16_t con) {
    auto it = pending_.find(con);
    if (it == pending_.end() || it->second.empty()) return;
    btstack_context_callback_registration_t &reg = g_send_regs[con];
    reg.callback = &can_send_trampoline;
    reg.context = (void *)(uintptr_t)con;
    const Pending &p = it->second.front();
    uint8_t status = p.indication ? att_server_request_to_send_indication(&reg, con)
                                  : att_server_request_to_send_notification(&reg, con);
    if (status != ERROR_CODE_SUCCESS && status != ERROR_CODE_COMMAND_DISALLOWED) {
        /* COMMAND_DISALLOWED: already registered, the callback comes */
        LOG_WARN("LE Audio: GATT server cannot send on 0x%04x (0x%02x), dropping", con, status);
        it->second.clear();
    }
}

void GattServer::can_send_now(uint16_t con) {
    auto it = pending_.find(con);
    if (it == pending_.end() || it->second.empty()) return;
    Pending p = std::move(it->second.front());
    it->second.erase(it->second.begin());
    uint8_t status = p.indication
        ? att_server_indicate(con, p.handle, p.value.data(), (uint16_t)p.value.size())
        : att_server_notify(con, p.handle, p.value.data(), (uint16_t)p.value.size());
    if (status != ERROR_CODE_SUCCESS)
        LOG_WARN("LE Audio: GATT %s of 0x%04x failed (0x%02x)", p.indication ? "indication" : "notification",
                 p.handle, status);
    if (!it->second.empty()) send_pending(con);
}

void GattServer::on_disconnected(uint16_t con) {
    pending_.erase(con);
    g_send_regs.erase(con);
    for (auto it = ccc_.begin(); it != ccc_.end();)
        it = (it->first >> 16) == con ? ccc_.erase(it) : std::next(it);
    for (auto it = prepared_.begin(); it != prepared_.end();)
        it = (it->first >> 16) == con ? prepared_.erase(it) : std::next(it);
}

} /* namespace le_audio */
