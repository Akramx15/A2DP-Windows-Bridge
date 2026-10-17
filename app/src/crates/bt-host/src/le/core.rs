//! The LE core on the BTstack thread (v1 le_audio::Core, le_audio_core.cpp):
//! stack init / deinit of the LE part, LE scan (extended scan fallback),
//! connect with RPA / identity merge, SM LE Secure Connections Just Works
//! bonding (a bond the device lost is deleted and pairing repeated once),
//! bonds in <bond_dir>\le_audio_bonds.tlv, GATT discovery (included /
//! secondary services too), PACS / ASCS reads, refusing a stream without
//! ISO buffers ("adapter does not support LE Audio (no ISO buffers)"), the
//! BAP stream, and the module dispatch.
//!
//! [`LeCtx`] is what module hooks and GATT / timer callbacks get: the core's
//! BTstack-thread state for one call. Its method signatures are the
//! contract for the module agents; [`LeCore`]'s fields are WP4's.

use std::sync::{Arc, Mutex};

use btstack_sys::Stack;
use le_audio::pacs_ascs::{Ase, PacRecord};
use le_audio::presets::PresetTable;
use le_audio::sdu_queue::SduQueue;

use super::gatt_client::{GattChar, GattClient, GattService};
use super::gatt_server::GattServer;
use super::module::LeModule;
use super::{LeAddr, LeDeviceId, LeStreamStatus};

/// hci_con_handle_t of no connection
pub const CON_HANDLE_INVALID: u16 = 0xFFFF;

/// One LE Audio peer (e.g. a headset, or one earbud of a pair), owned by
/// the core, only touched on the BTstack thread (v1 LeAudioDevice). Profile
/// modules read it and keep their own state keyed by `id`.
#[derive(Debug, Clone)]
pub struct LeDevice {
    pub id: LeDeviceId,
    /// The identity address (public or static random) once SM learned it
    /// (pairing, or an RPA resolved with the stored IRK); before that the
    /// address it was first connected with. Devices are keyed by it.
    pub addr: LeAddr,
    /// `addr` is the identity from SM
    pub identity_known: bool,
    /// Address of the current / last link (maybe an RPA)
    pub peer_addr: LeAddr,
    pub name: String,

    // ---- link ----
    /// ACL handle, CON_HANDLE_INVALID when down
    pub acl: u16,
    pub encrypted: bool,
    /// SM pairing in progress
    pub pairing: bool,
    /// An LTK was stored when we connected
    pub bonded_before: bool,
    pub pair_attempts: u32,
    /// Security done, discovery started
    pub secured: bool,
    /// GATT cache + PACS + ASCS read
    pub gatt_ready: bool,
    /// `le_connect` may return
    pub ready: bool,
    /// Why it went down / failed (HCI / ATT / SM)
    pub last_error: u8,

    // ---- GATT cache (filled once per connection) ----
    pub services: Vec<GattService>,

    // ---- PACS (sink direction: we are the audio source) ----
    pub sink_pacs: Vec<PacRecord>,
    pub sink_locations: u32,
    pub available_sink_contexts: u16,
    pub available_source_contexts: u16,
    pub supported_sink_contexts: u16,
    pub supported_source_contexts: u16,
    pub has_pacs: bool,
    pub has_ascs: bool,

    // ---- ASCS ----
    pub ases: Vec<Ase>,
    pub ase_cp_handle: u16,
}

impl LeDevice {
    pub fn connected(&self) -> bool {
        self.acl != CON_HANDLE_INVALID
    }

    /// The first service with this UUID (primary or secondary)
    pub fn find_service(&self, uuid16: u16) -> Option<&GattService> {
        todo!("Step 0 skeleton")
    }

    /// All characteristics with this UUID in all services with
    /// `service_uuid16` (0 = any service)
    pub fn find_chars(&self, service_uuid16: u16, char_uuid16: u16) -> Vec<&GattChar> {
        todo!("Step 0 skeleton")
    }

    pub fn find_char(&self, service_uuid16: u16, char_uuid16: u16) -> Option<&GattChar> {
        todo!("Step 0 skeleton")
    }

    pub fn ase(&self, ase_id: u8) -> Option<&Ase> {
        todo!("Step 0 skeleton")
    }

    pub fn ase_mut(&mut self, ase_id: u8) -> Option<&mut Ase> {
        todo!("Step 0 skeleton")
    }

