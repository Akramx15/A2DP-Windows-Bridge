/*
 * LE Audio (experimental) - core: LE scan, connection, security, streaming
 *
 * le_audio::Core runs inside the BTstack session of BtStackTransport. When the
 * experimental setting is on (set_enabled(true) before BtStackTransport::init)
 * BtStackTransport calls stack_init() on the BTstack thread: SM (LE Secure
 * Connections, Just Works, bonds in le_audio_bonds.tlv in the config folder),
 * the GATT client, the PC's GATT server and the profile modules
 * (le_audio_module.h) are set up. Nothing is advertised; with the setting
 * off none of this runs.
 *
 * Threads:
 *  - "worker" API (scan / connect / start_stream / ...): blocking calls for
 *    the streaming thread of A2dpService or the CLI. They run the work on
 *    the BTstack thread and wait for it with a timeout.
 *  - BTstack thread: everything touching BTstack, LeAudioDevice and the
 *    gatt_client queue. Use call() / run() / post() to get there.
 *  - audio thread: stream().push_pcm() only.
 *
 * Streaming to a group: start_stream(ids) plans the configuration
 * (le_audio_presets.h) for the devices together - e.g. one stereo headset
 * with one 2-channel ASE or two 1-channel ASEs, or two earbuds with one ASE
 * each - then runs the BAP unicast procedure on all of them (le_audio_bap.h):
 * Config Codec, CIG, Config QoS, Enable, CIS, Receiver Start Ready by the
 * sinks, ISO data.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_CORE_H
#define LE_AUDIO_CORE_H

#include "le_audio_device.h"
#include "le_audio_gatt_server.h"
#include "le_audio_presets.h"
#include "le_audio_stream.h"

#include <atomic>
#include <cstdint>
#include <functional>
#include <map>
#include <memory>
#include <mutex>
#include <string>
#include <unordered_set>
#include <vector>

namespace le_audio {

class Unicast;

/* One advertiser seen by scan() */
struct ScanResult {
    LeAddr addr;                      /* as advertised (connect with this) */
    /* addr is an RPA that SM resolved with the IRK of a bond: the bonded
     * device's identity address */
    bool resolved = false;
    LeAddr identity;
    std::string name;
    int8_t rssi = 0;
    uint16_t appearance = 0;
    bool le_audio = false;            /* ASCS / PACS / CAS advertised (or a module said so) */
    bool connectable = true;
    std::vector<uint16_t> uuids16;    /* service UUIDs and service data UUIDs */
    uint16_t ascs_sink_contexts = 0;  /* ASCS service data: available sink contexts */
    bool targeted = false;            /* ASCS / CAS targeted announcement */
    std::vector<uint8_t> rsi;         /* CSIP Resolvable Set Identifier (6 bytes) */
    int set_id = -1;                  /* CSIP module: group of set members */
    std::string note;                 /* module annotation for the UI / log */
    std::vector<uint8_t> adv, scan_rsp;  /* last raw AD */
};

/* The stream for the UI / log */
struct StreamStatus {
    bool active = false;
    StreamPlan plan;
    std::vector<int> device_ids;
    std::string summary;              /* plan.describe() */
};

/* A device as the worker sees it (a copy) */
struct DeviceInfo {
    int id = -1;
    LeAddr addr;
    std::string name;
    bool connected = false;
    bool ready = false;
    uint32_t sink_locations = 0;
    int sink_ases = 0;
    std::string pacs;                 /* short summary of the sink PAC records */
};

class Core {
public:
    static Core &instance();

    /* ---- Configuration (any thread, before BtStackTransport::init) ---- */
    void set_enabled(bool on) { enabled_.store(on); }
    bool enabled() const { return enabled_.load(); }
    /* Folder for le_audio_bonds.tlv (empty: bonds in memory only) */
    void set_storage_dir(const std::string &dir);

    /* ---- BtStackTransport, BTstack thread ---- */
    void stack_init();      /* after l2cap_init(), before HCI power on */
    void stack_deinit();    /* after the run loop stopped */
    /* LE Audio usable: stack_init() ran and HCI is working */
    bool ready() const { return initialized_.load() && hci_working_.load(); }

    /* ---- Worker thread (blocking) ---- */
    using Cancelled = std::function<bool()>;

    /* Run fn on the BTstack thread; it calls done(ok) when finished, maybe
     * later from a BTstack callback. Waits up to timeout_ms (or until
     * cancelled()); false on timeout / cancel / done(false). */
    bool call(std::function<void(std::function<void(bool)> done)> fn, uint32_t timeout_ms,
              const Cancelled &cancelled = {});
    /* Run fn on the BTstack thread and wait for it */
    bool run(std::function<void()> fn, uint32_t timeout_ms = 3000);

    /* LE scan for duration_ms; all advertisers (ScanResult::le_audio marks
     * LE Audio devices) */
    std::vector<ScanResult> scan(uint32_t duration_ms, const Cancelled &cancelled = {});
    /* Scan until addr is seen (to learn its address type / name). addr may
     * be an identity address or an RPA of a bonded device: an advertiser
     * whose RPA resolves to the same identity matches too, and out.addr is
     * the address it advertises now (connect with that). */
    bool find(const uint8_t addr[6], uint32_t timeout_ms, ScanResult &out, const Cancelled &cancelled = {});

