/*
 * LE Audio (experimental) - LC3 encoding and ISO SDU sending
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_stream.h"
#include "debug_log.h"

#include <algorithm>
#include <cmath>
#include <cstring>
#include <windows.h>
#include <mmsystem.h>

extern "C" {
#include "btstack_config.h"
#include "btstack.h"
#include "btstack_lc3_google.h"
}

namespace le_audio {

struct Stream::Encoder {
    btstack_lc3_encoder_google_t context;
    const btstack_lc3_encoder_t *api = nullptr;
};

Stream::Stream() {
    timer_ = new btstack_timer_source_t();
}

Stream::~Stream() {
    delete static_cast<btstack_timer_source_t *>(timer_);
}

void Stream::clear() {
    std::lock_guard<std::mutex> enc_lock(enc_mutex_);
    std::lock_guard<std::mutex> lock(mutex_);
    encoders_.clear();
    channels_.clear();
    sdu_len_.clear();
    sets_.clear();
    silence_.clear();
    sample_rate_ = 0;
    octets_ = 0;
    frame_fill_ = 0;
    stats_ = Stats();
}

bool Stream::configure(const StreamPlan &plan) {
    clear();
    std::lock_guard<std::mutex> enc_lock(enc_mutex_);
    std::lock_guard<std::mutex> lock(mutex_);
    sample_rate_ = plan.preset.sampling_hz;
    frame_us_ = plan.preset.frame_us;
    octets_ = plan.preset.octets;
    btstack_lc3_frame_duration_t fd = frame_us_ == 7500 ? BTSTACK_LC3_FRAME_DURATION_7500US
                                                        : BTSTACK_LC3_FRAME_DURATION_10000US;
    samples_per_frame_ = btstack_lc3_samples_per_frame(sample_rate_, fd);
    for (size_t c = 0; c < plan.cis.size(); c++) {
        const CisPlan &cis = plan.cis[c];
        sdu_len_.push_back(cis.sdu_len(octets_));
        for (size_t pos = 0; pos < cis.channels.size(); pos++) {
            auto enc = std::make_unique<Encoder>();
            enc->api = btstack_lc3_encoder_google_init_instance(&enc->context);
            if (enc->api->configure(&enc->context, sample_rate_, fd, octets_) != ERROR_CODE_SUCCESS) {
                LOG_ERROR("LE Audio: LC3 encoder configuration failed (%u Hz, %u octets)", sample_rate_, octets_);
                return false;
            }
            encoders_.push_back(std::move(enc));
            channels_.push_back({c, pos, cis.channels[pos].source});
        }
    }
    frame_l_.assign(samples_per_frame_, 0.0f);
    frame_r_.assign(samples_per_frame_, 0.0f);
    pcm16_.assign(samples_per_frame_, 0);
    frame_fill_ = 0;
    resample_pos_ = 0.0;
    last_in_rate_ = 0;
    buffering_ = true;

    /* Silence: encoded once with separate encoder instances */
    silence_.clear();
    for (uint16_t len : sdu_len_) silence_.push_back(std::vector<uint8_t>(len, 0));
    std::vector<int16_t> zeros(samples_per_frame_, 0);
    for (const ChannelRef &ch : channels_) {
        Encoder enc;
        enc.api = btstack_lc3_encoder_google_init_instance(&enc.context);
        enc.api->configure(&enc.context, sample_rate_, fd, octets_);
        enc.api->encode_signed_16(&enc.context, zeros.data(), 1, silence_[ch.cis].data() + ch.pos * octets_);
    }
    LOG_INFO("LE Audio: LC3 encoders ready: %u Hz, %u us frames (%u samples), %u octets, %u channel(s)",
             sample_rate_, frame_us_, samples_per_frame_, octets_, (unsigned)channels_.size());
    return true;
}

