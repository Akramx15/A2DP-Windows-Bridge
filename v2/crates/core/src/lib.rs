//! A2DPWB's core, shared by the resident process (tray, IPC for the UI) and
//! the command line: the streaming state machine, the Bluetooth stack's
//! lifecycle, and the settings, profiles and devices of the settings folder
//! (v1's files, read and written in their format).

#![forbid(unsafe_code)]

pub mod adapter;
pub mod devices;
pub mod messages;
pub mod paths;
pub mod profiles;
mod service;
pub mod settings;
pub mod stream;

pub use messages::Msg;
pub use service::{Service, ServiceEvent, ServiceOptions};
pub use stream::{SourceRequest, StreamInfo, StreamRequest, StreamState, VolumeSync};
