/*
 * shim.c - C glue between BTstack and the Rust side of btstack-sys
 *
 * Holds everything whose layout is BTstack's business: the static structs
 * BTstack keeps pointers to (registrations, timers, SDP record buffers,
 * stream endpoint capabilities, the codec configuration of
 * a2dp_source_set_config_other), the function tables Rust implements (HCI
 * transport, link key DB, HCI dump), and reads of struct members
 * (hci_connection_t, avdtp_connection_t). No logic of its own: every
 * decision is made in Rust. Runs on the BTstack run loop thread only,
 * except a2dpwb_wake().
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

#include "btstack_config.h"

#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <windows.h>

#include "bluetooth.h"
#include "btstack_defines.h"
#include "btstack_memory.h"
#include "btstack_run_loop.h"
#include "btstack_run_loop_windows.h"
#include "gap.h"
#include "hci.h"
#include "hci_cmd.h"
#include "hci_dump.h"
#include "hci_transport.h"
#include "l2cap.h"
#include "classic/a2dp_source.h"
#include "classic/avdtp.h"
#include "classic/avdtp_sink.h"
#include "classic/avrcp.h"
#include "classic/avrcp_controller.h"
#include "classic/avrcp_target.h"
#include "classic/btstack_link_key_db.h"
#include "classic/btstack_link_key_db_memory.h"
#include "classic/sdp_server.h"
#include "btstack_chipset_intel_firmware.h"

/* ---- Implemented in Rust (btstack-sys/src/callbacks.rs) ---- */
void a2dpwb_rs_on_wake(void);
void a2dpwb_rs_on_timer(uint32_t id);
void a2dpwb_rs_on_packet(uint8_t source, uint8_t packet_type, uint16_t channel, uint8_t *packet, uint16_t size);
int a2dpwb_rs_transport_open(void);
int a2dpwb_rs_transport_close(void);
int a2dpwb_rs_transport_can_send(uint8_t packet_type);
int a2dpwb_rs_transport_send(uint8_t packet_type, const uint8_t *packet, int size);
void a2dpwb_rs_log_packet(uint8_t packet_type, uint8_t in, const uint8_t *packet, uint16_t len);
void a2dpwb_rs_log_message(int level, const char *message);
int a2dpwb_rs_lk_get(const uint8_t *addr, uint8_t *key, int *type);
void a2dpwb_rs_lk_put(const uint8_t *addr, const uint8_t *key, int type);
void a2dpwb_rs_lk_delete(const uint8_t *addr);
int a2dpwb_rs_lk_at(int index, uint8_t *addr, uint8_t *key, int *type);
void a2dpwb_rs_intel_done(int result);

/* ======================================================================== */
/* Run loop: wake-up from other threads, timers                             */
/* ======================================================================== */

static btstack_context_callback_registration_t wake_registration;
static volatile LONG wake_pending;

static void wake_callback(void *context) {
    (void)context;
    /* Cleared first: a wake that comes while Rust runs queues a new callback */
    InterlockedExchange(&wake_pending, 0);
    a2dpwb_rs_on_wake();
}

void a2dpwb_runloop_init(void) {
    btstack_run_loop_init(btstack_run_loop_windows_get_instance());
    memset(&wake_registration, 0, sizeof(wake_registration));
    wake_registration.callback = &wake_callback;
    InterlockedExchange(&wake_pending, 0);
}

/* Any thread. One callback in the run loop's list at a time: the
 * registration is a list element and must not be added twice. */
void a2dpwb_wake(void) {
    if (InterlockedExchange(&wake_pending, 1) == 0) {
        btstack_run_loop_execute_on_main_thread(&wake_registration);
    }
}

#define A2DPWB_MAX_TIMERS 16
static btstack_timer_source_t timers[A2DPWB_MAX_TIMERS];

static void timer_callback(btstack_timer_source_t *ts) {
    a2dpwb_rs_on_timer((uint32_t)(uintptr_t)btstack_run_loop_get_timer_context(ts));
}

int a2dpwb_timer_start(uint32_t id, uint32_t timeout_ms) {
    if (id >= A2DPWB_MAX_TIMERS) return 0;
    btstack_timer_source_t *t = &timers[id];
    btstack_run_loop_remove_timer(t);
    btstack_run_loop_set_timer_handler(t, &timer_callback);
    btstack_run_loop_set_timer_context(t, (void *)(uintptr_t)id);
    btstack_run_loop_set_timer(t, timeout_ms);
    btstack_run_loop_add_timer(t);
    return 1;
}

