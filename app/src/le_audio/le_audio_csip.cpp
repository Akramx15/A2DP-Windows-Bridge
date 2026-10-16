/*
 * LE Audio (experimental) - CSIP Set Coordinator
 *
 * Earbuds that form a Coordinated Set (two devices, one per ear) stream
 * together. The user names one of them; this module finds and connects the
 * others, so the core streams to the whole set (one CIS per earbud, channels
 * by audio location - plan_stream() does that for a list of devices).
 *
 *  - on_gatt_ready(dev): CSIS (0x1846, primary or included by CAS) found:
 *    read the Set Identity Resolving Key (0x2B84; plaintext, or encrypted
 *    with the LTK of the link: sdf, CSIS 4.5), Coordinated Set Size (0x2B85),
 *    Set Member Rank (0x2B86) and Set Member Lock (0x2B87, only logged).
 *    Devices with the same SIRK form one set (an index into sets_, the
 *    ScanResult::set_id). The SIRK of a bonded device is kept in
 *    <config dir>\le_audio_csip.txt for later sessions.
 *  - on_scan_result(): an advertised RSI (AD 0x2E) is checked against every
 *    known SIRK (sih, CSIS 4.7); a match sets result.set_id / note /
 *    le_audio.
 *  - prepare_group() (worker): waits for the CSIS reads of the devices of the
 *    stream, adds set members that are already connected, scans (up to
 *    SCAN_TIMEOUT_MS) for RSIs of the set and connects the members found one
 *    after the other (the core sets up one LE connection at a time), then
 *    orders each set's devices by rank. Members not found in time are left
 *    out with a warning; the stream goes to the members available. A member
 *    that disconnects later makes the core report the stream lost; the
 *    worker sets it up again and prepare_group() looks for the member anew.
 *
 * Not implemented: the lock based Ordered Access procedure (the lock is read
 * and logged, never written: the stream is set up on all members without
 * locking the set, which only matters when another coordinator works on the
 * same set at the same time), notifications of SIRK / size / rank changes,
 * an OOB-only SIRK (CSIS error 0x80), more than one CSIS per device.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "le_audio_csip.h"
#include "le_audio_core.h"
#include "le_audio_gatt_client.h"
#include "config_path.h"
#include "debug_log.h"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <climits>
#include <cstdio>
#include <cstring>
#include <map>
#include <memory>
#include <mutex>

extern "C" {
#include "btstack_config.h"
#include "btstack.h"
#include "rijndael.h"
}

namespace le_audio {

/* ======================================================================== */
/* Crypto toolbox (CSIS 4.x)                                                 */
/* ======================================================================== */

