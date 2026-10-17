//! Serialized GATT client operations (v1 le_audio::gatt_client).
//!
//! BTstack's gatt_client runs one query per connection at a time and
//! refuses a second one (GATT_CLIENT_IN_WRONG_STATE). Every GATT client
//! operation of the core and of the modules therefore goes through this
//! queue: a FIFO per connection, the next operation starts when the
//! previous one completed, and its callback gets the result. All on the
//! BTstack thread; callbacks too. Status: 0 = success, else an ATT error
//! code (e.g. [`ATT_ERROR_HCI_DISCONNECT_RECEIVED`] when the link went
//! down) or the BTstack status of a query that could not start. In a
//! callback, check that the device is still the same link
//! (`ctx.device(id)` and its `acl == con`).

use std::collections::HashMap;

use super::core::LeCtx;

/// Status of the operations still queued or running when a link goes down
pub const ATT_ERROR_HCI_DISCONNECT_RECEIVED: u8 = 0x1F;

/// ATT_PROPERTY_* of a characteristic declaration
pub const PROPERTY_READ: u16 = 0x02;
pub const PROPERTY_WRITE_WITHOUT_RESPONSE: u16 = 0x04;
pub const PROPERTY_WRITE: u16 = 0x08;
pub const PROPERTY_NOTIFY: u16 = 0x10;
pub const PROPERTY_INDICATE: u16 = 0x20;

/// A characteristic of the GATT cache
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GattChar {
    /// 0 for a 128-bit UUID
    pub uuid16: u16,
    /// BTstack order
    pub uuid128: [u8; 16],
    /// Declaration
    pub start_handle: u16,
    pub value_handle: u16,
    /// Last descriptor
    pub end_handle: u16,
    /// PROPERTY_*
    pub properties: u16,
}

/// A service of the GATT cache
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GattService {
    /// 0 for a 128-bit UUID
    pub uuid16: u16,
    pub uuid128: [u8; 16],
    pub start_handle: u16,
    pub end_handle: u16,
    /// false: secondary, found as an included service
    pub primary: bool,
    /// Start handle of the including service (secondary)
    pub included_by: u16,
    pub chars: Vec<GattChar>,
}

/// Completion of a write / subscribe: the status
pub type GattDone = Box<dyn FnOnce(&mut LeCtx<'_>, u8)>;
/// Completion of a read: the status and the whole value (long reads
/// included)
pub type GattValue = Box<dyn FnOnce(&mut LeCtx<'_>, u8, &[u8])>;
/// A notification / indication value
pub type GattNotify = Box<dyn FnMut(&mut LeCtx<'_>, &[u8])>;
/// Completion of a discovery: the status and the services
pub type GattDiscovered = Box<dyn FnOnce(&mut LeCtx<'_>, u8, Vec<GattService>)>;

/// The per-connection queue (in the LE core; reach it with `ctx.gatt()`)
pub struct GattClient {
    /// Operations queued or running per connection (WP4)
    queues: HashMap<u16, usize>,
}

impl GattClient {
    /// Read a characteristic value (a long read when it does not fit the
    /// MTU)
    pub fn read(&mut self, con: u16, value_handle: u16, done: GattValue) {
        todo!("Step 0 skeleton")
    }

    /// Write with response (a long write when the value does not fit the
    /// MTU)
    pub fn write(&mut self, con: u16, value_handle: u16, value: Vec<u8>, done: Option<GattDone>) {
        todo!("Step 0 skeleton")
    }

    /// Write without response, in order with the queued operations
    pub fn write_without_response(&mut self, con: u16, value_handle: u16, value: Vec<u8>) {
        todo!("Step 0 skeleton")
    }

    /// Enable notifications (or indications) of a characteristic: writes its
    /// CCC descriptor. Values arrive at the [`GattClient::listen`]
    /// callbacks.
    pub fn subscribe(&mut self, con: u16, chr: &GattChar, indications: bool, done: Option<GattDone>) {
        todo!("Step 0 skeleton")
    }

    /// Discover all primary services, their included (secondary) services
    /// and all characteristics. The core runs this once per connection into
    /// `LeDevice::services`; modules use that cache.
    pub fn discover_all(&mut self, con: u16, done: GattDiscovered) {
        todo!("Step 0 skeleton")
    }

    /// Notifications / indications of `value_handle` on `con`. Dropped when
    /// the connection goes down.
    pub fn listen(&mut self, con: u16, value_handle: u16, cb: GattNotify) {
        todo!("Step 0 skeleton")
    }

    /// Operations queued or running on `con`
    pub fn pending(&self, con: u16) -> usize {
        todo!("Step 0 skeleton")
    }

    // ---- core only ----

    pub(crate) fn new() -> GattClient {
        todo!("Step 0 skeleton")
    }

    /// A new LE connection
    pub(crate) fn attach(&mut self, con: u16) {
        todo!("Step 0 skeleton")
    }

    /// The connection went down: the running and queued operations
    /// complete with ATT_ERROR_HCI_DISCONNECT_RECEIVED and the listeners
    /// are dropped
    pub(crate) fn detach(&mut self, con: u16) {
        todo!("Step 0 skeleton")
    }
}