void a2dpwb_timer_stop(uint32_t id) {
    if (id >= A2DPWB_MAX_TIMERS) return;
    btstack_run_loop_remove_timer(&timers[id]);
}

void a2dpwb_timers_clear(void) {
    for (int i = 0; i < A2DPWB_MAX_TIMERS; i++) btstack_run_loop_remove_timer(&timers[i]);
}

/* ======================================================================== */
/* HCI transport implemented in Rust                                        */
/* ======================================================================== */

static void (*transport_packet_handler)(uint8_t packet_type, uint8_t *packet, uint16_t size);
/* Incoming packets are copied here with BTstack's pre-buffer in front */
static uint8_t transport_rx_buffer[HCI_INCOMING_PRE_BUFFER_SIZE + HCI_ACL_PAYLOAD_SIZE + 4 + 260];

static void transport_init(const void *config) { (void)config; }
static int transport_open(void) { return a2dpwb_rs_transport_open(); }
static int transport_close(void) { return a2dpwb_rs_transport_close(); }
static void transport_register_packet_handler(void (*handler)(uint8_t, uint8_t *, uint16_t)) {
    transport_packet_handler = handler;
}
static int transport_can_send_packet_now(uint8_t packet_type) {
    return a2dpwb_rs_transport_can_send(packet_type);
}
static int transport_send_packet(uint8_t packet_type, uint8_t *packet, int size) {
    return a2dpwb_rs_transport_send(packet_type, packet, size);
}

/* Synchronous (send_packet returns when the packet is out, e.g. TCP): no
 * can_send_packet_now, HCI frees its buffer at once */
static const hci_transport_t transport_sync = {
    "a2dpwb-sync", &transport_init, &transport_open, &transport_close,
    &transport_register_packet_handler, NULL, &transport_send_packet, NULL, NULL, NULL};
/* Asynchronous (USB): HCI waits for HCI_EVENT_TRANSPORT_PACKET_SENT */
static const hci_transport_t transport_async = {
    "a2dpwb-async", &transport_init, &transport_open, &transport_close,
    &transport_register_packet_handler, &transport_can_send_packet_now, &transport_send_packet,
    NULL, NULL, NULL};

const hci_transport_t *a2dpwb_transport_instance(int async) {
    return async ? &transport_async : &transport_sync;
}

/* Returns 0 if the packet does not fit or no handler is registered */
int a2dpwb_transport_deliver(uint8_t packet_type, const uint8_t *packet, uint16_t size) {
    if (transport_packet_handler == NULL) return 0;
    if ((size_t)size > sizeof(transport_rx_buffer) - HCI_INCOMING_PRE_BUFFER_SIZE) return 0;
    uint8_t *p = &transport_rx_buffer[HCI_INCOMING_PRE_BUFFER_SIZE];
    memcpy(p, packet, size);
    transport_packet_handler(packet_type, p, size);
    return 1;
}

void a2dpwb_transport_packet_sent(void) {
    static uint8_t sent[] = {HCI_EVENT_TRANSPORT_PACKET_SENT, 0};
    if (transport_packet_handler) transport_packet_handler(HCI_EVENT_PACKET, sent, sizeof(sent));
}

void a2dpwb_hci_init(int async) {
    transport_packet_handler = NULL;
    hci_init(a2dpwb_transport_instance(async), NULL);
}

/* Intel bootloader firmware download: runs on the transport before hci_init */
static void intel_done(int result) { a2dpwb_rs_intel_done(result); }

void a2dpwb_intel_download_firmware(int async, const char *firmware_path) {
    btstack_chipset_intel_set_firmware_path(firmware_path);
    btstack_chipset_intel_download_firmware(a2dpwb_transport_instance(async), &intel_done);
}

/* ======================================================================== */
/* HCI dump: packets and BTstack's log to Rust                              */
/* ======================================================================== */

static void dump_reset(void) {}

static void dump_log_packet(uint8_t packet_type, uint8_t in, uint8_t *packet, uint16_t len) {
    a2dpwb_rs_log_packet(packet_type, in, packet, len);
}

static void dump_log_message(int log_level, const char *format, va_list argptr) {
    char buffer[512];
    vsnprintf(buffer, sizeof(buffer), format, argptr);
    a2dpwb_rs_log_message(log_level, buffer);
}

static const hci_dump_t hci_dump_rust = {&dump_reset, &dump_log_packet, &dump_log_message};

void a2dpwb_hci_dump_init(void) {
    hci_dump_init(&hci_dump_rust);
}

/* ======================================================================== */
/* Link key DB implemented in Rust                                          */
/* ======================================================================== */

