/*
 * LE Audio (experimental) - BAP unicast client (audio source to sink ASEs)
 *
 * read_device(): PACS client (Sink PAC records, Sink Audio Locations,
 * Available / Supported Audio Contexts) and ASCS client (Sink / Source ASEs,
 * ASE Control Point), with notifications enabled, once per connection.
 *
 * Unicast: the BAP unicast procedure for a StreamPlan over one or more
 * devices, as a state machine on the BTstack thread:
 *
 *   Config Codec (all planned ASEs)  -> Codec Configured
 *   LE Set CIG Parameters (gap_cig_create, one CIG for the group)
 *   Config QoS                       -> QoS Configured
 *   Enable (Streaming Audio Contexts)-> Enabling
 *   LE Create CIS (gap_cis_create, all CIS at once) + ISO data path (BTstack)
 *   Sink ASEs: the server's Receiver Start Ready -> Streaming
 *   ISO SDUs (Stream)
 *
 * and back: Disable -> QoS Configured, Release -> Idle, LE Remove CIG
 * (gap_cig_remove disconnects the CIS first). Each step has a timeout; a
 * failing step tears down what was set up and reports why.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_BAP_H
#define LE_AUDIO_BAP_H

#include "le_audio_device.h"
#include "le_audio_presets.h"

#include <functional>
#include <memory>
#include <string>

namespace le_audio {

class Core;

namespace bap {
/* Read PACS and ASCS of dev and subscribe to their notifications
 * (BTstack thread). done(true) also when the device has neither: then
 * has_pacs / has_ascs stay false. */
void read_device(Core &core, LeAudioDevice &dev, std::function<void(bool)> done);
} /* namespace bap */

class Unicast {
public:
    explicit Unicast(Core &core);
    ~Unicast();

    enum class Phase {
        Idle, ConfigCodec, CreateCig, ConfigQos, Enable, CreateCis, WaitStreaming, Streaming,
        Disable, Release, RemoveCig
    };
    static const char *phase_name(Phase p);

    /* BTstack thread */
    void start(const StreamPlan &plan, std::function<void(bool ok, const std::string &why)> done);
    void stop(std::function<void()> done);
    Phase phase() const { return phase_; }
    const StreamPlan &plan() const { return plan_; }
    bool streaming() const { return phase_ == Phase::Streaming; }
    bool busy() const { return phase_ != Phase::Idle; }
    /* Does the stream use this ACL or CIS handle? */
    bool uses_handle(uint16_t handle) const;

    /* Events (from Core / the GATT listeners) */
    void on_ase_update(LeAudioDevice &dev, const Ase &ase);
    void on_cp_response(LeAudioDevice &dev, const uint8_t *value, uint16_t len);
    void on_cig_created(const uint8_t *packet, uint16_t size);
    void on_cis_created(const uint8_t *packet, uint16_t size);
    void on_cig_removed();
    void on_link_lost(uint16_t handle);

private:
    void enter(Phase p);
    void advance();
    void fail(const std::string &why);
    void try_remove_cig();
    void finish_stop();
    void on_timeout();

    struct Impl;
    std::unique_ptr<Impl> impl_;
    Core &core_;
    Phase phase_ = Phase::Idle;
    StreamPlan plan_;
};

} /* namespace le_audio */

#endif /* LE_AUDIO_BAP_H */
