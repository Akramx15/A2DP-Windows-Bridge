//! Raw declarations: BTstack functions with plain signatures, and the glue in
//! csrc/shim.c for everything that involves BTstack's structs.
//!
//! Return types follow the C prototypes exactly (an `int` read as `u8`, or the
//! other way round, would be undefined).

use std::ffi::{c_char, c_int};

unsafe extern "C" {
    // ---- shim.c: run loop ----
    pub fn a2dpwb_runloop_init();
    pub fn a2dpwb_wake();
    pub fn a2dpwb_timer_start(id: u32, timeout_ms: u32) -> c_int;
    pub fn a2dpwb_timer_stop(id: u32);
    pub fn a2dpwb_timers_clear();

    // ---- shim.c: transport, dump, link keys, handlers ----
    pub fn a2dpwb_hci_init(is_async: c_int);
    pub fn a2dpwb_transport_deliver(packet_type: u8, packet: *const u8, size: u16) -> c_int;
    pub fn a2dpwb_transport_packet_sent();
    pub fn a2dpwb_intel_download_firmware(is_async: c_int, firmware_path: *const c_char);
    pub fn a2dpwb_hci_dump_init();
    pub fn a2dpwb_set_link_key_db(rust_store: c_int);
    pub fn a2dpwb_add_hci_event_handler();
    pub fn a2dpwb_a2dp_source_init();
    pub fn a2dpwb_avrcp_init();

    // ---- shim.c: GAP, SDP, endpoints, configuration ----
    pub fn a2dpwb_gap_setup(class_of_device: u32, page_timeout: u16);
    pub fn a2dpwb_sdp_register_records(controller_features: u16, target_features: u16);
    pub fn a2dpwb_create_stream_endpoint(
        slot: c_int,
        media_codec_type: u8,
        caps: *const u8,
        caps_len: u16,
        config: *const u8,
        config_len: u16,
    ) -> u8;
    pub fn a2dpwb_set_config_sbc(
        a2dp_cid: u16,
        local_seid: u8,
        remote_seid: u8,
        sampling_frequency: u16,
        channel_mode: c_int,
        block_length: c_int,
        subbands: c_int,
        allocation_method: c_int,
        min_bitpool: u8,
        max_bitpool: u8,
    ) -> u8;
    pub fn a2dpwb_set_config_aac(
        a2dp_cid: u16,
        local_seid: u8,
        remote_seid: u8,
        object_type: c_int,
        sampling_frequency: u32,
        channels: u8,
        bit_rate: u32,
        vbr: u8,
    ) -> u8;
    pub fn a2dpwb_set_config_other(a2dp_cid: u16, local_seid: u8, remote_seid: u8, info: *const u8, info_len: u8)
    -> u8;

    // ---- shim.c: state queries, HCI commands ----
    pub fn a2dpwb_acl_state(addr: *const u8, con_handle: *mut u16) -> c_int;
    pub fn a2dpwb_avdtp_state_for_addr(addr: *const u8, avdtp_cid: *mut u16) -> c_int;
    pub fn a2dpwb_avdtp_state_for_cid(avdtp_cid: u16) -> c_int;
    pub fn a2dpwb_cmd_set_afh_classification(map: *const u8) -> u8;
    pub fn a2dpwb_cmd_read_afh_channel_map(con_handle: u16) -> u8;
    pub fn a2dpwb_cmd_read_rssi(con_handle: u16) -> u8;
    pub fn a2dpwb_cmd_read_bd_addr() -> u8;
    pub fn a2dpwb_cmd_read_local_version() -> u8;
    pub fn a2dpwb_hci_get_state() -> c_int;
    pub fn a2dpwb_hci_can_send_command_now() -> c_int;

    // ---- BTstack: memory, run loop ----
    pub fn btstack_memory_init();
    pub fn btstack_memory_deinit();
    pub fn btstack_run_loop_execute();
    pub fn btstack_run_loop_trigger_exit();
    pub fn btstack_run_loop_deinit();

    // ---- BTstack: HCI, L2CAP, SDP ----
    pub fn hci_power_control(mode: c_int) -> c_int;
    pub fn hci_enable_custom_pre_init();
    pub fn hci_deinit();
    pub fn l2cap_init();
    pub fn l2cap_deinit();
    pub fn sdp_init();
    pub fn sdp_deinit();
    pub fn sdp_client_deinit();

    // ---- BTstack: GAP ----
    pub fn gap_inquiry_start(duration_in_1280ms_units: u8) -> c_int;
    pub fn gap_inquiry_stop() -> c_int;
    pub fn gap_ssp_confirmation_response(addr: *const u8) -> c_int;
    pub fn gap_pin_code_response(addr: *const u8, pin: *const c_char) -> c_int;
    pub fn gap_disconnect(handle: u16) -> u8;
    pub fn gap_drop_link_key_for_bd_addr(addr: *mut u8);

    // ---- BTstack: A2DP source ----
    pub fn a2dp_source_establish_stream(remote_addr: *mut u8, avdtp_cid: *mut u16) -> u8;
    pub fn a2dp_source_start_stream(a2dp_cid: u16, local_seid: u8) -> u8;
    pub fn a2dp_source_pause_stream(a2dp_cid: u16, local_seid: u8) -> u8;
    pub fn a2dp_source_disconnect(a2dp_cid: u16) -> u8;
    pub fn a2dp_source_stream_endpoint_request_can_send_now(a2dp_cid: u16, local_seid: u8);
    pub fn a2dp_max_media_payload_size(a2dp_cid: u16, local_seid: u8) -> c_int;
    pub fn a2dp_source_stream_send_media_payload_rtp(
        a2dp_cid: u16,
        local_seid: u8,
        marker: u8,
        timestamp: u32,
        payload: *const u8,
        payload_size: u16,
    ) -> u8;
    pub fn a2dp_source_stream_send_media_packet(a2dp_cid: u16, local_seid: u8, packet: *const u8, size: u16) -> u8;
    pub fn a2dp_source_deinit();

    // ---- BTstack: AVRCP ----
    pub fn avrcp_connect(remote_addr: *mut u8, avrcp_cid: *mut u16) -> u8;
    pub fn avrcp_disconnect(avrcp_cid: u16) -> u8;
    pub fn avrcp_controller_set_absolute_volume(avrcp_cid: u16, volume: u8) -> u8;
    pub fn avrcp_controller_enable_notification(avrcp_cid: u16, event_id: c_int) -> u8;
    pub fn avrcp_target_support_event(avrcp_cid: u16, event_id: c_int) -> u8;
    pub fn avrcp_target_set_playback_status(avrcp_cid: u16, playback_status: c_int) -> u8;
    pub fn avrcp_target_play_status(avrcp_cid: u16, song_length_ms: u32, song_position_ms: u32, status: c_int) -> u8;
    pub fn avrcp_target_deinit();
    pub fn avrcp_controller_deinit();
    pub fn avrcp_deinit();

    // ---- BTstack: chipsets ----
    pub fn btstack_chipset_realtek_get_num_usb_controllers() -> u16;
    pub fn btstack_chipset_realtek_get_vendor_product_id(index: u16, vendor_id: *mut u16, product_id: *mut u16);
    pub fn btstack_chipset_realtek_set_product_id(id: u16);
    pub fn btstack_chipset_realtek_set_firmware_folder_path(path: *const c_char);
    pub fn btstack_chipset_realtek_set_config_folder_path(path: *const c_char);
    pub fn btstack_chipset_realtek_instance() -> *const core::ffi::c_void;
    pub fn btstack_chipset_bcm_set_hcd_file_path(path: *const c_char);
    pub fn btstack_chipset_bcm_instance() -> *const core::ffi::c_void;
    pub fn hci_set_chipset(chipset: *const core::ffi::c_void);
}

#[cfg(feature = "sbc-reference")]
unsafe extern "C" {
    pub fn a2dpwb_sbc_ref_new(
        sample_rate: u16,
        channel_mode: c_int,
        blocks: c_int,
        subbands: c_int,
        allocation: c_int,
        bitpool: c_int,
    ) -> *mut core::ffi::c_void;
    pub fn a2dpwb_sbc_ref_encode(state: *mut core::ffi::c_void, pcm: *const i16, out: *mut u8, out_cap: c_int) -> c_int;
    pub fn a2dpwb_sbc_ref_free(state: *mut core::ffi::c_void);
}