static void lk_open(void) {}
static void lk_set_local_bd_addr(bd_addr_t bd_addr) { (void)bd_addr; }
static void lk_close(void) {}

static int lk_get_link_key(bd_addr_t bd_addr, link_key_t link_key, link_key_type_t *type) {
    int t = 0;
    if (!a2dpwb_rs_lk_get(bd_addr, link_key, &t)) return 0;
    *type = (link_key_type_t)t;
    return 1;
}

static void lk_put_link_key(bd_addr_t bd_addr, link_key_t link_key, link_key_type_t type) {
    a2dpwb_rs_lk_put(bd_addr, link_key, (int)type);
}

static void lk_delete_link_key(bd_addr_t bd_addr) { a2dpwb_rs_lk_delete(bd_addr); }

/* The iterator is an index into the Rust store (deleting while iterating
 * is allowed and shifts the entries, as in v1's file DB) */
static int lk_iterator_init(btstack_link_key_iterator_t *it) {
    it->context = (void *)(intptr_t)0;
    return 1;
}

static int lk_iterator_get_next(btstack_link_key_iterator_t *it, bd_addr_t bd_addr, link_key_t link_key,
                                link_key_type_t *type) {
    int index = (int)(intptr_t)it->context;
    int t = 0;
    if (!a2dpwb_rs_lk_at(index, bd_addr, link_key, &t)) return 0;
    it->context = (void *)(intptr_t)(index + 1);
    *type = (link_key_type_t)t;
    return 1;
}

static void lk_iterator_done(btstack_link_key_iterator_t *it) { it->context = NULL; }

static const btstack_link_key_db_t link_key_db_rust = {
    &lk_open, &lk_set_local_bd_addr, &lk_close, &lk_get_link_key, &lk_put_link_key,
    &lk_delete_link_key, &lk_iterator_init, &lk_iterator_get_next, &lk_iterator_done};

void a2dpwb_set_link_key_db(int rust_store) {
    hci_set_link_key_db(rust_store ? &link_key_db_rust : btstack_link_key_db_memory_instance());
}

/* ======================================================================== */
/* Packet handlers                                                          */
/* ======================================================================== */

enum {
    SOURCE_HCI = 0,
    SOURCE_A2DP = 1,
    SOURCE_AVDTP_SINK = 2,
    SOURCE_AVRCP = 3,
    SOURCE_AVRCP_CONTROLLER = 4,
    SOURCE_AVRCP_TARGET = 5,
};

static void handler_hci(uint8_t t, uint16_t c, uint8_t *p, uint16_t s) { a2dpwb_rs_on_packet(SOURCE_HCI, t, c, p, s); }
static void handler_a2dp(uint8_t t, uint16_t c, uint8_t *p, uint16_t s) { a2dpwb_rs_on_packet(SOURCE_A2DP, t, c, p, s); }
static void handler_avdtp_sink(uint8_t t, uint16_t c, uint8_t *p, uint16_t s) { a2dpwb_rs_on_packet(SOURCE_AVDTP_SINK, t, c, p, s); }
static void handler_avrcp(uint8_t t, uint16_t c, uint8_t *p, uint16_t s) { a2dpwb_rs_on_packet(SOURCE_AVRCP, t, c, p, s); }
static void handler_avrcp_ct(uint8_t t, uint16_t c, uint8_t *p, uint16_t s) { a2dpwb_rs_on_packet(SOURCE_AVRCP_CONTROLLER, t, c, p, s); }
static void handler_avrcp_tg(uint8_t t, uint16_t c, uint8_t *p, uint16_t s) { a2dpwb_rs_on_packet(SOURCE_AVRCP_TARGET, t, c, p, s); }

static btstack_packet_callback_registration_t hci_event_registration;

void a2dpwb_add_hci_event_handler(void) {
    memset(&hci_event_registration, 0, sizeof(hci_event_registration));
    hci_event_registration.callback = &handler_hci;
    hci_add_event_handler(&hci_event_registration);
}

void a2dpwb_a2dp_source_init(void) {
    a2dp_source_init();
    a2dp_source_register_packet_handler(&handler_a2dp);
    /* No sink SEPs are registered, so the AVDTP sink callback only gets the
     * events AVDTP emits to both roles, SEP discovery among them */
    avdtp_register_sink_packet_handler(&handler_avdtp_sink);
}

void a2dpwb_avrcp_init(void) {
    avrcp_init();
    avrcp_register_packet_handler(&handler_avrcp);
    avrcp_controller_init();
    avrcp_controller_register_packet_handler(&handler_avrcp_ct);
    avrcp_target_init();
    avrcp_target_register_packet_handler(&handler_avrcp_tg);
}

