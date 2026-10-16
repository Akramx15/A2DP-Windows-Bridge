/*
 * LE Audio (experimental) - LC3 encoding and ISO SDU sending
 *
 * The audio source (WASAPI capture or the test tone) calls push_pcm() with
 * the same arguments as an AudioCallback (wasapi_capture.h). The PCM is
 * downmixed to stereo, resampled to the stream's LC3 rate if needed (linear
 * interpolation; WASAPI is normally asked for that rate already), cut into
 * LC3 frames and encoded per output channel (btstack_lc3_google). One "SDU
 * set" (one SDU per CIS for the same 10 ms) goes into a small jitter buffer.
 *
 * On the BTstack thread, HCI_EVENT_CIS_CAN_SEND_NOW takes the next set and
 * sends each CIS its SDU as one HCI ISO data packet: complete SDU (never
 * fragmented: plan_stream() only takes SDUs that fit the controller's ISO
 * data packets), sequence number counting up per CIS, no time stamp (the
 * controller schedules the SDUs by sequence number into its SDU intervals).
 * A round (one set) starts at the first CIS event after every CIS that is
 * still up got its SDU, so a CIS that went down does not stall the others.
 * The controller paces the sending through BTstack's per-CIS queue
 * (hci_set_num_iso_packets_to_queue, queue_depth: Unicast keeps
 * queue_depth x CIS within the controller's ISO buffers); a host-side guard
 * additionally keeps the sent SDUs within queue_depth + 2 intervals of real
 * time. An empty buffer (underrun) sends encoded silence; a full one drops
 * the oldest set.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_STREAM_H
#define LE_AUDIO_STREAM_H

#include "le_audio_presets.h"

#include <atomic>
#include <cstdint>
#include <deque>
#include <memory>
#include <mutex>
#include <vector>

struct btstack_timer_source;

namespace le_audio {

class Stream {
public:
    Stream();
    ~Stream();

    struct Stats {
        uint64_t sets_encoded = 0;     /* SDU sets from captured audio */
        uint64_t sets_sent = 0;        /* SDU sets sent (audio + silence) */
        uint64_t sdus_sent = 0;
        uint64_t silence_sets = 0;     /* sent while the buffer was empty */
        uint64_t underruns = 0;        /* times the buffer ran empty */
        uint64_t overflows = 0;        /* sets dropped, buffer full */
        uint64_t send_errors = 0;
    };

    /* Worker thread, before the audio starts: encoders for the plan */
    bool configure(const StreamPlan &plan);
    void clear();
    uint32_t sample_rate() const { return sample_rate_; }

    /* Audio thread (AudioCallback signature) */
    void push_pcm(const uint8_t *data, uint32_t frames, uint32_t channels, uint32_t sample_rate,
                  uint32_t bits_per_sample);

    /* BTstack thread */
    void start_sending(const std::vector<uint16_t> &cis_handles, uint8_t queue_depth);
    void stop_sending();
    bool sending() const { return sending_; }
    void on_cis_can_send_now(const uint8_t *packet, uint16_t size);
    /* A CIS of the stream disconnected: rounds no longer wait for it */
    void on_cis_down(uint16_t handle);

    Stats stats() const;

private:
    void encode_frame();                       /* audio thread: frame_ buffer full */
    void request_next();                       /* BTstack thread */
    void request_events();
    bool round_done() const;
    static void timer_handler(struct btstack_timer_source *ts);
    void send_sdu(size_t index, uint16_t handle);

    struct Encoder;                            /* btstack_lc3_encoder_google_t + instance */
    std::vector<std::unique_ptr<Encoder>> encoders_;  /* one per output channel */
    struct ChannelRef { size_t cis; size_t pos; ChannelSource source; };
    std::vector<ChannelRef> channels_;         /* output channel -> CIS / position */
    std::vector<uint16_t> sdu_len_;            /* per CIS */
    uint32_t sample_rate_ = 0;
    uint16_t frame_us_ = 10000;
    uint16_t octets_ = 0;
    uint32_t samples_per_frame_ = 0;

    /* Audio thread state (enc_mutex_: configure() / clear() vs push_pcm()) */
    std::mutex enc_mutex_;
    std::vector<float> frame_l_, frame_r_;
    uint32_t frame_fill_ = 0;
    double resample_pos_ = 0.0;                /* fractional input position */
    float last_l_ = 0.0f, last_r_ = 0.0f;
    uint32_t last_in_rate_ = 0;
    std::vector<int16_t> pcm16_;

    /* Jitter buffer of SDU sets */
    using SduSet = std::vector<std::vector<uint8_t>>;
    mutable std::mutex mutex_;
    std::deque<SduSet> sets_;
    bool buffering_ = true;
    SduSet silence_;
    size_t prefill_ = 5;                       /* sets buffered before audio starts / after an underrun */
    static constexpr size_t CAPACITY = 16;

    /* BTstack thread state */
    bool sending_ = false;
    std::vector<uint16_t> cis_handles_;
    std::vector<uint16_t> seq_;
    std::vector<bool> alive_;                  /* per CIS: not reported down */
    std::vector<bool> served_;                 /* per CIS: has its SDU of the current round */
    bool round_open_ = false;
    SduSet current_;
    uint32_t t0_ms_ = 0;
    uint64_t rounds_ = 0;
    uint8_t lead_ = 4;
    void *timer_ = nullptr;                    /* btstack_timer_source_t */
    bool timer_armed_ = false;

    Stats stats_;
};

} /* namespace le_audio */

#endif /* LE_AUDIO_STREAM_H */