namespace csip {

void aes128(const uint8_t key[16], const uint8_t in[16], uint8_t out[16]) {
    uint32_t rk[RKLENGTH(128)];
    int rounds = rijndaelSetupEncrypt(rk, key, 128);
    rijndaelEncrypt(rk, rounds, in, out);
}

namespace {

/* Subkey derivation of RFC 4493: shift left by one bit, conditional XOR */
void cmac_double(const uint8_t in[16], uint8_t out[16]) {
    uint8_t carry = 0;
    for (int i = 15; i >= 0; i--) {
        uint8_t b = in[i];
        out[i] = (uint8_t)((b << 1) | carry);
        carry = (uint8_t)(b >> 7);
    }
    if (in[0] & 0x80) out[15] ^= 0x87;
}

int nibble(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

} /* namespace */

/* 32 hex digits, most significant octet first; false if malformed */
bool parse_key(const char *hex, Key &k) {
    for (int i = 0; i < 16; i++) {
        int hi = nibble(hex[2 * i]), lo = hi < 0 ? -1 : nibble(hex[2 * i + 1]);
        if (lo < 0) return false;
        k[i] = (uint8_t)(hi << 4 | lo);
    }
    return hex[32] == '\0' || hex[32] == ' ' || hex[32] == '\n' || hex[32] == '\r';
}

namespace {

Key from_hex(const char *hex) {
    Key k{};
    parse_key(hex, k);
    return k;
}

} /* namespace */

void aes_cmac(const uint8_t key[16], const uint8_t *msg, size_t len, uint8_t out[16]) {
    uint8_t zero[16] = {}, l[16], k1[16], k2[16];
    aes128(key, zero, l);
    cmac_double(l, k1);
    cmac_double(k1, k2);
    size_t blocks = (len + 15) / 16;
    bool complete = blocks > 0 && len % 16 == 0;
    if (blocks == 0) blocks = 1;
    uint8_t last[16] = {};
    const size_t last_off = 16 * (blocks - 1);
    if (complete) {
        for (int i = 0; i < 16; i++) last[i] = (uint8_t)(msg[last_off + i] ^ k1[i]);
    } else {
        size_t rest = len - last_off;
        if (rest) memcpy(last, msg + last_off, rest);
        last[rest] = 0x80;
        for (int i = 0; i < 16; i++) last[i] ^= k2[i];
    }
    uint8_t x[16] = {}, y[16];
    for (size_t b = 0; b + 1 < blocks; b++) {
        for (int i = 0; i < 16; i++) y[i] = (uint8_t)(x[i] ^ msg[16 * b + i]);
        aes128(key, y, x);
    }
    for (int i = 0; i < 16; i++) y[i] = (uint8_t)(x[i] ^ last[i]);
    aes128(key, y, out);
}

Key sef(const Key &k, const Key &sirk) {
    /* s1(M) = AES-CMAC_ZERO(M); k1(N, SALT, P) = AES-CMAC_T(P), T = AES-CMAC_SALT(N) */
    static const uint8_t zero[16] = {};
    uint8_t salt[16], t[16], key[16];
    aes_cmac(zero, (const uint8_t *)"SIRKenc", 7, salt);
    aes_cmac(salt, k.data(), 16, t);
    aes_cmac(t, (const uint8_t *)"csis", 4, key);
    Key out;
    for (int i = 0; i < 16; i++) out[i] = (uint8_t)(key[i] ^ sirk[i]);
    return out;
}

uint32_t sih(const Key &sirk, uint32_t prand) {
    /* e(k, r'), r' = padding || r (r in the least significant 24 bits); mod 2^24 */
    uint8_t r[16] = {}, e[16];
    r[13] = (uint8_t)(prand >> 16);
    r[14] = (uint8_t)(prand >> 8);
    r[15] = (uint8_t)prand;
    aes128(sirk.data(), r, e);
    return ((uint32_t)e[13] << 16) | ((uint32_t)e[14] << 8) | e[15];
}

bool rsi_matches(const Key &sirk, const uint8_t rsi[6]) {
    /* RSI = hash || prand (prand most significant), sent least significant octet first */
    uint32_t hash = rd24(rsi);
    uint32_t prand = rd24(rsi + 3);
    return sih(sirk, prand) == hash;
}

bool self_test() {
    /* CSIS 1.0.1 appendix A: sample data of sih (A.1), s1 / sef (A.2 - A.4) */
    const Key sirk = from_hex("457d7d0921a1fd22cecd8c86dd72cccd");
    const Key ltk = from_hex("676e1b9bd448696f061ec6223ce5ced9");
    const Key enc = from_hex("170a3835e13524a07e2562d5f25fd346");
    const Key s1 = from_hex("6901983f18149e823c7d133a7d774572");
    static const uint8_t zero[16] = {};
    uint8_t salt[16];
    aes_cmac(zero, (const uint8_t *)"SIRKenc", 7, salt);
    bool ok = memcmp(salt, s1.data(), 16) == 0;
    ok = ok && sih(sirk, 0x69f563) == 0x1948da;
    ok = ok && sef(ltk, sirk) == enc && sef(ltk, enc) == sirk;
    const uint8_t rsi[6] = {0xda, 0x48, 0x19, 0x63, 0xf5, 0x69};
    ok = ok && rsi_matches(sirk, rsi);
    return ok;
}

} /* namespace csip */

/* ======================================================================== */
/* The module                                                                */
/* ======================================================================== */

namespace {

constexpr uint16_t UUID_CHAR_SIRK = 0x2B84;
constexpr uint16_t UUID_CHAR_SET_SIZE = 0x2B85;
constexpr uint16_t UUID_CHAR_LOCK = 0x2B86;   /* Set Member Lock */
constexpr uint16_t UUID_CHAR_RANK = 0x2B87;   /* Set Member Rank */

constexpr uint8_t SIRK_ENCRYPTED = 0x00;
constexpr uint8_t SIRK_PLAINTEXT = 0x01;
constexpr uint8_t ATT_ERROR_OOB_SIRK_ONLY = 0x80;

constexpr uint32_t READ_TIMEOUT_MS = 6000;      /* CSIS reads of a connected device */
constexpr uint32_t SCAN_TIMEOUT_MS = 10000;     /* looking for the missing members */
constexpr uint32_t CONNECT_TIMEOUT_MS = 30000;

using csip::Key;

Key key_from_air(const uint8_t *v) {
    Key k;
    for (int i = 0; i < 16; i++) k[i] = v[15 - i];
    return k;
}

std::string key_hex(const Key &k) {
    char buf[33];
    for (int i = 0; i < 16; i++) snprintf(buf + 2 * i, 3, "%02x", k[i]);
    return std::string(buf, 32);
}

/* CSIS of one connected device (per LeAudioDevice::id) */
struct Member {
    uint16_t con = CON_HANDLE_INVALID;
    int pending = 0;            /* reads in flight */
    bool done = false;          /* reads finished (or no CSIS) */
    bool ok = false;            /* SIRK known */
    bool encrypted_sirk = false;
    Key sirk{};
    int size = 0;               /* 0: not exposed */
    int rank = 0;               /* 0: not exposed */
    int lock = 0;               /* 0: not exposed, 1 unlocked, 2 locked */
    int set = -1;               /* index into sets_ */
    std::vector<std::function<void(bool)>> waiters;
};

struct KnownSet {
    Key sirk{};
    int size = 0;
};

/* A SIRK kept for a bonded device (le_audio_csip.txt) */
struct Stored {
    LeAddr addr;                /* identity address */
    Key sirk{};
    int size = 0;
    int rank = 0;
};

/* A member as the worker sees it */
struct Snap {
    int id = -1;
    int set = -1;
    int size = 0;
    int rank = 0;
    LeAddr addr;
};

class CsipModule : public Module {
public:
    const char *name() const override { return "CSIP"; }