    /* Connect, pair / encrypt, discover GATT, read PACS / ASCS. Returns the
     * device id, or -1 (error: why). An already connected device returns at
     * once. addr may be an RPA or the identity of a bonded device: the id is
     * the same for both, and a device using RPAs is connected with the RPA
     * it was last seen advertising (scan() / find()). */
    int connect(const LeAddr &addr, const std::string &name, uint32_t timeout_ms, std::string *error,
                const Cancelled &cancelled = {});
    void disconnect(int device_id, uint32_t timeout_ms = 3000);
    void disconnect_all(uint32_t timeout_ms = 3000);

    /* Plan and set up the unicast stream to the devices (after the modules'
     * prepare_group() / adjust_stream_request()). Audio flows from
     * stream().push_pcm() once this returned true. */
    bool start_stream(std::vector<int> device_ids, StreamRequest request, StreamStatus *status,
                      std::string *error, const Cancelled &cancelled = {});
    /* Disable / release the ASEs, remove the CIG (devices stay connected) */
    void stop_stream(uint32_t timeout_ms = 8000);
    /* True once after a device or CIS of the running stream was lost */
    bool stream_lost() { return stream_lost_.exchange(false); }
    StreamStatus stream_status();
    Stream &stream() { return stream_; }
    std::vector<DeviceInfo> devices();

    /* Headset volume through the modules (VCP), any thread */
    int remote_volume();
    bool request_volume(uint8_t level);

    /* ---- BTstack thread (modules) ---- */
    LeAudioDevice *device(int id);
    LeAudioDevice *device_by_handle(uint16_t acl);
    std::vector<LeAudioDevice *> all_devices();
    GattServer &gatt_server() { return server_; }
    /* Queue fn for the BTstack thread (any thread, no wait) */
    void post(std::function<void()> fn);
    /* ISO buffers of the controller (LE Read Buffer Size v2; any thread).
     * 0: no ISO support, start_stream() refuses (BTstack's ISO send would
     * loop for ever) */
    uint16_t iso_packet_length() const { return iso_len_.load(); }
    uint8_t iso_packet_count() const { return iso_count_.load(); }

    /* ---- Internal (BTstack callbacks) ---- */
    void on_hci_event(uint8_t *packet, uint16_t size);
    void on_sm_event(uint8_t *packet, uint16_t size);
    void on_iso_packet(uint8_t packet_type, uint8_t *packet, uint16_t size);
    void notify_stream_state(bool streaming);
    void set_stream_lost(const char *why);
    void on_ase_update(LeAudioDevice &dev, const Ase &ase);
    void on_cp_response(LeAudioDevice &dev, const uint8_t *value, uint16_t len);

private:
    Core();
    struct PostedFn;
    static void run_posted(void *context);
    /* RPA resolution (BTstack SM, IRKs of le_device_db) */
    struct RpaEntry {
        bool done = false, ok = false;
        LeAddr identity;
        uint32_t seen_ms = 0;               /* last advertising report, 0: never seen */
    };
    static uint64_t addr_key(const uint8_t addr[6]);
    void lookup_rpa(const LeAddr &addr, bool seen);
    bool resolved_identity(const uint8_t addr[6], LeAddr &identity) const;
    LeAddr connect_address(const LeAudioDevice &dev, const LeAddr &given) const;
    void on_identity(LeAudioDevice &dev, const LeAddr &identity);
    LeAudioDevice *device_for_addr(const LeAddr &addr, bool create);
    void on_adv_report(const LeAddr &addr, int8_t rssi, uint8_t event_type, bool extended,
                       const uint8_t *data, uint8_t len);
    void on_le_connected(uint8_t status, uint16_t handle, const LeAddr &addr, uint8_t role);
    void on_disconnected(uint16_t handle, uint8_t reason);
    void on_encryption(uint16_t handle, uint8_t status, bool enabled);
    void maybe_secured(LeAudioDevice &dev);
    void fail_connect(LeAudioDevice &dev, const std::string &why);
    void finish_connect(LeAudioDevice &dev, bool ok);

    std::atomic<bool> enabled_{false};
    std::atomic<bool> initialized_{false};
    std::atomic<bool> hci_working_{false};
    std::string storage_dir_;
    void *tlv_context_ = nullptr;           /* btstack_tlv_windows_t */

    /* BTstack thread state */
    std::vector<std::unique_ptr<LeAudioDevice>> devices_;
    std::vector<ScanResult> scan_results_;
    bool scanning_ = false;
    int connecting_ = -1;                   /* device id of the gap_connect() in progress */
    LeAddr connect_target_;                 /* the address gap_connect() was given */
    std::shared_ptr<int> connect_id_;       /* connect()'s result id (moves when on_identity() merges) */
    std::function<void(bool)> connect_done_;
    std::string connect_error_;
    GattServer server_;
    Stream stream_;
    std::unique_ptr<Unicast> unicast_;
    std::atomic<uint16_t> iso_len_{0};
    std::atomic<uint8_t> iso_count_{0};
    std::map<uint64_t, RpaEntry> rpa_cache_;   /* by RPA */

    /* post(): queued callbacks of this session (owned). stack_deinit() moves
     * the ones still queued to posted_dead_ (their function dropped: they do
     * nothing if run) and stack_init() frees those, once the new run loop
     * no longer knows them. */
    std::mutex posted_mutex_;
    bool posts_open_ = false;
    std::unordered_set<PostedFn *> posted_;
    std::vector<PostedFn *> posted_dead_;

    std::atomic<bool> stream_lost_{false};
    std::mutex status_mutex_;
    StreamStatus status_;
};

} /* namespace le_audio */

#endif /* LE_AUDIO_CORE_H */