void Stream::push_pcm(const uint8_t *data, uint32_t frames, uint32_t channels, uint32_t rate,
                      uint32_t bits_per_sample) {
    std::lock_guard<std::mutex> enc_lock(enc_mutex_);
    if (encoders_.empty() || channels == 0 || rate == 0) return;
    if (bits_per_sample != 32 && bits_per_sample != 16) return;
    if (rate != last_in_rate_) {
        if (last_in_rate_ != 0 || rate != sample_rate_)
            LOG_INFO("LE Audio: capture at %u Hz, LC3 at %u Hz%s", rate, sample_rate_,
                     rate != sample_rate_ ? " (resampling)" : "");
        last_in_rate_ = rate;
        resample_pos_ = 0.0;
    }
    const float *f32 = reinterpret_cast<const float *>(data);
    const int16_t *i16 = reinterpret_cast<const int16_t *>(data);
    auto sample = [&](uint32_t frame, uint32_t ch) -> float {
        size_t i = (size_t)frame * channels + ch;
        return bits_per_sample == 32 ? f32[i] : i16[i] / 32768.0f;
    };
    const double step = (double)rate / (double)sample_rate_;
    for (uint32_t f = 0; f < frames; f++) {
        /* Stereo downmix (ITU-R BS.775 as the A2DP path) */
        float l, r;
        if (channels == 1) {
            l = r = sample(f, 0);
        } else {
            l = sample(f, 0);
            r = sample(f, 1);
            if (channels > 2) {
                float c = sample(f, 2);
                float lfe = channels > 3 ? sample(f, 3) : 0.0f;
                float bl = channels > 4 ? sample(f, 4) : 0.0f, br = channels > 5 ? sample(f, 5) : 0.0f;
                float sl = channels > 6 ? sample(f, 6) : 0.0f, sr = channels > 7 ? sample(f, 7) : 0.0f;
                l += 0.707107f * c + 0.707107f * (bl + sl) + 0.5f * lfe;
                r += 0.707107f * c + 0.707107f * (br + sr) + 0.5f * lfe;
            }
        }
        if (rate == sample_rate_) {
            frame_l_[frame_fill_] = l;
            frame_r_[frame_fill_] = r;
            if (++frame_fill_ == samples_per_frame_) encode_frame();
        } else {
            /* Output samples between the previous input sample (pos 0) and
             * this one (pos 1) */
            while (resample_pos_ < 1.0) {
                float a = (float)resample_pos_;
                frame_l_[frame_fill_] = last_l_ + (l - last_l_) * a;
                frame_r_[frame_fill_] = last_r_ + (r - last_r_) * a;
                if (++frame_fill_ == samples_per_frame_) encode_frame();
                resample_pos_ += step;
            }
            resample_pos_ -= 1.0;
        }
        last_l_ = l;
        last_r_ = r;
    }
}

static int16_t to_i16(float x) {
    if (x > 1.0f) x = 1.0f;
    if (x < -1.0f) x = -1.0f;
    return (int16_t)lrintf(x * 32767.0f);
}

void Stream::encode_frame() {
    frame_fill_ = 0;
    SduSet set;
    for (uint16_t len : sdu_len_) set.push_back(std::vector<uint8_t>(len, 0));
    for (size_t i = 0; i < channels_.size(); i++) {
        const ChannelRef &ch = channels_[i];
        for (uint32_t s = 0; s < samples_per_frame_; s++) {
            float v = ch.source == ChannelSource::Left ? frame_l_[s]
                    : ch.source == ChannelSource::Right ? frame_r_[s]
                    : 0.5f * (frame_l_[s] + frame_r_[s]);
            pcm16_[s] = to_i16(v);
        }
        Encoder &e = *encoders_[i];
        e.api->encode_signed_16(&e.context, pcm16_.data(), 1, set[ch.cis].data() + ch.pos * octets_);
    }
    std::lock_guard<std::mutex> lock(mutex_);
    stats_.sets_encoded++;
    if (sets_.size() >= CAPACITY) {
        sets_.pop_front();
        stats_.overflows++;
    }
    sets_.push_back(std::move(set));
}

/* ---- BTstack thread ---- */

void Stream::start_sending(const std::vector<uint16_t> &cis_handles, uint8_t queue_depth) {
    cis_handles_ = cis_handles;
    seq_.assign(cis_handles.size(), 0);
    alive_.assign(cis_handles.size(), true);
    served_.assign(cis_handles.size(), false);
    round_open_ = false;
    rounds_ = 0;
    t0_ms_ = 0;
    lead_ = (uint8_t)(queue_depth + 2);
    {
        std::lock_guard<std::mutex> lock(mutex_);
        buffering_ = true;
        /* The controller queue takes queue_depth SDU sets at once and the
         * host may run lead_ sets ahead of real time: buffer more than that */
        prefill_ = (size_t)queue_depth + 3;
    }
    /* 1 ms timer resolution for the pacing timer (as WASAPI capture does) */
    timeBeginPeriod(1);
    sending_ = true;
    LOG_INFO("LE Audio: sending ISO SDUs on %u CIS (queue %u)", (unsigned)cis_handles.size(), queue_depth);
    request_next();
}

void Stream::stop_sending() {
    if (!sending_) return;
    sending_ = false;
    timeEndPeriod(1);
    if (timer_armed_) {
        btstack_run_loop_remove_timer(static_cast<btstack_timer_source_t *>(timer_));
        timer_armed_ = false;
    }
    Stats s = stats();
    LOG_INFO("LE Audio: stopped sending: %llu SDU sets (%llu SDUs), %llu silence, %llu underruns, %llu dropped",
             (unsigned long long)s.sets_sent, (unsigned long long)s.sdus_sent,
             (unsigned long long)s.silence_sets, (unsigned long long)s.underruns,
             (unsigned long long)s.overflows);
}

void Stream::timer_handler(struct btstack_timer_source *t) {
    auto *self = static_cast<Stream *>(btstack_run_loop_get_timer_context(t));
    self->timer_armed_ = false;
    if (self->sending_) self->request_events();
}