    void on_stack_init(Core &core) override {
        (void)core;
        members_.clear();
        sets_.clear();
        rsi_cache_.clear();
        crypto_ok_ = csip::self_test();
        if (crypto_ok_)
            LOG_INFO("CSIP: crypto self-check passed (sih, sef against the CSIS sample data)");
        else
            LOG_ERROR("CSIP: crypto self-check FAILED: coordinated sets are not used");
        load_stored();
    }

    void on_stack_deinit() override {
        members_.clear();
        sets_.clear();
        rsi_cache_.clear();
    }

    void on_scan_result(ScanResult &result, const uint8_t *ad, uint16_t len) override {
        (void)ad;
        (void)len;
        if (result.rsi.size() != 6 || !crypto_ok_ || sets_.empty()) return;
        uint64_t key = 0;
        for (int i = 0; i < 6; i++) key |= (uint64_t)result.rsi[i] << (8 * i);
        int set = -1;
        auto it = rsi_cache_.find(key);
        if (it != rsi_cache_.end()) {
            set = it->second;
        } else {
            for (size_t i = 0; i < sets_.size() && set < 0; i++)
                if (csip::rsi_matches(sets_[i].sirk, result.rsi.data())) set = (int)i;
            rsi_cache_[key] = set;
        }
        if (set < 0) return;
        result.set_id = set;
        result.le_audio = true;
        char note[64];
        if (sets_[set].size > 0)
            snprintf(note, sizeof(note), "member of coordinated set %d (%d devices)", set, sets_[set].size);
        else
            snprintf(note, sizeof(note), "member of coordinated set %d", set);
        result.note = note;
        /* find_members() waits for a member it has not connected yet */
        if (set == wanted_set_.load() && result.connectable && !member_seen_.load()) {
            for (LeAudioDevice *d : Core::instance().all_devices())
                if (d->connected() && memcmp(d->addr.addr, result.addr.addr, 6) == 0) return;
            std::lock_guard<std::mutex> lock(tried_mutex_);
            for (const LeAddr &a : tried_)
                if (a == result.addr) return;
            member_seen_.store(true);
        }
    }

