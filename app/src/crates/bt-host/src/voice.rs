//! HFP audio (headset microphone, developer mode): the audio connection on
//! the HFP service level connection, and SCO data both ways.
//!
//! [`crate::BtHost::voice_start`] hands a [`VoiceTx`] and a [`VoiceRx`] to
//! the BTstack thread and asks the HFP audio gateway for the audio
//! connection (codec negotiation first). Once it is up, received SCO
//! payloads go to the `VoiceRx`, and on every HCI_EVENT_SCO_CAN_SEND_NOW the
//! `VoiceTx` fills one packet of the length BTstack wants for the
//! connection. The codec is the caller's business: payloads are what goes
//! over HCI (CVSD: 16-bit linear PCM, mSBC / LC3-SWB: H2 framed frames).

use btstack_sys::Stack;
use btstack_sys::consts::{
    HCI_EVENT_PACKET, HCI_EVENT_SCO_CAN_SEND_NOW, HCI_SCO_DATA_PACKET, HFP_CODEC_CVSD, HFP_CODEC_LC3_SWB,
    HFP_CODEC_MSBC,
};

use crate::events::{event_type, le16, u8_at};

/// What happens to the HFP audio connection ([`crate::HostEvent::Voice`])
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceEvent {
    /// The audio connection is up: the negotiated codec (HFP codec id: 1
    /// CVSD, 2 mSBC, 3 LC3-SWB), the SCO handle, and the packet lengths
    /// of the (e)SCO link over the air (0 for SCO)
    Established { codec: u8, sco_handle: u16, rx_len: u16, tx_len: u16 },
    /// The audio connection went down (asked for, or by the headphones)
    Released,
    /// The audio connection could not be opened (HCI / BTstack status)
    Failed { status: u8 },
}

/// Payload bytes to send, on the BTstack thread
pub trait VoiceTx: Send {
    /// Fills `out` completely (the SCO payload of one packet)
    fn fill(&mut self, out: &mut [u8]);
}

/// Received payloads, on the BTstack thread
pub trait VoiceRx: Send {
    /// One SCO packet's payload; `status` is the HCI packet status flag (0
    /// correctly received, 1 possibly invalid, 2 no data, 3 partially lost)
    fn packet(&mut self, payload: &[u8], status: u8);
}

/// Why a voice request failed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostError {
    /// The stack was set up without voice ([`crate::HostConfig::voice`])
    NotEnabled,
    /// No HFP service level connection (yet)
    NotConnected,
    /// An audio connection is already open or being opened
    Busy,
    /// BTstack refused the request (its status)
    Refused(u8),
    /// The BTstack thread did not answer
    NoAnswer,
}

/// "CVSD", "mSBC", "LC3-SWB"
pub fn codec_name(codec: u8) -> &'static str {
    match codec {
        HFP_CODEC_CVSD => "CVSD",
        HFP_CODEC_MSBC => "mSBC",
        HFP_CODEC_LC3_SWB => "LC3-SWB",
        _ => "unknown",
    }
}

/// A SCO data packet: (handle, packet status flag, payload)
pub fn parse_sco(packet: &[u8]) -> Option<(u16, u8, &[u8])> {
    let head = le16(packet, 0)?;
    let len = u8_at(packet, 2)? as usize;
    let payload = packet.get(3..3 + len)?;
    Some((head & 0x0FFF, ((head >> 12) & 0x03) as u8, payload))
}

/// The voice state on the BTstack thread
#[derive(Default)]
pub(crate) struct Voice {
    tx: Option<Box<dyn VoiceTx>>,
    rx: Option<Box<dyn VoiceRx>>,
    /// The ACL of the audio connection asked for (between voice_start and
    /// Established / Failed)
    opening: Option<u16>,
    /// The open audio connection: SCO handle and its ACL
    sco: Option<(u16, u16)>,
    buf: Vec<u8>,
    /// Packets sent / failed sends, for the log at the end
    sent: u64,
    send_errors: u64,
    received: u64,
}

impl Voice {
    /// [`crate::BtHost::voice_start`] on the BTstack thread
    pub fn start(
        &mut self,
        stack: &Stack,
        enabled: bool,
        acl: Option<u16>,
        tx: Box<dyn VoiceTx>,
        rx: Box<dyn VoiceRx>,
    ) -> Result<(), HostError> {
        self.check(enabled, acl)?;
        let Some(acl) = acl else { return Err(HostError::NotConnected) };
        let status = stack.hfp_ag_audio_connect(acl);
        if status != 0 {
            tracing::info!("HFP: audio connection refused by BTstack (0x{status:02x})");
            return Err(HostError::Refused(status));
        }
        tracing::info!("HFP: opening the audio connection on ACL 0x{acl:04x}");
        self.tx = Some(tx);
        self.rx = Some(rx);
        self.opening = Some(acl);
        self.sent = 0;
        self.send_errors = 0;
        self.received = 0;
        Ok(())
    }

    /// What [`Voice::start`] would answer, opening nothing
    pub fn check(&self, enabled: bool, acl: Option<u16>) -> Result<(), HostError> {
        if !enabled {
            return Err(HostError::NotEnabled);
        }
        if acl.is_none() {
            return Err(HostError::NotConnected);
        }
        if self.opening.is_some() || self.sco.is_some() {
            return Err(HostError::Busy);
        }
        Ok(())
    }

