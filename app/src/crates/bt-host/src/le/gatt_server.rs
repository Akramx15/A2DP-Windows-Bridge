//! The PC's GATT server (v1 le_audio::GattServer, le_audio_gatt_server.cpp).
//!
//! The PC is the GATT server for the services a headset reads from the
//! audio source: GAP and GATT (built by the core) and the modules' services
//! (GMCS, GTBS, TMAS, GMAS). The database is built with BTstack's
//! att_db_util while the stack starts ([`GattServerBuilder`], in
//! `LeModule::add_gatt_services` only); reads and writes are dispatched by
//! attribute handle to the module that added the characteristic
//! (`LeModule::on_att_read` / `on_att_write`), synchronously from BTstack's
//! att_server callbacks.
//!
//! The CCC descriptor of a NOTIFY / INDICATE characteristic is created at
//! value handle + 1 and handled here: [`GattServer::notify`] only sends to
//! clients that enabled it, and queues until ATT can send. Long reads
//! (offsets) and prepared writes are handled here; modules see whole
//! values. CCC values of bonded clients persist (BTstack's att_server in
//! the bond TLV, replayed on re-encryption); the GATT service has Service
//! Changed and a Database Hash ([`le_audio::db_hash`]); a database that
//! differs from the stored hash drops the stored CCC values and makes a
//! Service Changed indication due for clients that had it enabled (v1
//! check_stored_client_configs; log "GATT: Database Hash ..."). Keep the
//! first handles (GAP, then Service Changed and its CCC) as they are.

use std::collections::HashMap;

pub use le_audio::db_hash::AttUuid;

/// ATT_PROPERTY_* for [`GattServerBuilder::add_characteristic`]
pub const ATT_PROPERTY_READ: u16 = 0x02;
pub const ATT_PROPERTY_WRITE_WITHOUT_RESPONSE: u16 = 0x04;
pub const ATT_PROPERTY_WRITE: u16 = 0x08;
pub const ATT_PROPERTY_NOTIFY: u16 = 0x10;
pub const ATT_PROPERTY_INDICATE: u16 = 0x20;

/// Security of a read / write (BTstack ATT_SECURITY_*)
pub const ATT_SECURITY_NONE: u8 = 0;
pub const ATT_SECURITY_ENCRYPTED: u8 = 1;
pub const ATT_SECURITY_AUTHENTICATED: u8 = 2;

/// A read of a module's characteristic
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttRead {
    /// Connection handle of the client
    pub con: u16,
    /// Value handle
    pub handle: u16,
    /// > 0: Read Blob, the rest of a long read
    pub offset: u16,
}

/// A write of a module's characteristic (the whole value)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttWrite<'a> {
    pub con: u16,
    /// Value handle
    pub handle: u16,
    pub value: &'a [u8],
    /// Write Without Response / Write Command
    pub without_response: bool,
}

/// Builds the modules' services (`LeModule::add_gatt_services`): the
/// characteristics added here belong to the module the builder was made
/// for
pub struct GattServerBuilder<'a> {
    pub(crate) server: &'a mut GattServer,
    /// Index of the module in the registry
    pub(crate) owner: usize,
}

impl GattServerBuilder<'_> {
    /// Starts a service (primary, or secondary to include); returns its
    /// start handle
    pub fn begin_service(&mut self, uuid: AttUuid, primary: bool) -> u16 {
        todo!("Step 0 skeleton")
    }

    /// Includes an already added service (its start / end handle)
    pub fn include_service(&mut self, start_handle: u16, end_handle: u16, uuid16: u16) {
        todo!("Step 0 skeleton")
    }

    /// Adds a characteristic; returns its value handle (CCC: + 1 with
    /// NOTIFY / INDICATE). `properties`: ATT_PROPERTY_*; `read_security` /
    /// `write_security`: ATT_SECURITY_*.
    pub fn add_characteristic(&mut self, uuid: AttUuid, properties: u16, read_security: u8, write_security: u8) -> u16 {
        todo!("Step 0 skeleton")
    }

    /// Ends the service; returns its end handle (for include_service)
    pub fn end_service(&mut self) -> u16 {
        todo!("Step 0 skeleton")
    }
}

/// The server at run time (`ctx.gatt_server()`)
pub struct GattServer {
    /// Owner module per value handle (WP4)
    owners: HashMap<u16, usize>,
}

impl GattServer {
    /// Sends to `con` if it enabled notifications of `value_handle` (cut to
    /// ATT_MTU - 3); false if it did not or the link is gone
    pub fn notify(&mut self, con: u16, value_handle: u16, value: &[u8]) -> bool {
        todo!("Step 0 skeleton")
    }

    /// As [`GattServer::notify`], for indications
    pub fn indicate(&mut self, con: u16, value_handle: u16, value: &[u8]) -> bool {
        todo!("Step 0 skeleton")
    }

    /// To every connected client that enabled it
    pub fn notify_all(&mut self, value_handle: u16, value: &[u8]) {
        todo!("Step 0 skeleton")
    }

    /// The CCC value `con` wrote for `value_handle` (0 = none)
    pub fn client_config(&self, con: u16, value_handle: u16) -> u16 {
        todo!("Step 0 skeleton")
    }

    /// Connections that have written any CCC
    pub fn clients(&self) -> Vec<u16> {
        todo!("Step 0 skeleton")
    }

    /// ATT_MTU of a connection (23 until exchanged)
    pub fn mtu(&self, con: u16) -> u16 {
        todo!("Step 0 skeleton")
    }

    // ---- core only ----

    /// A new session: empty database
    pub(crate) fn new() -> GattServer {
        todo!("Step 0 skeleton")
    }

    /// GAP and GATT services (Service Changed, Database Hash)
    pub(crate) fn build_base(&mut self) {
        todo!("Step 0 skeleton")
    }

    /// The builder for module `owner`'s add_gatt_services
    pub(crate) fn builder(&mut self, owner: usize) -> GattServerBuilder<'_> {
        GattServerBuilder { server: self, owner }
    }

    /// Computes the Database Hash, checks the stored client configurations
    /// and starts att_server
    pub(crate) fn start(&mut self) {
        todo!("Step 0 skeleton")
    }

    /// The module owning `value_handle`, None for the core's own
    /// attributes
    pub(crate) fn owner(&self, value_handle: u16) -> Option<usize> {
        todo!("Step 0 skeleton")
    }

    pub(crate) fn on_disconnected(&mut self, con: u16) {
        todo!("Step 0 skeleton")
    }
}
