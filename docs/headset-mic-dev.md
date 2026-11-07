# Headset microphone (developer note)

Status: in development on `feat/headset-mic`. Not user-facing yet: every part
of it is behind developer mode.

This note is the contract between the parts of the feature, which were
written in parallel. Each section names the crate that owns it and the API the
other parts rely on. Keep it current when an API changes.

## What the feature is

"Headset mode" lets the headphones' microphone reach Windows, and sends the
PC's audio to the headphones over the same voice link:

- **Classic devices (HFP):** the A2DP stream is suspended, an HFP audio
  connection (eSCO/SCO) is opened on the existing service level connection,
  and both directions run over it. The codec is negotiated: LC3-SWB (32 kHz),
  mSBC (16 kHz) or CVSD (8 kHz), in that preference order. Playback is mono
  call quality while headset mode is on; this is inherent to HFP.
- **LE Audio devices:** the unicast stream becomes bidirectional (a
  conversational configuration): the sink ASEs keep playing LC3 at the media
  preset, and a source ASE sends the microphone to the PC on the same CIS
  (or on its own CIS). Playback quality does not change.

The microphone audio is **rendered** to a Windows output device the user picks
(a virtual audio cable such as VB-CABLE's "CABLE Input"); applications then
see the cable's capture side ("CABLE Output") as a microphone. BlueCaster has
no audio driver of its own, so this is the only way without a signed kernel
driver.

## Settings and IPC (crate `ipc`, core `state.rs`, UI)

Per device, in `Profile` (`app/src/crates/ipc/src/protocol.rs`), all with
`#[serde(default)]`:

| Field | Type | Meaning |
|---|---|---|
| `headset_mic` | `bool` | Headset mode for this device. Default false. |
| `mic_device_id` | `String` | Render endpoint id that receives the microphone. Empty = none chosen (headset mode stays off and the UI says so). |
| `mic_device_name` | `String` | Friendly name, for display when the device is missing. |

In `StreamStatus`: `voice: Option<VoiceStatus>` (`#[serde(default)]
#[ts(optional = nullable)]`), where

```rust
pub struct VoiceStatus {
    /// "cvsd" | "msbc" | "lc3_swb" | "lc3"
    pub codec: String,
    /// "hfp" | "le"
    pub link: String,
    /// Sampling rate of the voice link in Hz (8000 / 16000 / 32000 / 48000 ...)
    pub rate: u32,
    /// Microphone output device in use (id), empty while not rendering
    pub mic_device_id: String,
}
```

`voice` is `Some` only while a voice link carries audio. The core clears it on
Idle / Error / Reconnecting like the battery.

The settings count only in developer mode (`settings.rs` gate helper
`headset_mic_in_force()`, same pattern as `capture_source()`). Changing
`headset_mic` of the profile in use switches **in place** (no stream restart)
for classic devices; for LE Audio entries it restarts the LE stream.

CLI (`app/src/core/src/cli.rs`): `--headset-mic` (on), `--mic-output <name>`
(render device by friendly-name substring; the emulator runs pass the Windows
default or a cable). No CLI flag for the codec: the negotiation decides. For
LE: `--headset-mic` on an `--le-audio` run requests the conversational
configuration.

Language keys (all 8 files): `ui.headset_mic` ("Headset microphone"),
`ui.headset_mic_hint`, `ui.mic_output` ("Microphone output"),
`ui.headset_mic_quality` (the call-quality warning, shown only for classic
devices), `ui.headset_mic_no_output` (no output device chosen),
`status.voice_codec` ("Voice: %s") and codec names `codec.cvsd`,
`codec.msbc`, `codec.lc3_swb`. Hints stay under 120 characters.

## Voice audio plumbing (new crate `app/src/crates/voice-link`)

`#![forbid(unsafe_code)]`; deps: `wasapi`, `audio-capture` (for `Format`,
`Sink`, `render_devices`), `rtrb`, `tracing`. Add `voice_link` to
`OWN_TARGETS` in `app/src/core/src/logging.rs`.

```rust
/// PCM going to the Windows render endpoint (the headset microphone).
pub struct Renderer { .. }
impl Renderer {
    /// Opens the render endpoint `device_id` (WASAPI shared, event-driven,
    /// autoconvert on, MMCSS "Audio" thread "voice-render"), consuming mono
    /// i16 at `rate` Hz. Prefill `prefill_ms` before the first write; silence
    /// on underrun; a slow drift controller drops/duplicates a sample when
    /// the queue drifts past the prefill by more than one period.
    pub fn open(device_id: &str, rate: u32, prefill_ms: u32) -> Result<Renderer, RenderError>;
    /// The producer side: lock-free, callable from the BTstack or decode thread.
    pub fn producer(&self) -> PcmProducer;
    pub fn stats(&self) -> RenderStats; // frames written, underruns, dropped
}
pub struct PcmProducer { .. }
impl PcmProducer { pub fn push(&mut self, mono_i16: &[i16]) -> usize /* frames accepted */; }

/// Capture -> mono i16 at a fixed rate, for the uplink. Implements
/// audio_capture::Sink. Downmix: average of all channels. The capture is
/// opened with `preferred_rate = rate` so Windows resamples; the sink only
/// converts the sample type and downmixes. If the capture format's rate still
/// differs, a windowed-sinc (or polyphase) resampler is used, never plain
/// linear interpolation when downsampling.
pub struct UplinkSink { .. }
impl UplinkSink {
    pub fn new(rate: u32, capacity_ms: u32) -> (UplinkSink, UplinkConsumer);
}
pub struct UplinkConsumer { .. }
impl UplinkConsumer {
    /// Pops up to `out.len()` mono samples; fills the rest with zeros when
    /// short and counts an underrun. Never blocks.
    pub fn pop(&mut self, out: &mut [i16]) -> usize;
}

/// HFP voice codecs over SCO payloads.
pub enum VoiceCodec { Cvsd, Msbc, Lc3Swb }
impl VoiceCodec {
    pub fn rate(self) -> u32;             // 8000 / 16000 / 32000
    pub fn from_hfp_id(id: u8) -> Option<VoiceCodec>; // 1 / 2 / 3
}
/// Encoder side: PCM in, SCO payload bytes out, as a byte stream the SCO
/// sender drains in whatever packet length the controller wants.
pub struct VoiceEncoder { .. }
impl VoiceEncoder {
    pub fn new(codec: VoiceCodec) -> VoiceEncoder;
    /// Pulls `out.len()` payload bytes, encoding frames from `pcm` as needed.
    /// CVSD: 16-bit LE PCM pass-through. mSBC: H2 header + 57-byte frame +
    /// 1 padding byte per 120 samples (60 bytes). LC3-SWB: H2 header +
    /// 58-byte frame per 240 samples (60 bytes).
    pub fn fill(&mut self, pcm: &mut UplinkConsumer, out: &mut [u8]);
}
/// Decoder side: SCO payloads (with the HCI packet status flag) in, PCM out,
/// with packet loss concealment for bad or missing packets.
pub struct VoiceDecoder { .. }
impl VoiceDecoder {
    pub fn new(codec: VoiceCodec) -> VoiceDecoder;
    /// `status`: HCI SCO packet status flag (0 ok, 1 possibly invalid,
    /// 2 no data, 3 partially lost). Decoded PCM goes to `out` (push).
    pub fn decode(&mut self, payload: &[u8], status: u8, out: &mut PcmProducer);
}
```

The codec primitives come from `btstack-sys` (safe wrappers in
`btstack-sys/src/voice.rs`): the Bluedroid mSBC encoder (`btstack_sbc_encoder_bluedroid`,
`SBC_MODE_mSBC`), the Bluedroid SBC decoder with `btstack_sbc_plc`
(`btstack_sbc_decoder_bluedroid`, mSBC mode; it does H2 sync and PLC itself),
and `btstack_cvsd_plc`. LC3-SWB uses `lc3_sys::{Lc3Encoder, Lc3Decoder}`
(32 kHz, 7.5 ms, 58 octets) with H2 framing/sync in Rust (`voice_link::h2`).

## SCO transport (crates `hci-transport`, `win-sys`, `btstack-sys`)

`btstack_config.h` gains `ENABLE_SCO_OVER_HCI`, `ENABLE_HFP_WIDE_BAND_SPEECH`,
`ENABLE_HFP_SUPER_WIDE_BAND_SPEECH`.

`HciBackend` (`btstack-sys/src/transport.rs`) gains

```rust
/// BTstack's set_sco_config: voice setting and number of SCO connections.
/// The USB backend starts/stops the isochronous pipes here.
fn set_sco_config(&self, voice_setting: u16, connections: u8) {}
/// Whether this backend is an "H2" (USB isochronous) transport: BTstack then
/// sizes SCO payloads to 3 ms of audio and uses the transport's
/// HCI_EVENT_SCO_CAN_SEND_NOW. TCP returns false.
fn is_h2(&self) -> bool { false }
```

shim.c keeps `transport_sync` / `transport_async` for non-H2 backends and adds
`transport_h2` whose name starts with "H2" (`"H2_BLUECASTER"`), chosen by
`is_h2()` at install. shim.c registers a SCO packet handler
(`hci_register_sco_packet_handler`) before any SCO can-send request, which
forwards to Rust as `SOURCE_SCO`.

USB (`hci-transport/src/usb.rs`): interface 1 is claimed through nusb; the
isochronous transfers go through WinUSB directly (`WinUsb_RegisterIsochBuffer`,
`WinUsb_ReadIsochPipeAsap`, `WinUsb_WriteIsochPipeAsap`) in `win-sys`
(`win-sys/src/usb_isoch.rs`), on the interface-1 WinUSB handle obtained from a
vendored, minimally patched nusb (`app/src/crates/nusb`, wired by
`[patch.crates-io]`; the patch only adds an accessor for the WinUSB interface
handle). Completions of these transfers must not reach nusb's thread-pool
I/O callback: use event-based OVERLAPPED with the low bit of `hEvent` set.
The alternate setting follows BTstack's libusb/winusb ports (8-bit: alt
1/2/3, 16-bit: alt 2/4/5 by connection count; 3 ms per transfer). The TCP
backend already carries packet type 3 both ways.