    /// [`crate::BtHost::voice_attach`] on the BTstack thread: an audio
    /// connection the headphones opened gets its `tx` / `rx`
    pub fn attach(&mut self, stack: &Stack, tx: Box<dyn VoiceTx>, rx: Box<dyn VoiceRx>) -> Result<(), HostError> {
        let Some((sco_handle, _)) = self.sco else { return Err(HostError::NotConnected) };
        if self.tx.is_some() {
            return Err(HostError::Busy);
        }
        tracing::info!("HFP: taking over the audio connection the headphones opened");
        self.tx = Some(tx);
        self.rx = Some(rx);
        stack.sco_request_can_send_now(sco_handle);
        Ok(())
    }

    /// [`crate::BtHost::voice_stop`] on the BTstack thread
    pub fn stop(&mut self, stack: &Stack) {
        self.tx = None;
        self.rx = None;
        let acl = self.sco.map(|(_, acl)| acl).or(self.opening);
        if let Some(acl) = acl {
            let status = stack.hfp_ag_audio_release(acl);
            tracing::info!("HFP: closing the audio connection (0x{status:02x})");
        }
    }

    /// HFP_SUBEVENT_AUDIO_CONNECTION_ESTABLISHED (the whole HFP meta event)
    pub fn on_established(&mut self, stack: &Stack, p: &[u8]) -> Option<crate::HostEvent> {
        let status = u8_at(p, 5)?;
        self.opening = None;
        if status != 0 {
            tracing::info!("HFP: audio connection failed (0x{status:02x})");
            self.tx = None;
            self.rx = None;
            return Some(crate::HostEvent::Voice(VoiceEvent::Failed { status }));
        }
        let (acl, sco_handle) = (le16(p, 3)?, le16(p, 6)?);
        let codec = u8_at(p, 14)?;
        let (packet_types, rx_len, tx_len) = (le16(p, 15)?, le16(p, 17)?, le16(p, 19)?);
        let packet_length = stack.sco_packet_length(sco_handle);
        tracing::info!(
            "HFP: audio connection established, codec {} ({codec}), SCO handle 0x{sco_handle:04x}, packet types 0x{packet_types:04x}, air rx {rx_len} / tx {tx_len} bytes, HCI packet {packet_length} bytes",
            codec_name(codec)
        );
        self.sco = Some((sco_handle, acl));
        if self.tx.is_some() {
            stack.sco_request_can_send_now(sco_handle);
        } else {
            tracing::info!("HFP: audio connection opened by the headphones, nothing to send");
        }
        Some(crate::HostEvent::Voice(VoiceEvent::Established { codec, sco_handle, rx_len, tx_len }))
    }

    /// HFP_SUBEVENT_AUDIO_CONNECTION_RELEASED, or the service level
    /// connection went down: an event if an audio connection was open or
    /// being opened
    pub fn on_released(&mut self) -> Option<crate::HostEvent> {
        let was = self.sco.take().is_some() || self.opening.take().is_some();
        self.tx = None;
        self.rx = None;
        if !was {
            return None;
        }
        tracing::info!(
            "HFP: audio connection released ({} SCO packets sent, {} send errors, {} received)",
            self.sent,
            self.send_errors,
            self.received
        );
        Some(crate::HostEvent::Voice(VoiceEvent::Released))
    }

    /// A packet of [`btstack_sys::Source::Sco`]
    pub fn on_sco(&mut self, stack: &Stack, packet_type: u8, p: &[u8]) {
        let Some((sco_handle, _)) = self.sco else { return };
        match packet_type {
            HCI_SCO_DATA_PACKET => {
                let Some((handle, status, payload)) = parse_sco(p) else { return };
                if handle != sco_handle {
                    return;
                }
                self.received += 1;
                if let Some(rx) = self.rx.as_mut() {
                    rx.packet(payload, status);
                }
            }
            HCI_EVENT_PACKET if event_type(p) == Some(HCI_EVENT_SCO_CAN_SEND_NOW) => self.send(stack, sco_handle),
            _ => {}
        }
    }

    /// One packet from the VoiceTx, then the next can-send-now request
    fn send(&mut self, stack: &Stack, sco_handle: u16) {
        let Some(tx) = self.tx.as_mut() else { return };
        let length = stack.sco_packet_length(sco_handle) as usize;
        if length <= 3 {
            return;
        }
        self.buf.resize(length - 3, 0);
        tx.fill(&mut self.buf);
        let status = stack.sco_send(sco_handle, &self.buf);
        if status == 0 {
            self.sent += 1;
        } else {
            self.send_errors += 1;
            if self.send_errors <= 3 {
                tracing::debug!("SCO send: 0x{status:02x}");
            }
        }
        stack.sco_request_can_send_now(sco_handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sco_packet_fields() {
        // handle 0x0123, status 2 (no data), 3 payload bytes
        let p = [0x23, 0x21, 3, 9, 8, 7, 0xEE];
        assert_eq!(parse_sco(&p), Some((0x0123, 2, &[9u8, 8, 7][..])));
        assert_eq!(parse_sco(&[0x23, 0x01, 4, 1, 2]), None);
        assert_eq!(parse_sco(&[0x23]), None);
    }

    #[test]
    fn codec_names() {
        assert_eq!(codec_name(1), "CVSD");
        assert_eq!(codec_name(2), "mSBC");
        assert_eq!(codec_name(3), "LC3-SWB");
    }
}