/* ======================================================================== */
/* GAP, SDP records, stream endpoints                                       */
/* ======================================================================== */

void a2dpwb_gap_setup(uint32_t class_of_device, uint16_t page_timeout) {
    /* SSP: Just Works (no display, no keyboard), General Bonding without MITM */
    gap_ssp_set_io_capability(SSP_IO_CAPABILITY_NO_INPUT_NO_OUTPUT);
    gap_ssp_set_authentication_requirement(SSP_IO_AUTHREQ_MITM_PROTECTION_NOT_REQUIRED_GENERAL_BONDING);
    gap_set_local_name("A2DPWB");  /* BTstack keeps the pointer: a literal */
    gap_set_class_of_device(class_of_device);
    gap_set_allow_role_switch(true);
    gap_set_default_link_policy_settings(0x0005);  /* role switch + sniff */
    gap_set_page_timeout(page_timeout);
}

static uint8_t sdp_a2dp_source_service[150];
static uint8_t sdp_avrcp_controller_service[200];
static uint8_t sdp_avrcp_target_service[200];

void a2dpwb_sdp_register_records(uint16_t controller_features, uint16_t target_features) {
    memset(sdp_a2dp_source_service, 0, sizeof(sdp_a2dp_source_service));
    a2dp_source_create_sdp_record(sdp_a2dp_source_service, sdp_create_service_record_handle(),
                                  AVDTP_SOURCE_FEATURE_MASK_PLAYER, NULL, NULL);
    sdp_register_service(sdp_a2dp_source_service);

    memset(sdp_avrcp_controller_service, 0, sizeof(sdp_avrcp_controller_service));
    avrcp_controller_create_sdp_record(sdp_avrcp_controller_service, sdp_create_service_record_handle(),
                                       controller_features, NULL, NULL);
    sdp_register_service(sdp_avrcp_controller_service);

    memset(sdp_avrcp_target_service, 0, sizeof(sdp_avrcp_target_service));
    avrcp_target_create_sdp_record(sdp_avrcp_target_service, sdp_create_service_record_handle(),
                                   target_features, NULL, NULL);
    sdp_register_service(sdp_avrcp_target_service);
}

#define A2DPWB_MAX_ENDPOINTS MAX_NR_AVDTP_STREAM_ENDPOINTS
#define A2DPWB_CODEC_INFO_MAX 32
static uint8_t endpoint_caps[A2DPWB_MAX_ENDPOINTS][A2DPWB_CODEC_INFO_MAX];
static uint8_t endpoint_config[A2DPWB_MAX_ENDPOINTS][A2DPWB_CODEC_INFO_MAX];

/* Returns the local SEID, 0 on failure */
uint8_t a2dpwb_create_stream_endpoint(int slot, uint8_t media_codec_type, const uint8_t *caps, uint16_t caps_len,
                                      const uint8_t *config, uint16_t config_len) {
    if (slot < 0 || slot >= A2DPWB_MAX_ENDPOINTS) return 0;
    if (caps_len > A2DPWB_CODEC_INFO_MAX || config_len > A2DPWB_CODEC_INFO_MAX) return 0;
    memcpy(endpoint_caps[slot], caps, caps_len);
    memcpy(endpoint_config[slot], config, config_len);
    avdtp_stream_endpoint_t *ep = a2dp_source_create_stream_endpoint(
        AVDTP_AUDIO, (avdtp_media_codec_type_t)media_codec_type, endpoint_caps[slot], caps_len,
        endpoint_config[slot], config_len);
    return ep ? avdtp_local_seid(ep) : 0;
}

/* ======================================================================== */
/* Stream configuration                                                     */
/* ======================================================================== */

uint8_t a2dpwb_set_config_sbc(uint16_t a2dp_cid, uint8_t local_seid, uint8_t remote_seid,
                              uint16_t sampling_frequency, int channel_mode, int block_length, int subbands,
                              int allocation_method, uint8_t min_bitpool, uint8_t max_bitpool) {
    avdtp_configuration_sbc_t c;
    memset(&c, 0, sizeof(c));
    c.sampling_frequency = sampling_frequency;
    c.channel_mode = (avdtp_channel_mode_t)channel_mode;
    c.block_length = (avdtp_sbc_block_length_t)block_length;
    c.subbands = (avdtp_sbc_subbands_t)subbands;
    c.allocation_method = (avdtp_sbc_allocation_method_t)allocation_method;
    c.min_bitpool_value = min_bitpool;
    c.max_bitpool_value = max_bitpool;
    return a2dp_source_set_config_sbc(a2dp_cid, local_seid, remote_seid, &c);
}

