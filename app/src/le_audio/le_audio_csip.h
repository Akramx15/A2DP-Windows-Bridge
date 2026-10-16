/*
 * LE Audio (experimental) - CSIP Set Coordinator
 *
 * Earbuds that form a Coordinated Set (one device per ear) stream together:
 * the module reads the Coordinated Set Identification Service (CSIS) of each
 * connected device, resolves the Resolvable Set Identifiers (RSI) of
 * advertisers with the known Set Identity Resolving Keys (SIRK) and, before a
 * stream is set up, connects the members the user did not name
 * (prepare_group). See le_audio_csip.cpp and docs/le-audio-dev.md (CSIP).
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef LE_AUDIO_CSIP_H
#define LE_AUDIO_CSIP_H

#include "le_audio_module.h"

#include <array>
#include <cstddef>
#include <cstdint>

namespace le_audio {

/* Registered in le_audio_modules.cpp */
Module *csip_module();

namespace csip {

/* CSIS crypto toolbox (CSIS 1.0.1 section 4). Keys and values are 128-bit
 * numbers in the spec's notation: byte 0 is the most significant octet (the
 * reverse of the over-the-air order). */
using Key = std::array<uint8_t, 16>;

/* AES-128 (e) and AES-CMAC (RFC 4493) */
void aes128(const uint8_t key[16], const uint8_t in[16], uint8_t out[16]);
void aes_cmac(const uint8_t key[16], const uint8_t *msg, size_t len, uint8_t out[16]);
/* SIRK encryption / decryption function sef / sdf (4.5): k1(K, s1("SIRKenc"),
 * "csis") XOR sirk, K = the LTK of the link */
Key sef(const Key &k, const Key &sirk);
/* RSI hash function sih (4.7): the 24-bit hash of prand under sirk */
uint32_t sih(const Key &sirk, uint32_t prand);
/* True if rsi (6 bytes as advertised: hash, prand; least significant octet
 * first) was generated with sirk */
bool rsi_matches(const Key &sirk, const uint8_t rsi[6]);
/* Checks the functions against the sample data of CSIS appendix A */
bool self_test();

} /* namespace csip */

} /* namespace le_audio */

#endif /* LE_AUDIO_CSIP_H */
