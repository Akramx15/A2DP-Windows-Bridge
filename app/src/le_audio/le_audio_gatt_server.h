/*
 * LE Audio (experimental) - GATT server of the PC (dynamic attribute DB)
 *
 * The PC is the GATT server for the services a headset reads from the audio
 * source: GAP and GATT (here), and the profile modules' services such as
 * GMCS (MCP), GTBS (CCP), TMAS (TMAP) and GMAS (GMAP). The database is built
 * with BTstack's att_db_util while the stack starts:
 *
 *   void MyModule::add_gatt_services(GattServer &server) {
 *       server.begin_service(UUID_SERVICE_GMCS);
 *       name_handle_ = server.add_characteristic(0x2B93, ATT_PROPERTY_READ | ATT_PROPERTY_NOTIFY,
 *           [](uint16_t con) { return bytes_of("A2DPWB"); },   // read: whole value
 *           nullptr);                                           // not writable
 *       cp_handle_ = server.add_characteristic(0x2BA4, ATT_PROPERTY_WRITE | ATT_PROPERTY_NOTIFY,
 *           nullptr,
 *           [this](uint16_t con, const uint8_t *v, uint16_t n) { return on_cp(con, v, n); });
 *       server.end_service();
 *   }
 *
 * Reads and writes are dispatched by attribute handle to the callbacks given
 * here. The CCC descriptor of a NOTIFY / INDICATE characteristic is created
 * (value handle + 1) and handled by the server: notify() only sends to
 * clients that enabled it. Long reads (offsets) and long / prepared writes
 * are handled here; callbacks always see the whole value.
 *
 * add_gatt_services() runs on the BTstack thread before att_server_init();
 * services cannot be added later. notify() / indicate() must be called on
 * the BTstack thread; they queue until ATT can send.
 *
 * Bonded clients: the CCC values a bonded client wrote persist across
 * connections and sessions. BTstack's att_server does that (it stores every
 * CCC write of a bonded client in the bond TLV, keyed by the le_device_db
 * index = the identity, and replays them through write_cb() when the link is
 * encrypted again); the server keeps only the live values per connection
 * (ccc_), and those of an unbonded client are gone with its link. The GATT
 * service has Service Changed and a Database Hash (Core Vol 3 Part G 7.3,
 * computed in start()). When the database differs from the one the stored CCC
 * values were written for (another A2DPWB version), start() drops them and
 * a bonded client that had Service Changed indications enabled gets a
 * Service Changed indication (whole range) on its next encrypted link, so it
 * discovers the services again instead of using cached handles.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_GATT_SERVER_H
#define LE_AUDIO_GATT_SERVER_H

#include <cstdint>
#include <functional>
#include <map>
#include <vector>

namespace le_audio {

class GattServer {
public:
    /* Whole value for a read by con */
    using ReadFn = std::function<std::vector<uint8_t>(uint16_t con)>;
    /* Write of the whole value; returns 0 or an ATT error code */
    using WriteFn = std::function<uint8_t(uint16_t con, const uint8_t *value, uint16_t len)>;

    /* ---- Building (add_gatt_services() hooks) ---- */
    uint16_t begin_service(uint16_t uuid16, bool primary = true);
    uint16_t begin_service128(const uint8_t uuid128[16], bool primary = true);
    /* Include an already added service (its start / end handle) */
    void include_service(uint16_t start_handle, uint16_t end_handle, uint16_t uuid16);
    /* Returns the value handle. properties: ATT_PROPERTY_READ / WRITE /
     * WRITE_WITHOUT_RESPONSE / NOTIFY / INDICATE. read / write may be empty
     * (an unreadable / unwritable characteristic). read_security /
     * write_security: ATT_SECURITY_NONE / ENCRYPTED / ... */
    uint16_t add_characteristic(uint16_t uuid16, uint16_t properties, ReadFn read, WriteFn write,
                                uint8_t read_security = 0, uint8_t write_security = 0);
    uint16_t add_characteristic128(const uint8_t uuid128[16], uint16_t properties, ReadFn read, WriteFn write,
                                   uint8_t read_security = 0, uint8_t write_security = 0);
    /* Returns the end handle of the service */
    uint16_t end_service();

    /* ---- Runtime (BTstack thread) ---- */
    /* Send to con if it enabled notifications (indications) of value_handle;
     * false if it did not or the link is gone */
    bool notify(uint16_t con, uint16_t value_handle, const std::vector<uint8_t> &value);
    bool indicate(uint16_t con, uint16_t value_handle, const std::vector<uint8_t> &value);
    /* To every connected client that enabled it */
    void notify_all(uint16_t value_handle, const std::vector<uint8_t> &value);
    /* CCC value con wrote for value_handle (0 = none) */
    uint16_t client_config(uint16_t con, uint16_t value_handle) const;
    /* Connections that have written any CCC (for notify_all) */
    std::vector<uint16_t> clients() const;
    /* Inside a ReadFn: offset of the read being served (> 0: Read Blob, the
     * rest of a long read) */
    uint16_t read_offset() const { return read_offset_; }
    /* Inside a ReadFn: answer this read with an ATT error instead (e.g. a
     * profile's "Value Changed During Read Long", 0x80) */
    void fail_read(uint8_t att_error) { read_error_ = att_error; }

    /* ---- Core only ---- */
    void reset();            /* new stack: empty DB */
    void build_base();       /* GAP + GATT services */
    void start();            /* att_server_init() with the DB built */
    void on_disconnected(uint16_t con);
    uint16_t read_cb(uint16_t con, uint16_t handle, uint16_t offset, uint8_t *buffer, uint16_t buffer_size);
    int write_cb(uint16_t con, uint16_t handle, uint16_t mode, uint16_t offset, uint8_t *buffer, uint16_t size);
    void can_send_now(uint16_t con);
    void on_hci_event(const uint8_t *packet, uint16_t size);

private:
    struct Attr {
        ReadFn read;
        WriteFn write;
        uint16_t value_handle = 0;   /* for a CCC: its characteristic value */
        bool ccc = false;
        uint16_t properties = 0;
    };
    struct Pending {
        uint16_t handle;
        bool indication;
        std::vector<uint8_t> value;
    };
    std::map<uint16_t, Attr> attrs_;                      /* by attribute handle */
    std::map<uint32_t, uint16_t> ccc_;                    /* con << 16 | value handle -> CCC */
    std::map<uint16_t, std::vector<Pending>> pending_;    /* per connection */
    std::map<uint32_t, std::vector<uint8_t>> prepared_;   /* long writes in progress */
    bool started_ = false;
    uint16_t last_handle_ = 0;
    uint16_t read_offset_ = 0;
    uint8_t read_error_ = 0;
    uint16_t service_changed_ = 0;                        /* value handle (CCC: + 1) */
    uint16_t db_hash_handle_ = 0;                         /* Database Hash value handle */
    uint8_t db_hash_[16] = {};                            /* as sent: least significant octet first */
    void compute_db_hash();
    void check_stored_client_configs();
    void send_pending(uint16_t con);
    bool queue(uint16_t con, uint16_t handle, bool indication, const std::vector<uint8_t> &value);
};

} /* namespace le_audio */

#endif /* LE_AUDIO_GATT_SERVER_H */