Rust API (`btstack-sys/src/stack.rs`):

```rust
impl Stack {
    pub fn sco_send(&self, handle: u16, payload: &[u8]) -> u8;
    pub fn sco_request_can_send_now(&self, handle: u16);
    pub fn sco_packet_length(&self, handle: u16) -> u16;
    pub fn hfp_ag_audio_connect(&self, acl: u16) -> u8;
    pub fn hfp_ag_audio_release(&self, acl: u16) -> u8;
}
```

`Source::Sco` packets reach `App::on_packet`: `HCI_SCO_DATA_PACKET` with
handle (12 bits) + packet status flag (bits 12-13) + length + payload, and
`HCI_EVENT_SCO_CAN_SEND_NOW`.

## HFP audio (crate `bt-host`, core `stream.rs`)

`HostConfig` gains `voice: bool` (offer codec negotiation, mSBC, LC3-SWB,
eSCO S4 in the HFP AG; off = today's battery-only AG). The core sets it from
`headset_mic_in_force()` of the profile in use.

```rust
pub enum VoiceEvent {
    /// The audio connection is up: codec and the SCO handle.
    Established { codec: u8, sco_handle: u16, rx_len: u16, tx_len: u16 },
    Released,
    Failed { status: u8 },
}
pub enum HostEvent { .., Voice(VoiceEvent) }

impl BtHost {
    /// Opens the HFP audio connection on the current service level
    /// connection. `tx` supplies payload bytes on the BTstack thread,
    /// `rx` receives payloads there.
    pub fn voice_start(&self, tx: Box<dyn VoiceTx>, rx: Box<dyn VoiceRx>) -> Result<(), HostError>;
    pub fn voice_stop(&self) -> Result<(), HostError>;
}
pub trait VoiceTx: Send { fn fill(&mut self, out: &mut [u8]); }
pub trait VoiceRx: Send { fn packet(&mut self, payload: &[u8], status: u8); }
```

The session (`crates/core/src/stream.rs`) gets `take_voice_change()`, polled
in the tick loop like `take_source_change()`. Voice on: stop the capture and
the encode thread, `host.stop_stream()` (AVDTP Suspend, link kept),
`host.voice_start(..)`; on `Established` open `voice_link::Renderer` for the
profile's `mic_device_id` at the codec rate and a capture at the codec rate
with `UplinkSink`. Voice off, link loss or `Released`: tear the voice link
down, then `host.start_stream()` and a fresh pipeline. The status event
carries `VoiceStatus`.

## LE Audio source ASEs (crates `le-audio`, `bt-host/src/le`, `pipeline`, core `le_stream.rs`)

- `read_device` also reads Source PAC, Source Audio Locations and subscribes
  to source ASE notifications; `LeDevice` gets `source_pacs`,
  `source_locations`.
- `plan::StreamRequest` gets `conversational: bool`; the plan then adds a
  source direction: on the first sink CIS of each device (`CisPlan` gets
  `source_ase: Option<SourceDir { ase_id, channels, octets }>`), or a
  source-only CIS when the device has source ASEs on a different set member.
  Presets: a conversational pair uses the sink preset for C→P and `32_2_1`
  (or the best source PAC rate ≤ 32 kHz, falling back to 16_2_1) for P→C;
  GMAP `_gs` presets are added (`32_1_gs`, `32_2_gs`, `48_1_gs`, `48_2_gs`)
  from the GMAP spec table, values checked against the spec text and marked
  where unsure.
- CIG: `max_sdu_p_to_c`, `rtn_p_to_c`, `phy_p_to_c`, `sdu_interval_p_to_c`
  from the source direction. BTstack then sets up the output data path.
- Source ASEs: Config Codec / QoS / Enable like sink ASEs with
  `available_source_contexts`; after Enabling, the client writes Receiver
  Start Ready; on Disable the client writes Receiver Stop Ready when the ASE
  reaches Disabling. `IsoSender` gets only CIS with a C→P direction.
- Receive: `LeCore::on_iso` first checks `packet_type == HCI_ISO_DATA_PACKET`
  (handle from bytes 0-1, then TS if flagged, sequence, SDU length + packet
  status), and hands `(cis_handle, seq, status, sdu)` to an `IsoRx` sink
  (lock-free ring to a "le-decode" thread in `pipeline/src/le_rx.rs`: per-CIS
  `Lc3Decoder`s, PLC on lost/invalid, mono mix of the source channels,
  output to a `voice_link::PcmProducer`). The jitter buffer is sequence
  based with a prefill of two SDU intervals.
- `LeStreamStarted` gets `rx: Option<LeRxHandle>`; `le_stream.rs` opens the
  `Renderer` at the source rate when `headset_mic` is on.
- TMAP: claim CG when a conversational plan is possible; GMAP: UGG with
  source features. `LeDeviceInfo` lists `source_ases` and the source PACs.
- Emulator: `emu_le_audio_sink.py --mic` adds a Source PAC (16/32 kHz, 10 ms),
  source contexts (Conversational), a source ASE, and a sender task that
  writes LC3 SDUs (a 1 kHz tone; frames pre-encoded by `bluecaster_decode
  --lc3-encode` or a tiny Python LC3 encoder is NOT required: send a fixed
  set of frames encoded once by a Rust test binary and checked in as hex).
  `run_test.py --le-audio mic-*` cases check both directions;
  `bluecaster_decode --le-audio` learns ASE direction so the mic stream
  decodes separately.

As implemented (W4, branch `feat/headset-mic-w4`), where it differs from or
adds to the above:

- The request flags live in bt-host's `LeRequest` (`headset_mic: bool`,
  `mic_output: Option<String>`), which core's `StreamRequest::le` carries;
  core's `StreamRequest` (stream.rs) is unchanged. Still to wire: the
  profile's `headset_mic_in_force()` into `LeRequest::headset_mic` where
  the LE request is built (`profiles.rs` / `state.rs`), and
  `StreamStatus::voice` (`VoiceStatus { codec: "lc3", link: "le", rate:
  LeMicPath::rate() }`) in `le_stream.rs`'s `report`.