    void on_gatt_ready(LeAudioDevice &dev) override {
        Member &m = members_[dev.id];
        m = Member();
        m.con = dev.acl;
        const GattChar *sirk = dev.find_char(UUID_SERVICE_CSIS, UUID_CHAR_SIRK);
        if (!sirk) {
            m.done = true;
            return;
        }
        const int id = dev.id;
        const uint16_t con = dev.acl;
        struct Opt { uint16_t uuid; int Member::*field; };
        static const Opt opts[] = {{UUID_CHAR_SET_SIZE, &Member::size}, {UUID_CHAR_RANK, &Member::rank},
                                   {UUID_CHAR_LOCK, &Member::lock}};
        std::vector<std::pair<uint16_t, int Member::*>> reads;
        for (const Opt &o : opts)
            if (const GattChar *c = dev.find_char(UUID_SERVICE_CSIS, o.uuid)) reads.push_back({c->value_handle, o.field});
        m.pending = 1 + (int)reads.size();
        gatt_client::read(con, sirk->value_handle, [this, id, con](uint8_t status, const uint8_t *v, uint16_t n) {
            Member *x = member(id, con);
            if (!x) return;
            on_sirk(id, *x, status, v, n);
            read_done(id, *x);
        });
        for (const auto &r : reads) {
            int Member::*field = r.second;
            gatt_client::read(con, r.first, [this, id, con, field](uint8_t status, const uint8_t *v, uint16_t n) {
                Member *x = member(id, con);
                if (!x) return;
                if (status == 0 && n >= 1) x->*field = v[0];
                read_done(id, *x);
            });
        }
    }

    void on_disconnected(LeAudioDevice &dev, uint8_t reason) override {
        auto it = members_.find(dev.id);
        if (it == members_.end()) return;
        if (it->second.set >= 0)
            LOG_INFO("CSIP: %s (rank %d of coordinated set %d) disconnected (0x%02x)", dev.addr.str().c_str(),
                     it->second.rank, it->second.set, reason);
        auto waiters = std::move(it->second.waiters);
        members_.erase(it);
        for (auto &w : waiters) w(false);
    }

    void prepare_group(Core &core, std::vector<int> &ids, const std::function<bool()> &cancelled) override {
        if (ids.empty() || !crypto_ok_) return;
        for (int id : ids) wait_member(core, id, cancelled);
        std::vector<int> out;
        std::vector<int> sets_done;
        auto add = [&out](int id) {
            if (std::find(out.begin(), out.end(), id) == out.end()) out.push_back(id);
        };
        for (int id : ids) {
            Snap me;
            if (!snapshot(core, id, me) || me.set < 0) {
                add(id);
                continue;
            }
            if (std::find(sets_done.begin(), sets_done.end(), me.set) != sets_done.end()) continue;
            sets_done.push_back(me.set);
            std::vector<Snap> have = set_members(core, me.set);
            if (std::none_of(have.begin(), have.end(), [&me](const Snap &s) { return s.id == me.id; }))
                have.push_back(me);
            int size = me.size;
            for (const Snap &s : have) size = std::max(size, s.size);
            if ((int)have.size() < size && !(cancelled && cancelled())) find_members(core, me.set, size, have, cancelled);
            /* A member that went down meanwhile is left out (the device named
             * by the caller stays: the core reports it) */
            std::vector<Snap> up = set_members(core, me.set);
            have.erase(std::remove_if(have.begin(), have.end(), [&up, &me](const Snap &s) {
                bool gone = s.id != me.id && std::none_of(up.begin(), up.end(), [&s](const Snap &u) { return u.id == s.id; });
                if (gone) LOG_WARN("CSIP: set member %s went down, left out", s.addr.str().c_str());
                return gone;
            }), have.end());
            std::sort(have.begin(), have.end(), [](const Snap &a, const Snap &b) {
                int ra = a.rank > 0 ? a.rank : INT_MAX, rb = b.rank > 0 ? b.rank : INT_MAX;
                return ra != rb ? ra < rb : a.id < b.id;
            });
            std::string list;
            for (const Snap &s : have) {
                char buf[48];
                snprintf(buf, sizeof(buf), "%s%s rank %d", list.empty() ? "" : ", ", s.addr.str().c_str(), s.rank);
                list += buf;
                add(s.id);
            }
            if ((int)have.size() < size)
                LOG_WARN("CSIP: only %d of %d members of coordinated set %d found within %u s: streaming to "
                         "what is available", (int)have.size(), size, me.set, SCAN_TIMEOUT_MS / 1000);
            LOG_INFO("CSIP: streaming to coordinated set %d, %d of %d members: %s", me.set, (int)have.size(), size,
                     list.c_str());
        }
        ids = out;
    }

private:
    /* BTstack thread state */
    std::map<int, Member> members_;
    std::vector<KnownSet> sets_;
    std::vector<Stored> stored_;
    std::map<uint64_t, int> rsi_cache_;    /* RSI -> set index or -1 */
    bool crypto_ok_ = false;
    /* find_members() (worker) <-> on_scan_result() (BTstack thread) */
    std::atomic<int> wanted_set_{-1};
    std::atomic<bool> member_seen_{false};
    std::mutex tried_mutex_;
    std::vector<LeAddr> tried_;             /* connect attempts of this search */

