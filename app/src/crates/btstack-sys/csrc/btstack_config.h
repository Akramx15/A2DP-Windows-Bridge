/*
 * btstack_config.h - BTstack compile-time configuration for A2DPWB v2
 *
 * Classic Bluetooth A2DP Source + AVRCP with vendor codecs over a host-side
 * HCI transport (WinUSB via nusb, or H4 over TCP to tools/emu).
 *
 * Same values as v1 (app/src/btstack_config.h) without LE: LE Audio is not
 * part of v2.0, and without ENABLE_BLE BTstack sends no LE setup commands,
 * so v1's "LE hidden" transport wrapper is not needed.
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#ifndef BTSTACK_CONFIG_H
#define BTSTACK_CONFIG_H

/* Platform features */
#define HAVE_ASSERT
#define HAVE_MALLOC
#define HAVE_POSIX_FILE_IO   /* chipset drivers read firmware files */
#define HAVE_POSIX_TIME

/* Classic Bluetooth (required for A2DP) */
#define ENABLE_CLASSIC

/* Security: Simple Secure Pairing (SSP) for headphone pairing */
#define ENABLE_SOFTWARE_AES128

/* Logging (to the HCI dump: A2DPWB routes it to tracing) */
#define ENABLE_LOG_ERROR
#define ENABLE_LOG_INFO

/* L2CAP Enhanced Retransmission (some A2DP devices use this) */
#define ENABLE_L2CAP_ENHANCED_RETRANSMISSION_MODE

/* HCI ACL buffer size - large enough for A2DP media packets */
#define HCI_ACL_PAYLOAD_SIZE (1691 + 4)
#define HCI_INCOMING_PRE_BUFFER_SIZE 14

/* Link key storage for paired devices */
#define NVM_NUM_DEVICE_DB_ENTRIES  16
#define NVM_NUM_LINK_KEYS          16

/* A2DP explicit config: disable SBC auto-selection, allow vendor codec config.
 * Required for LDAC/aptX - BTstack's auto-mode only handles SBC. */
#define ENABLE_A2DP_EXPLICIT_CONFIG

/* A2DP: 8 stream endpoints (LDAC, aptX HD, aptX, aptX LL, SBC (also SBC XQ), AAC, LHDC V5, FastStream).
 * The pool is static: an endpoint beyond this count fails to register. */
#define MAX_NR_AVDTP_STREAM_ENDPOINTS  8
#define MAX_NR_AVDTP_CONNECTIONS       1
#define MAX_NR_A2DP_SOURCE_CONNECTIONS 1

#endif /* BTSTACK_CONFIG_H */