- `SourceDir { ase_id, channels: u8, allocation: u32, octets: u16 }`; the
  plan has `StreamPlan::source: Option<SourcePlan>` (preset, contexts,
  presentation delay) and `source_note` (why a requested microphone is
  missing: the stream then plays without it). The source preset order is
  `32_2_1, 32_1_1, 24_2_1, 24_1_1, 16_2_1, 16_1_1`, those with the sink
  preset's frame duration first; GMAP (UGTs with the Source feature) puts
  `32_1_gs, 32_2_gs, 48_1_gs, 48_2_gs` first. A conversational plan also
  asks for Conversational on the sink ASEs (next to Media, where
  available); source ASEs ask for Conversational (GMAP: Game +
  Conversational).
- `IsoRx` is bt-host's ring producer (a struct, `le/iso_recv.rs`), created
  per stream in `le_start_stream`; `LeStreamStarted::rx:
  Option<LeRxHandle>` holds the consumer, the streams and the LC3
  parameters.
- The decoded PCM goes to `pipeline::le_rx::PcmOut` (`fn push(&mut self,
  mono_i16: &[i16])`), one frame per call, data driven (no clock of its own:
  the renderer's drift control has to absorb the headset's clock). Outputs
  now: `WavOut` (`--mic-output x.wav`; a stream set up again writes
  `x-2.wav`, ...) and `CountingOut`. A device name (`--mic-output CABLE`)
  is only counted so far: the coordinator adds `impl PcmOut for
  voice_link::PcmProducer` and opens the `Renderer` in
  `core::le_stream::LeMicPath::start` (`MicOutput::Device`).