    pub fn sink_ase_count(&self) -> usize {
        todo!("Step 0 skeleton")
    }
}

/// A timer of [`LeCtx::run_after`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LeTimerId(pub u32);

/// A timer / posted callback
pub type LeCallback = Box<dyn FnOnce(&mut LeCtx<'_>)>;

/// Completion of a `BtHost::le_call` operation: true = success
pub type LeDone = Box<dyn FnOnce(bool) + Send>;

/// The LE core's BTstack-thread state (WP4: devices, scan results, RPA
/// cache, GATT client and server, presets, unicast, ISO sender, modules,
/// posted callbacks, timers). Lives in HostApp while LE Audio is on.
pub struct LeCore {
    // WP4
}

/// The LE core for one hook or callback (BTstack thread)
pub struct LeCtx<'a> {
    pub(crate) stack: &'a Stack,
    pub(crate) core: &'a mut LeCore,
}

impl<'a> LeCtx<'a> {
    pub fn stack(&self) -> &Stack {
        self.stack
    }

    pub fn device(&self, id: LeDeviceId) -> Option<&LeDevice> {
        todo!("Step 0 skeleton")
    }

    pub fn device_mut(&mut self, id: LeDeviceId) -> Option<&mut LeDevice> {
        todo!("Step 0 skeleton")
    }

    /// The device with this ACL handle (connected)
    pub fn device_by_handle(&self, acl: u16) -> Option<&LeDevice> {
        todo!("Step 0 skeleton")
    }

    /// All devices the core knows (connected or not)
    pub fn device_ids(&self) -> Vec<LeDeviceId> {
        todo!("Step 0 skeleton")
    }

    /// The GATT client queue (never call BTstack's gatt_client directly)
    pub fn gatt(&mut self) -> &mut GattClient {
        todo!("Step 0 skeleton")
    }

    /// The PC's GATT server at run time (notify / indicate / CCC state)
    pub fn gatt_server(&mut self) -> &mut GattServer {
        todo!("Step 0 skeleton")
    }

    /// The stream presets of this session (v1 add_stream_preset /
    /// find_stream_preset)
    pub fn presets(&self) -> &PresetTable {
        todo!("Step 0 skeleton")
    }

    pub fn presets_mut(&mut self) -> &mut PresetTable {
        todo!("Step 0 skeleton")
    }

    /// ISO buffers of the controller (LE Read Buffer Size v2):
    /// (ISO_Data_Packet_Length, Total_Num_ISO_Data_Packets), (0, 0) without
    /// ISO support
    pub fn iso_buffers(&self) -> (u16, u8) {
        todo!("Step 0 skeleton")
    }

    /// The running stream, None when idle
    pub fn stream_status(&self) -> Option<LeStreamStatus> {
        todo!("Step 0 skeleton")
    }

    /// The "media keys" setting (MCP refuses commands while it is off, as
    /// AVRCP buttons are ignored then)
    pub fn media_keys(&self) -> bool {
        todo!("Step 0 skeleton")
    }

    /// Another module (or this one, from a GATT / timer callback); None
    /// while its own hook runs or when it is not registered
    pub fn module_mut<T: LeModule>(&mut self) -> Option<&mut T> {
        todo!("Step 0 skeleton")
    }

    /// Runs `f` on the BTstack thread after `ms` (v1 btstack timers of the
    /// modules); dropped when the session ends
    pub fn run_after(&mut self, ms: u32, f: LeCallback) -> LeTimerId {
        todo!("Step 0 skeleton")
    }

    pub fn cancel_timer(&mut self, timer: LeTimerId) {
        todo!("Step 0 skeleton")
    }
}

/// Queues closures for the BTstack thread from any thread without waiting
/// (v1 Core::post). A closure still queued when the session ends never
/// runs.
#[derive(Clone)]
pub struct LePoster {
    // WP4: the job sender and the run loop's Remote
}

impl LePoster {
    pub fn post(&self, f: impl FnOnce(&mut LeCtx<'_>) + Send + 'static) {
        todo!("Step 0 skeleton")
    }
}

/// The stream's jitter buffer, shared by [`super::LeSduSink`] (encode
/// thread) and the ISO sender (BTstack thread)
pub type SharedSduQueue = Arc<Mutex<SduQueue>>;
