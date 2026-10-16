/*
 * LE Audio (experimental) - profile module hook interface
 *
 * Each LE Audio profile beyond BAP lives in its own module: a class derived
 * from le_audio::Module in its own le_audio_<name>.{h,cpp}, created by a
 * factory function that le_audio_modules.cpp already registers:
 *
 *   le_audio_csip.cpp       CSIP Set Coordinator        csip_module()
 *   le_audio_vcp.cpp        VCP Volume Controller       vcp_module()
 *   le_audio_mcp.cpp        MCP: GMCS server on the PC  mcp_module()
 *   le_audio_ccp.cpp        CCP: GTBS server on the PC  ccp_module()
 *   le_audio_tmap_gmap.cpp  TMAP + GMAP                 tmap_gmap_module()
 *
 * All hooks have empty defaults; override only what the profile needs. The
 * thread each hook runs on is given below. On the BTstack thread a hook must
 * not block; it may call BTstack and the le_audio::gatt_client queue. Worker
 * thread hooks may block (with a timeout) and reach the BTstack thread
 * through Core::call() / Core::run().
 *
 * Modules exist for the whole process; on_stack_init() / on_stack_deinit()
 * mark each BTstack session (a stack can be restarted, e.g. after sleep).
 * Keep per-device state keyed by LeAudioDevice::id and drop it in
 * on_disconnected().
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_MODULE_H
#define LE_AUDIO_MODULE_H

#include "le_audio_device.h"
#include "le_audio_presets.h"

#include <cstdint>
#include <functional>
#include <string>
#include <vector>

namespace le_audio {

class Core;
class GattServer;
struct ScanResult;
struct StreamStatus;

/* Content Control IDs of the PC's content control services: unique on the
 * server, the value of their Content Control ID characteristic (0x2BBA) and
 * what Module::add_ccids() reports for the CCID List of a stream */
constexpr uint8_t CCID_GMCS = 0x01;   /* MCP: Generic Media Control Service */
constexpr uint8_t CCID_GTBS = 0x02;   /* CCP: Generic Telephone Bearer Service */

class Module {
public:
    virtual ~Module() = default;
    virtual const char *name() const = 0;

    /* ---- BTstack thread ---- */

    /* A BTstack session with LE Audio enabled starts: SM, GATT client and
     * the core are up, HCI is not powered on yet. Register presets
     * (add_stream_preset) or BTstack event handlers here. */
    virtual void on_stack_init(Core &core) { (void)core; }
    /* Add the PC's GATT services of this profile (GattServer docs). Runs
     * after on_stack_init(), before att_server_init(). */
    virtual void add_gatt_services(GattServer &server) { (void)server; }
    /* The session ends (the run loop has stopped: no BTstack calls) */
    virtual void on_stack_deinit() {}

    /* Every advertising report during a scan, after the core has merged it
     * into result (raw AD of this report in ad/len). May set
     * result.le_audio (show it as an LE Audio device), result.set_id and
     * result.note (CSIP: RSI of a known set). */
    virtual void on_scan_result(ScanResult &result, const uint8_t *ad, uint16_t len) {
        (void)result; (void)ad; (void)len;
    }

    /* LE link up (not encrypted yet; do not use GATT before on_encrypted) */
    virtual void on_connected(LeAudioDevice &dev) { (void)dev; }
    /* Link encrypted (paired / bonded) */
    virtual void on_encrypted(LeAudioDevice &dev) { (void)dev; }
    /* GATT cache filled and PACS / ASCS read by the core: find this
     * profile's service with dev.find_service() / find_chars() and queue
     * reads, writes and subscriptions with le_audio::gatt_client. connect()
     * returns to the worker right after this hook; queued operations keep
     * running in the background. */
    virtual void on_gatt_ready(LeAudioDevice &dev) { (void)dev; }
    /* Link down (reason: HCI error code). dev still has its last data. */
    virtual void on_disconnected(LeAudioDevice &dev, uint8_t reason) { (void)dev; (void)reason; }

    /* Unicast stream state: streaming = true once all ASEs stream (audio
     * flows), false when it stopped or was lost */
    virtual void on_stream_state(const StreamStatus &status, bool streaming) { (void)status; (void)streaming; }
    /* Enable of a stream with these Streaming Audio Contexts (CONTEXT_*
     * bits): append the CCIDs of this profile's content control services
     * that belong to such a stream (MCP: GMCS for media, CCP: GTBS for
     * calls). The core sends them, without duplicates, as the CCID List
     * metadata (0x05) of every ASE next to the Streaming Audio Contexts. */
    virtual void add_ccids(uint16_t contexts, std::vector<uint8_t> &ccids) { (void)contexts; (void)ccids; }

    /* ---- Worker thread (may block, see Core::call()) ---- */

    /* Before a stream is set up to device_ids (connected and ready). CSIP:
     * find and connect the other members of a coordinated set and append
     * their ids. cancelled() turns true when the user stops. */
    virtual void prepare_group(Core &core, std::vector<int> &device_ids, const std::function<bool()> &cancelled) {
        (void)core; (void)device_ids; (void)cancelled;
    }
    /* Before the configuration is chosen. GMAP: prefer low-latency presets
     * for a gaming headset (request.preset_order, request.contexts). */
    virtual void adjust_stream_request(Core &core, const std::vector<int> &device_ids, StreamRequest &request) {
        (void)core; (void)device_ids; (void)request;
    }

    /* ---- Any thread ---- */

    /* Headset volume on the AVRCP scale 0-127 (VolumeSync keeps the Windows
     * volume equal to it), -1 while unknown. The first module answering
     * >= 0 is used. VCP: Volume State 0-255 mapped to 0-127. */
    virtual int remote_volume() { return -1; }
    /* Set the headset volume (0-127); true if this module handles it */
    virtual bool request_volume(uint8_t level) { (void)level; return false; }
};

/* All registered modules (le_audio_modules.cpp), in a fixed order */
const std::vector<Module *> &modules();

/* Factories of the modules (one per le_audio_<name>.cpp) */
Module *csip_module();
Module *vcp_module();
Module *mcp_module();
Module *ccp_module();
Module *tmap_gmap_module();

} /* namespace le_audio */

#endif /* LE_AUDIO_MODULE_H */