- TMAS claims Call Gateway + UMS (0x0005) and GMAS UGG Features 0x07
  (Multisink added) always: the TMAP / GMAP roles are static GATT values.
- Emulator: `--mic [SINKS]` (1-based sink numbers, default all) and
  `--mic-rates`; the frames come from the ignored test `mic_frames` in
  `app/src/crates/codec/src/lc3.rs` (no `bluecaster_decode --lc3-encode`).
  `bluecaster_decode` reports a microphone as "=== Microphone N" with
  "Mic ..." labels (one per CIS, mono WAV `<prefix>.mic.<n>.lc3.wav`), so
  run_test's existing stream counters do not see it.

## Emulator for HFP audio (tools/emu)

`emu_sink.py --hfp [cvsd|msbc|lc3swb]` adds an HF role (Bumble `HfProtocol`,
HF SDP record, codec list), accepts the AG's audio connection and sinks SCO,
and sends a tone back. The controller subclass carries SCO data between the
two virtual controllers at 7.5 ms (template: `le_audio/controller.py`), sends
Number Of Completed Packets, reports SCO buffers (60 × 8) and the eSCO LMP
feature bits, and handles the legacy Setup/Accept Synchronous Connection
commands. The snooper writes SCO as PacketLogger 0x08/0x09. `run_test.py
--voice [cvsd|msbc|lc3swb]` runs `bluecaster-core --cli --headset-mic` and
checks: audio connection established with the expected codec, SCO packets in
both directions in both captures, no decoder errors in the sink log.

## Open questions / risks

- WinUSB isochronous on real adapters (full vs high speed, alt setting 6 on
  Intel/Realtek) is unverified; the emulator covers only the TCP transport.
- Opening HFP audio without a call: some headphones may refuse or show a call
  UI. The AG uses `hfp_ag_establish_audio_connection` with no call state.
- Feedback loop: the microphone render device must never be the capture
  source. The core refuses headset mode when `mic_device_id` equals the
  profile's virtual/output capture device, or when System Loopback is in use
  and the mic device is the default output (log + status message).