uint8_t a2dpwb_set_config_aac(uint16_t a2dp_cid, uint8_t local_seid, uint8_t remote_seid, int object_type,
                              uint32_t sampling_frequency, uint8_t channels, uint32_t bit_rate, uint8_t vbr) {
    avdtp_configuration_mpeg_aac_t c;
    memset(&c, 0, sizeof(c));
    c.object_type = (avdtp_aac_object_type_t)object_type;
    c.sampling_frequency = sampling_frequency;
    c.channels = channels;
    c.bit_rate = bit_rate;
    c.vbr = vbr;
    c.drc = false;
    return a2dp_source_set_config_mpeg_aac(a2dp_cid, local_seid, remote_seid, &c);
}

/* BTstack keeps the pointer of the codec information until the
 * configuration is done: a persistent copy */
static uint8_t other_config_info[A2DPWB_CODEC_INFO_MAX];

uint8_t a2dpwb_set_config_other(uint16_t a2dp_cid, uint8_t local_seid, uint8_t remote_seid, const uint8_t *info,
                                uint8_t info_len) {
    if (info_len > sizeof(other_config_info)) return ERROR_CODE_INVALID_HCI_COMMAND_PARAMETERS;
    memcpy(other_config_info, info, info_len);
    return a2dp_source_set_config_other(a2dp_cid, local_seid, remote_seid, other_config_info, info_len);
}

/* ======================================================================== */
/* Connection state queries                                                 */
/* ======================================================================== */

/* ACL link to addr: -1 none, 0 still paging, 1 open (handle set), 2 other */
int a2dpwb_acl_state(const uint8_t *addr, uint16_t *con_handle) {
    hci_connection_t *c = hci_connection_for_bd_addr_and_type(addr, BD_ADDR_TYPE_ACL);
    if (c == NULL) return -1;
    if (c->state == SEND_CREATE_CONNECTION || c->state == SENT_CREATE_CONNECTION) return 0;
    if (c->state == OPEN) {
        if (con_handle) *con_handle = c->con_handle;
        return 1;
    }
    return 2;
}

static int avdtp_state(const avdtp_connection_t *c) {
    if (c == NULL) return -1;
    return (c->state == AVDTP_SIGNALING_CONNECTION_OPENED ||
            c->state == AVDTP_SIGNALING_CONNECTION_W4_L2CAP_DISCONNECTED) ? 1 : 2;
}

/* AVDTP connection: -1 none, 1 signaling channel open (a release follows a
 * disconnect), 2 not open yet (freed at once by a disconnect) */
int a2dpwb_avdtp_state_for_addr(const uint8_t *addr, uint16_t *avdtp_cid) {
    bd_addr_t a;
    memcpy(a, addr, 6);
    avdtp_connection_t *c = avdtp_get_connection_for_bd_addr(a);
    if (c && avdtp_cid) *avdtp_cid = c->avdtp_cid;
    return avdtp_state(c);
}

int a2dpwb_avdtp_state_for_cid(uint16_t avdtp_cid) {
    return avdtp_state(avdtp_get_connection_for_avdtp_cid(avdtp_cid));
}

/* ======================================================================== */
/* HCI commands                                                             */
/* ======================================================================== */

/* Not in BTstack's command table: HCI Set AFH Host Channel Classification
 * (10-byte map as 8 + 1 + 1 bytes) and HCI Read AFH Channel Map */
static const hci_cmd_t cmd_set_afh_host_channel_classification = {0x0C3F, "D11"};
static const hci_cmd_t cmd_read_afh_channel_map = {0x1406, "H"};

uint8_t a2dpwb_cmd_set_afh_classification(const uint8_t *map) {
    return hci_send_cmd(&cmd_set_afh_host_channel_classification, map, map[8], map[9]);
}

uint8_t a2dpwb_cmd_read_afh_channel_map(uint16_t con_handle) {
    return hci_send_cmd(&cmd_read_afh_channel_map, con_handle);
}

uint8_t a2dpwb_cmd_read_rssi(uint16_t con_handle) {
    return hci_send_cmd(&hci_read_rssi, con_handle);
}

uint8_t a2dpwb_cmd_read_bd_addr(void) {
    return hci_send_cmd(&hci_read_bd_addr);
}

uint8_t a2dpwb_cmd_read_local_version(void) {
    return hci_send_cmd(&hci_read_local_version_information);
}

int a2dpwb_hci_get_state(void) {
    return (int)hci_get_state();
}

int a2dpwb_hci_can_send_command_now(void) {
    return hci_can_send_command_packet_now() ? 1 : 0;
}
