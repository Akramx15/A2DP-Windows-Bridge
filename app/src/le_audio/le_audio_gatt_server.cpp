/*
 * LE Audio (experimental) - GATT server of the PC (dynamic attribute DB)
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_gatt_server.h"
#include "le_audio_csip.h"
#include "le_audio_defs.h"
#include "debug_log.h"

#include <cstring>
#include <set>

extern "C" {
#include "btstack_config.h"
#include "btstack.h"
#include "btstack_tlv.h"
}

/* att_server.c's default (btstack_config.h may raise it) */
#ifndef NVN_NUM_GATT_SERVER_CCC
#define NVN_NUM_GATT_SERVER_CCC 20
#endif

namespace le_audio {

namespace {
/* Can-send-now registrations, one per connection (stable addresses) */
std::map<uint16_t, btstack_context_callback_registration_t> g_send_regs;
GattServer *g_server = nullptr;
btstack_packet_callback_registration_t g_hci_reg;

void can_send_trampoline(void *context) {
    if (g_server) g_server->can_send_now((uint16_t)(uintptr_t)context);
}

void hci_trampoline(uint8_t packet_type, uint16_t channel, uint8_t *packet, uint16_t size) {
    (void)channel;
    if (packet_type == HCI_EVENT_PACKET && g_server) g_server->on_hci_event(packet, size);
}

uint32_t ckey(uint16_t con, uint16_t handle) { return ((uint32_t)con << 16) | handle; }

/* BTstack's persistent CCC store (att_server.c, private there): one TLV tag
 * 'BTC' + slot per CCC value of a bonded client, and 'BTDB' = the Database
 * Hash they were written for */
struct PersistentCcc {
    uint32_t seq_nr;
    uint16_t att_handle;
    uint8_t value;
    uint8_t device_index;
};
static_assert(sizeof(PersistentCcc) == 8, "att_server.c persistent_ccc_entry_t");
uint32_t tag_ccc(int slot) { return ((uint32_t)'B' << 24) | ((uint32_t)'T' << 16) | ((uint32_t)'C' << 8) | (uint8_t)slot; }
const uint32_t TAG_DB_HASH = ((uint32_t)'B' << 24) | ((uint32_t)'T' << 16) | ((uint32_t)'D' << 8) | 'B';
/* Ours: a Service Changed indication is due for this le_device_db index */
uint32_t tag_service_changed(int index) {
    return ((uint32_t)'A' << 24) | ((uint32_t)'W' << 16) | ((uint32_t)'S' << 8) | (uint8_t)index;
}
} /* namespace */

void GattServer::reset() {
    hci_remove_event_handler(&g_hci_reg);
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
    /* GATT: Service Changed (the DB never changes while connected; indicated
     * to bonded clients after an A2DPWB update changed it), then the
     * Database Hash (static value, filled in by start()). Keep this layout:
     * the handles up to the Service Changed CCC must stay the same in every
     * version, check_stored_client_configs() relies on them. */
    begin_service(UUID_SERVICE_GATT);
    service_changed_ = add_characteristic(UUID_CHR_SERVICE_CHANGED, ATT_PROPERTY_INDICATE, nullptr, nullptr);
    const uint8_t zero[16] = {};
    db_hash_handle_ = att_db_util_add_characteristic_uuid16(ORG_BLUETOOTH_CHARACTERISTIC_DATABASE_HASH,
                                                            ATT_PROPERTY_READ, ATT_SECURITY_NONE, ATT_SECURITY_NONE,
                                                            const_cast<uint8_t *>(zero), sizeof(zero));
    last_handle_ = db_hash_handle_;
}

/* Database Hash (Core Vol 3 Part G 7.3): AES-CMAC with a zero key over the
 * attributes att_db_util selects (service / include / characteristic
 * declarations with their values, descriptors' handle and type), stored least
 * significant octet first as BTstack's compile_gatt.py does. Written into the
 * static value in the DB, where BTstack's att_server also looks for it. */
void GattServer::compute_db_hash() {
    std::vector<uint8_t> msg(att_db_util_hash_len());
    att_db_util_hash_init();
    for (uint8_t &b : msg) b = att_db_util_hash_get_next();
    static const uint8_t zero_key[16] = {};
    uint8_t mac[16];
    csip::aes_cmac(zero_key, msg.data(), msg.size(), mac);
    for (int i = 0; i < 16; i++) db_hash_[i] = mac[15 - i];
    /* The DB: version byte, then per attribute size, flags, handle, UUID, value */
    uint8_t *p = att_db_util_get_address() + 1;
    for (uint16_t size; (size = little_endian_read_16(p, 0)) != 0; p += size) {
        if (little_endian_read_16(p, 4) != db_hash_handle_) continue;
        const uint16_t flags = little_endian_read_16(p, 2);
        const uint16_t uuid_len = (flags & ATT_PROPERTY_UUID128) ? 16 : 2;
        if (size == 6 + uuid_len + 16) memcpy(p + 6 + uuid_len, db_hash_, 16);
        break;
    }
}

/* Before att_server_init(): the CCC values BTstack stored for bonded clients
 * belong to the database with the stored hash. Another database (an A2DPWB
 * update added or moved characteristics, or a version from before the
 * Database Hash, which stored no hash): drop them - BTstack would replay them
 * on the old handles - and remember which clients had Service Changed
 * indications on, to tell them on their next encrypted link. */
void GattServer::check_stored_client_configs() {
    const btstack_tlv_t *tlv = nullptr;
    void *ctx = nullptr;
    btstack_tlv_get_instance(&tlv, &ctx);
    if (!tlv) return;
    uint8_t stored[16];
    const bool have = tlv->get_tag(ctx, TAG_DB_HASH, stored, sizeof(stored)) == (int)sizeof(stored);
    if (have && memcmp(stored, db_hash_, 16) == 0) return;
    std::set<int> clients, notify;
    for (int slot = 0; slot < NVN_NUM_GATT_SERVER_CCC; slot++) {
        PersistentCcc e;
        if (tlv->get_tag(ctx, tag_ccc(slot), (uint8_t *)&e, sizeof(e)) != (int)sizeof(e)) continue;
        clients.insert(e.device_index);
        if (e.att_handle == service_changed_ + 1 && (e.value & GATT_CLIENT_CHARACTERISTICS_CONFIGURATION_INDICATION))
            notify.insert(e.device_index);
        tlv->delete_tag(ctx, tag_ccc(slot));
    }
    tlv->store_tag(ctx, TAG_DB_HASH, db_hash_, sizeof(db_hash_));
    const uint8_t due = 1;
    for (int index : notify) tlv->store_tag(ctx, tag_service_changed(index), &due, 1);
    if (!clients.empty())
        LOG_INFO("LE Audio: GATT database changed since the last session (%s): client configurations of %u "
                 "bonded client(s) dropped, Service Changed due for %u",
                 have ? "other hash" : "no hash stored", (unsigned)clients.size(), (unsigned)notify.size());
}

void GattServer::on_hci_event(const uint8_t *packet, uint16_t size) {
    (void)size;
    const btstack_tlv_t *tlv = nullptr;
    void *ctx = nullptr;
    btstack_tlv_get_instance(&tlv, &ctx);
    if (!tlv) return;
    switch (hci_event_packet_get_type(packet)) {
    case HCI_EVENT_ENCRYPTION_CHANGE:
    case HCI_EVENT_ENCRYPTION_CHANGE_V2: {
        if (!hci_event_encryption_change_get_encryption_enabled(packet)) break;
        const uint16_t con = hci_event_encryption_change_get_connection_handle(packet);
        if (gap_get_connection_type(con) != GAP_CONNECTION_LE) break;
        const int index = sm_le_device_index(con);   /* the bond (identity), -1: not bonded */
        if (index < 0) break;
        uint8_t due = 0;
        if (tlv->get_tag(ctx, tag_service_changed(index), &due, 1) != 1) break;
        tlv->delete_tag(ctx, tag_service_changed(index));
        /* The client had indications on (its CCC was dropped with the old
         * database): the whole range changed */
        ccc_[ckey(con, service_changed_)] |= GATT_CLIENT_CHARACTERISTICS_CONFIGURATION_INDICATION;
        LOG_INFO("LE Audio: GATT database changed since bonded client 0x%04x saw it: Service Changed", con);
        indicate(con, service_changed_, {0x01, 0x00, 0xFF, 0xFF});
        break;
    }
    case HCI_EVENT_META_GAP:
        if (hci_event_gap_meta_get_subevent_code(packet) == GAP_SUBEVENT_BONDING_DELETED)
            tlv->delete_tag(ctx, tag_service_changed(gap_subevent_bonding_deleted_get_index(packet)));
        break;
    default:
        break;
    }
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
    compute_db_hash();
    check_stored_client_configs();
    att_server_init(att_db_util_get_address(), &att_read_trampoline, &att_write_trampoline);
    g_hci_reg.callback = &hci_trampoline;
    hci_add_event_handler(&g_hci_reg);
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
        read_offset_ = offset;
        read_error_ = 0;
        value = it->second.read(con);
        read_offset_ = 0;
        if (read_error_) {
            uint8_t error = read_error_;
            read_error_ = 0;
            return (uint16_t)(ATT_READ_ERROR_CODE_OFFSET + error);
        }
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
    /* BTstack copies at most ATT_MTU-3 bytes but reports the full length as
     * the PDU size, so an over-long value would go out with garbage: cut it
     * (the client reads the full value if it needs it) */
    uint16_t mtu = att_server_get_mtu(con);
    size_t len = p.value.size();
    if (mtu > 3 && len > (size_t)(mtu - 3)) len = mtu - 3;
    uint8_t status = p.indication
        ? att_server_indicate(con, p.handle, p.value.data(), (uint16_t)len)
        : att_server_notify(con, p.handle, p.value.data(), (uint16_t)len);
    if (status != ERROR_CODE_SUCCESS)
        LOG_WARN("LE Audio: GATT %s of 0x%04x failed (0x%02x)", p.indication ? "indication" : "notification",
                 p.handle, status);
    if (!it->second.empty()) send_pending(con);
}

void GattServer::on_disconnected(uint16_t con) {
    /* The live values of this link go; a bonded client gets its CCC values
     * back from BTstack's persistent store when it encrypts again (they are
     * replayed through write_cb()), an unbonded one starts from none */
    pending_.erase(con);
    g_send_regs.erase(con);
    for (auto it = ccc_.begin(); it != ccc_.end();)
        it = (it->first >> 16) == con ? ccc_.erase(it) : std::next(it);
    for (auto it = prepared_.begin(); it != prepared_.end();)
        it = (it->first >> 16) == con ? prepared_.erase(it) : std::next(it);
}

} /* namespace le_audio */