    Member *member(int id, uint16_t con) {
        auto it = members_.find(id);
        if (it == members_.end() || it->second.con != con) return nullptr;
        return &it->second;
    }

    void on_sirk(int id, Member &m, uint8_t status, const uint8_t *v, uint16_t n) {
        LeAudioDevice *d = Core::instance().device(id);
        const std::string addr = d ? d->addr.str() : "?";
        if (status != 0) {
            LOG_WARN("CSIP: %s: reading the SIRK failed (0x%02x%s)", addr.c_str(), status,
                     status == ATT_ERROR_OOB_SIRK_ONLY ? ", OOB SIRK only: not supported" : "");
            return;
        }
        if (n < 17) {
            LOG_WARN("CSIP: %s: SIRK value of %u bytes", addr.c_str(), n);
            return;
        }
        Key value = key_from_air(v + 1);
        if (v[0] == SIRK_PLAINTEXT) {
            m.sirk = value;
        } else if (v[0] == SIRK_ENCRYPTED) {
            /* sdf(K, EncSIRK) with K = the LTK of the link (BTstack keeps it most
             * significant octet first, as the spec writes it) */
            int index = sm_le_device_index(m.con);
            if (index < 0) {
                LOG_WARN("CSIP: %s: encrypted SIRK, but no bond (LTK) to decrypt it", addr.c_str());
                return;
            }
            uint16_t ediv = 0;
            uint8_t rand[8] = {};
            sm_key_t ltk = {};
            int key_size = 0, authenticated = 0, authorized = 0, secure = 0;
            le_device_db_encryption_get(index, &ediv, rand, ltk, &key_size, &authenticated, &authorized, &secure);
            Key k;
            memcpy(k.data(), ltk, 16);
            if (k == Key{}) {
                LOG_WARN("CSIP: %s: encrypted SIRK, but the LTK of the bond is not available", addr.c_str());
                return;
            }
            m.sirk = csip::sef(k, value);
            m.encrypted_sirk = true;
        } else {
            LOG_WARN("CSIP: %s: SIRK type 0x%02x unknown", addr.c_str(), v[0]);
            return;
        }
        m.ok = true;
    }

    void read_done(int id, Member &m) {
        if (--m.pending > 0) return;
        m.done = true;
        LeAudioDevice *d = Core::instance().device(id);
        if (m.ok) {
            m.set = add_set(m.sirk, m.size);
            static const char *locks[] = {"no lock", "unlocked", "locked"};
            LOG_INFO("CSIP: %s: member of coordinated set %d, rank %d of %d (%s SIRK, %s)",
                     d ? d->addr.str().c_str() : "?", m.set, m.rank, m.size,
                     m.encrypted_sirk ? "encrypted" : "plaintext", m.lock >= 0 && m.lock <= 2 ? locks[m.lock] : "lock ?");
            if (d) store(*d, m);
        }
        auto waiters = std::move(m.waiters);
        m.waiters.clear();
        for (auto &w : waiters) w(m.ok);
    }