void Stream::request_events() {
    /* BTstack requests CAN_SEND_NOW for every CIS of the CIG from any of
     * its handles: take one that is still up */
    for (size_t i = 0; i < cis_handles_.size(); i++)
        if (alive_[i]) {
            hci_request_cis_can_send_now_events(cis_handles_[i]);
            return;
        }
}

void Stream::on_cis_down(uint16_t handle) {
    if (!sending_) return;
    bool any = false;
    for (size_t i = 0; i < cis_handles_.size(); i++) {
        if (cis_handles_[i] == handle) alive_[i] = false;
        any |= alive_[i];
    }
    /* The round may have waited for this CIS only */
    if (any && round_open_ && round_done()) {
        round_open_ = false;
        request_next();
    }
}

bool Stream::round_done() const {
    for (size_t i = 0; i < cis_handles_.size(); i++)
        if (alive_[i] && !served_[i]) return false;
    return true;
}

void Stream::request_next() {
    if (!sending_ || cis_handles_.empty()) return;
    uint32_t now = btstack_run_loop_get_time_ms();
    if (t0_ms_ == 0) t0_ms_ = now ? now : 1;
    uint32_t interval_ms = frame_us_ / 1000u ? frame_us_ / 1000u : 1u;
    uint64_t elapsed_rounds = (uint64_t)(now - t0_ms_) * 1000u / frame_us_;
    if (rounds_ < elapsed_rounds + lead_) {
        request_events();
        return;
    }
    /* Ahead of real time (a controller that completes SDUs early): wait */
    uint64_t due_ms = (rounds_ - lead_ + 1) * frame_us_ / 1000u;
    uint32_t wait = (uint32_t)(due_ms > (uint64_t)(now - t0_ms_) ? due_ms - (now - t0_ms_) : interval_ms);
    auto *t = static_cast<btstack_timer_source_t *>(timer_);
    btstack_run_loop_set_timer_handler(t, &Stream::timer_handler);
    btstack_run_loop_set_timer_context(t, this);
    btstack_run_loop_set_timer(t, wait ? wait : 1);
    btstack_run_loop_add_timer(t);
    timer_armed_ = true;
}

void Stream::send_sdu(size_t index, uint16_t handle) {
    const std::vector<uint8_t> &sdu = current_[index];
    hci_reserve_packet_buffer();
    uint8_t *buf = hci_get_outgoing_packet_buffer();
    /* ISO data packet: handle | PB 0b10 (complete SDU) | TS 0 */
    little_endian_store_16(buf, 0, (uint16_t)(handle | (2u << 12)));
    little_endian_store_16(buf, 2, (uint16_t)(4u + sdu.size()));
    little_endian_store_16(buf, 4, seq_[index]);
    little_endian_store_16(buf, 6, (uint16_t)sdu.size());
    memcpy(buf + 8, sdu.data(), sdu.size());
    uint8_t status = hci_send_iso_packet_buffer((uint16_t)(8u + sdu.size()));
    seq_[index]++;
    std::lock_guard<std::mutex> lock(mutex_);
    if (status == ERROR_CODE_SUCCESS) stats_.sdus_sent++;
    else stats_.send_errors++;
}

void Stream::on_cis_can_send_now(const uint8_t *packet, uint16_t size) {
    if (!sending_ || size < 8) return;
    uint16_t handle = little_endian_read_16(packet, 4);
    bool group_complete = packet[7] != 0;
    size_t index = 0;
    while (index < cis_handles_.size() && cis_handles_[index] != handle) index++;
    if (index == cis_handles_.size()) return;

    if (!round_open_ || served_[index]) {
        /* First CIS of a round (whichever CIS is up): the next 10 ms for
         * all CIS */
        std::lock_guard<std::mutex> lock(mutex_);
        if (buffering_ && sets_.size() >= prefill_) buffering_ = false;
        if (!buffering_ && !sets_.empty()) {
            current_ = std::move(sets_.front());
            sets_.pop_front();
        } else {
            if (!buffering_) {
                buffering_ = true;
                stats_.underruns++;
                if (stats_.underruns <= 10)
                    LOG_WARN("LE Audio: no audio for SDU set %llu (underrun %llu), sending silence",
                             (unsigned long long)rounds_, (unsigned long long)stats_.underruns);
            }
            current_ = silence_;
            stats_.silence_sets++;
        }
        stats_.sets_sent++;
        rounds_++;
        round_open_ = true;
        std::fill(served_.begin(), served_.end(), false);
    }
    if (current_.size() == cis_handles_.size()) send_sdu(index, handle);
    served_[index] = true;
    /* BTstack's group_complete is never set while the CIG's last CIS is
     * down: also count the CIS that are up ourselves */
    if (group_complete || round_done()) {
        round_open_ = false;
        request_next();
    }
}

Stream::Stats Stream::stats() const {
    std::lock_guard<std::mutex> lock(mutex_);
    return stats_;
}

} /* namespace le_audio */