    int add_set(const Key &sirk, int size) {
        for (size_t i = 0; i < sets_.size(); i++)
            if (sets_[i].sirk == sirk) {
                sets_[i].size = std::max(sets_[i].size, size);
                return (int)i;
            }
        KnownSet s;
        s.sirk = sirk;
        s.size = size;
        sets_.push_back(s);
        /* RSIs that did not resolve may belong to the new set */
        for (auto it = rsi_cache_.begin(); it != rsi_cache_.end();)
            it = it->second < 0 ? rsi_cache_.erase(it) : std::next(it);
        return (int)sets_.size() - 1;
    }

    /* ---- Persistence: the SIRK of bonded devices ---- */

    static std::string stored_path() { return get_config_dir() + "\\le_audio_csip.txt"; }

    static bool bonded(const LeAddr &addr) {
        for (int i = 0; i < le_device_db_max_count(); i++) {
            int type = BD_ADDR_TYPE_UNKNOWN;
            bd_addr_t a;
            sm_key_t irk;
            le_device_db_info(i, &type, a, irk);
            if (type != BD_ADDR_TYPE_UNKNOWN && memcmp(a, addr.addr, 6) == 0) return true;
        }
        return false;
    }

    void load_stored() {
        stored_.clear();
        FILE *f = nullptr;
        if (fopen_s(&f, stored_path().c_str(), "rb") != 0 || !f) return;
        char line[160];
        int dropped = 0;
        while (fgets(line, sizeof(line), f)) {
            char addr[24], hex[40];
            int type = 0, size = 0, rank = 0;
            if (line[0] == '#' || sscanf_s(line, "%23s %d %39s %d %d", addr, (unsigned)sizeof(addr), &type, hex,
                                           (unsigned)sizeof(hex), &size, &rank) != 5)
                continue;
            Stored s;
            if (!addr_from_string(addr, s.addr.addr) || !csip::parse_key(hex, s.sirk)) continue;
            s.addr.type = (uint8_t)type;
            s.size = size;
            s.rank = rank;
            if (!bonded(s.addr)) {   /* the bond was removed: forget its SIRK too */
                dropped++;
                continue;
            }
            stored_.push_back(s);
            add_set(s.sirk, s.size);
        }
        fclose(f);
        if (dropped) save_stored();
        if (!stored_.empty())
            LOG_INFO("CSIP: %u known coordinated set(s) of %u bonded device(s)", (unsigned)sets_.size(),
                     (unsigned)stored_.size());
    }

    void save_stored() {
        FILE *f = nullptr;
        if (fopen_s(&f, stored_path().c_str(), "wb") != 0 || !f) {
            LOG_WARN("CSIP: cannot write %s", stored_path().c_str());
            return;
        }
        fputs("# A2DPWB LE Audio: SIRK of bonded coordinated set members (identity address, type, SIRK, size, rank)\n", f);
        for (const Stored &s : stored_)
            fprintf(f, "%s %u %s %d %d\n", s.addr.str().c_str(), s.addr.type, key_hex(s.sirk).c_str(), s.size, s.rank);
        fclose(f);
    }

    void store(const LeAudioDevice &dev, const Member &m) {
        int index = sm_le_device_index(m.con);
        if (index < 0) return;   /* not bonded: this session only */
        Stored s;
        int type = BD_ADDR_TYPE_UNKNOWN;
        sm_key_t irk;
        le_device_db_info(index, &type, s.addr.addr, irk);
        if (type == BD_ADDR_TYPE_UNKNOWN) s.addr = dev.addr;
        else s.addr.type = (uint8_t)type;
        s.sirk = m.sirk;
        s.size = m.size;
        s.rank = m.rank;
        for (const Stored &x : stored_)
            if (x.addr == s.addr && x.sirk == s.sirk && x.size == s.size && x.rank == s.rank) return;
        stored_.erase(std::remove_if(stored_.begin(), stored_.end(), [&s](const Stored &x) { return x.addr == s.addr; }),
                      stored_.end());
        stored_.push_back(s);
        save_stored();
    }

    /* ---- Worker side (prepare_group) ---- */

    /* Waits until the CSIS reads of device id finished; true if it has a SIRK */
    bool wait_member(Core &core, int id, const std::function<bool()> &cancelled) {
        return core.call([this, id](std::function<void(bool)> done) {
            auto it = members_.find(id);
            if (it == members_.end()) { done(false); return; }
            if (it->second.done) { done(it->second.ok); return; }
            it->second.waiters.push_back(done);
        }, READ_TIMEOUT_MS, cancelled);
    }

    bool snapshot(Core &core, int id, Snap &out) {
        auto res = std::make_shared<Snap>();
        auto ok = std::make_shared<bool>(false);
        core.run([this, id, res, ok]() {
            LeAudioDevice *d = Core::instance().device(id);
            auto it = members_.find(id);
            if (!d || it == members_.end() || !it->second.ok) return;
            res->id = id;
            res->set = it->second.set;
            res->size = it->second.size;
            res->rank = it->second.rank;
            res->addr = d->addr;
            *ok = true;
        });
        if (*ok) out = *res;
        return *ok;
    }

    /* Connected, ready devices of a set */
    std::vector<Snap> set_members(Core &core, int set) {
        auto res = std::make_shared<std::vector<Snap>>();
        core.run([this, set, res]() {
            for (LeAudioDevice *d : Core::instance().all_devices()) {
                auto it = members_.find(d->id);
                if (!d->ready || it == members_.end() || !it->second.ok || it->second.set != set) continue;
                Snap s;
                s.id = d->id;
                s.set = set;
                s.size = it->second.size;
                s.rank = it->second.rank;
                s.addr = d->addr;
                res->push_back(s);
            }
        });
        return *res;
    }

    /* Scan for RSIs of the set and connect the members found, one at a time */
    void find_members(Core &core, int set, int size, std::vector<Snap> &have, const std::function<bool()> &cancelled) {
        LOG_INFO("CSIP: coordinated set %d: %d of %d members connected, looking for the others (up to %u s)", set,
                 (int)have.size(), size, SCAN_TIMEOUT_MS / 1000);
        auto is_cancelled = [&cancelled]() { return cancelled && cancelled(); };
        auto known = [&have](const LeAddr &a) {
            return std::any_of(have.begin(), have.end(),
                               [&a](const Snap &s) { return memcmp(s.addr.addr, a.addr, 6) == 0; });
        };
        auto was_tried = [this](const LeAddr &a) {
            std::lock_guard<std::mutex> lock(tried_mutex_);
            return std::any_of(tried_.begin(), tried_.end(), [&a](const LeAddr &t) { return t == a; });
        };
        {
            std::lock_guard<std::mutex> lock(tried_mutex_);
            tried_.clear();
        }
        wanted_set_.store(set);
        /* Scan until on_scan_result() saw a member not tried yet (or the time is up) */
        auto stop_scan = [this, &cancelled]() { return member_seen_.load() || (cancelled && cancelled()); };
        auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(SCAN_TIMEOUT_MS);
        while ((int)have.size() < size && !is_cancelled()) {
            auto left = std::chrono::duration_cast<std::chrono::milliseconds>(deadline - std::chrono::steady_clock::now());
            if (left.count() <= 0) break;
            member_seen_.store(false);
            auto results = core.scan((uint32_t)left.count(), stop_scan);
            for (const ScanResult &r : results) {
                if ((int)have.size() >= size || is_cancelled()) break;
                if (r.set_id != set || !r.connectable || known(r.addr) || was_tried(r.addr)) continue;
                {
                    std::lock_guard<std::mutex> lock(tried_mutex_);
                    tried_.push_back(r.addr);
                }
                LOG_INFO("CSIP: %s \"%s\" resolves to coordinated set %d (RSI): connecting it", r.addr.str().c_str(),
                         r.name.c_str(), set);
                std::string error;
                int id = core.connect(r.addr, r.name, CONNECT_TIMEOUT_MS, &error, cancelled);
                if (id < 0) {
                    LOG_WARN("CSIP: set member %s: %s", r.addr.str().c_str(), error.c_str());
                    continue;
                }
                Snap s;
                if (!wait_member(core, id, cancelled) || !snapshot(core, id, s) || s.set != set) {
                    LOG_WARN("CSIP: %s is not a member of coordinated set %d after all: not used", r.addr.str().c_str(),
                             set);
                    core.disconnect(id);
                    continue;
                }
                have.push_back(s);
            }
        }
        wanted_set_.store(-1);
    }
};

} /* namespace */

Module *csip_module() {
    static CsipModule module;
    return &module;
}

} /* namespace le_audio */
