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
`Settings::headset_mic_in_force(profile_flag)`, same pattern as
`capture_source()`); headset mode also stays off while `mic_device_id` is
empty. Changing `headset_mic` or the mic device of the profile in use
switches **in place** (no stream restart) for classic devices streaming
(`state.rs` `headset_change` → `HeadsetChange::InPlace`, `TODO(W3)` hook in
`profile.save`); for LE Audio entries it restarts the LE stream. Outside
developer mode, or a mic device changed while headset mode is off, the save
changes nothing in the stream.

Feedback loop check (pure, in `profiles.rs`):
`Profile::mic_device_conflicts(&self, default_output_id: &str) ->
Option<&'static str>` returns `"error.headset_mic_loop"` when the mic device
is the captured virtual/output device, or the default output while the
profile captures System Loopback; None otherwise (also with no device
chosen). The caller (W3) passes the current default render endpoint id.

CLI (`app/src/core/src/cli.rs`): `--headset-mic` (on), `--mic-output <name>`
(render device by friendly-name substring; the emulator runs pass the Windows
default or a cable). No CLI flag for the codec: the negotiation decides. For
LE: `--headset-mic` on an `--le-audio` run requests the conversational
configuration.

As implemented (W3, classic): `StreamRequest` has `headset_mic: bool` and
`mic_output: Option<String>`. `mic_output` is a render endpoint id (the GUI:
the profile's `mic_device_id`), from the CLI also a friendly-name part, or a
path ending in `.wav`, which `voice_link::WavSink` writes (mono 16-bit at the
voice rate) instead of rendering; None / empty: the default output.
`--headset-mic` sets `headset_mic` (the session switches once streaming).
`--headset-mic-toggle <on_s>,<off_s>` (Development / testing) switches
headset mode on and off at those seconds of the stream through
`Service::set_headset_mic` (the stack then offers voice from the start, via
`set_voice_probe`).

Language keys (all 8 files): `ui.headset_mic` ("Headset microphone"),
`ui.headset_mic_hint`, `ui.mic_output` ("Microphone output"),
`ui.mic_output_hint` (apps use the cable's recording side),
`ui.headset_mic_quality` (the call-quality warning, shown only for classic
devices), `ui.headset_mic_no_output` (no output device chosen),
`status.voice_codec` ("Voice: %s"), codec names `codec.cvsd`,
`codec.msbc`, `codec.lc3_swb` ("lc3" shows as plain "LC3"), and
`error.headset_mic_loop` (the feedback-loop refusal, a status message).
Hints stay under 120 characters.

## Voice audio plumbing (new crate `app/src/crates/voice-link`)

`#![forbid(unsafe_code)]`; deps: `wasapi`, `audio-capture` (for `Format`,
`Sink`, `render_devices`), `rtrb`, `tracing`, and `btstack-sys` / `lc3-sys`
(codecs), `win-sys` (COM apartment, MMCSS), `thiserror`. `voice_link` is in
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

As implemented, beyond the sketch above:

- `Renderer::open("")` opens the default output device. `producer()` hands
  out the queue's one producer on the first call; a later call returns a
  detached producer that only counts its pushes as overflow. Also
  `rate()`, `ended()` / `end_error()` (device removed or failed: the thread
  has stopped). `RenderStats { frames_written, silence_frames, underruns,
  overflow, dropped, duplicated, queued }` (frames = mono samples at the
  voice rate; `dropped` / `duplicated` are the drift control's). The queue
  holds max(4 x prefill, 250 ms).
- `pcm_channel(capacity) -> (PcmProducer, PcmConsumer)`: the same queue
  without a device, for tests and tools (`PcmConsumer::pop`, `queued`,
  `stats`).
- `UplinkConsumer` sends zeros until 30 ms are queued (at the start and
  after an underrun) and has the same drift control as the renderer (the
  trough of the queue level over 200 ms against the prefill, ±10 ms; at
  most 0.25 % correction). `queued()`, `stats() -> UplinkStats`, `rate()`.
- `VoiceCodec` is `Copy`, with `hfp_id()` and `name()` ("cvsd", "msbc",
  "lc3_swb", for `VoiceStatus::codec`). `VoiceEncoder::stats()`,
  `VoiceDecoder::stats() -> DecoderStats { packets, bad_packets, frames,
  concealed, samples, overflow }`. Any packet status flag other than 0
  is concealed.
- `voice_link::h2`: `H2Framer::next_header()`, `H2Sync::push(bad, bytes,
  |frame: H2Frame|)` with `H2Frame::Good(&[u8])` (60 bytes) or
  `H2Frame::Bad` (a port of BTstack's `hfp_h2_sync`: bad or unsynchronised
  bytes come out as one `Bad` per 60), `is_header`, `sequence_of`,
  `SEQUENCE`.
- The 10 s debug summaries: "voice link: render ...", "voice link: uplink
  ...", "voice link: downlink ...".

The codec primitives come from `btstack-sys` (`btstack_sys::voice`, C glue in
`csrc/voice_codecs.c`): `MsbcEncoder::encode(&[i16; 120]) -> [u8; 57]` (the
Bluedroid encoder through BTstack's instance API
`btstack_sbc_encoder_bluedroid_init_instance`, `SBC_MODE_mSBC`),
`MsbcDecoder::decode(status, bytes, |pcm: &[i16]|)` (the Bluedroid SBC
decoder through `btstack_sbc_decoder_bluedroid_init_instance`, mSBC mode; it
does H2 sync and `btstack_sbc_plc` itself, concealing only after its first
good frame) and `CvsdPlc::process(bad, input, out)` (`btstack_cvsd_plc`, in
pieces of at most 60 samples). The Bluedroid encoder is now always compiled
(feature `sbc-reference` adds only `csrc/sbc_reference.c`). LC3-SWB uses
`lc3_sys::{Lc3Encoder, Lc3Decoder}` (32 kHz, 7.5 ms, 58 octets) with H2
framing/sync in Rust (`voice_link::h2`).

## SCO transport (crates `hci-transport`, `win-sys`, `btstack-sys`)

`btstack_config.h` gains `ENABLE_SCO_OVER_HCI`, `ENABLE_HFP_WIDE_BAND_SPEECH`,
`ENABLE_HFP_SUPER_WIDE_BAND_SPEECH`.

`HciBackend` (`btstack-sys/src/transport.rs`) gains

```rust
/// BTstack's set_sco_config: voice setting and number of SCO connections.
/// The USB backend starts/stops the isochronous pipes here.
fn set_sco_config(&mut self, voice_setting: u16, connections: u8) {}
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
    /// voice: codec negotiation (LC3-SWB, mSBC, CVSD) and eSCO S4;
    /// false: the battery-only AG (CVSD, no negotiation)
    pub fn hfp_ag_init(&self, voice: bool);
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

`sco_send` copies the payload into HCI's outgoing buffer
(`hci_reserve_packet_buffer` / `hci_send_sco_packet_buffer`); call it only
on `HCI_EVENT_SCO_CAN_SEND_NOW`. The SDP record of the voice AG gets its
wide band speech / LC3-SWB bits although `hfp_ag_create_sdp_record_with_codecs`
tests the HF's codec negotiation bit (7) instead of the AG's (9): shim.c
adds bit 7 to the features it passes to the record builder only (bits 5-13
of the AG features never reach the record).

### USB: state (W1)

Done, compile-tested only. `UsbTransport` is an H2 transport and ports
BTstack's `platform/windows/hci_transport_h2_winusb.c`:

- interface 1 is claimed through nusb at open (nusb vendored as 0.2.8 plus
  `Interface::winusb_handle()`, see `app/src/crates/nusb/SOURCE.txt`);
- `set_sco_config` selects alternate setting 1/2/3 (8-bit voice) or 2/4/5
  (16-bit) for one to three links and starts a `usb-sco` thread, which keeps
  8 isochronous reads of 3 ms in flight (`WinUsb_ReadIsochPipeAsap`,
  continuing the stream) and reassembles the IN byte stream into SCO
  packets; each outgoing SCO packet is one `WinUsb_WriteIsochPipeAsap`, up to
  20 queued or in flight (60 ms); `HCI_EVENT_SCO_CAN_SEND_NOW` is handed out
  while there is room; with no link left the thread stops and alternate
  setting 0 is selected;
- the WinUSB calls are in `win-sys/src/usb_isoch.rs`, their OVERLAPPEDs set
  the low bit of `hEvent`, so nusb's completion port never sees them;
- every USB adapter is H2 (`is_h2` is read at install, before open): without
  a claimable interface 1, SCO is refused (`can_send`) and dropped (`send`,
  still reported as sent so BTstack's buffer is freed);
- high speed adapters: one packet per 2^(bInterval-1) microframes, rounded
  to whole 1 ms frames (bInterval 1: 24 packets per 3 ms read); full speed
  (most adapters): 3 packets per read; unknown speed counts as full speed.

Unit tests: alternate setting choice, packet sizes, SCO reassembly (also
across failed isochronous packets), send token accounting, packet counts
for both speeds.

To verify on real hardware:

1. Interface 1 can be claimed (debug line "interface 1 claimed for SCO");
   adapters whose WinUSB sits on a usbccgp child are unknown.
2. SetCurrentAlternateSetting(1/2) and RegisterIsochBuffer after it succeed
   ("USB: SCO on interface 1 alt N" with the pipe sizes); WinUSB's
   MaximumBytesPerInterval is 9 / 17.
3. IN: about 333 SCO packets/s per link (the 10 s "USB: SCO last ..." debug
   line) with the right handle and length (24 bytes transparent, 48 CVSD
   16-bit), the microphone decodes cleanly; failed isochronous packets and
   gaps (stream continuation falling back to ASAP).
4. OUT: a 3+24 / 3+48 byte write takes 3 frames ("USB: SCO out:"), writes
   complete without ERROR_INVALID_PARAMETER, the headset hears the PC without
   underruns or overruns, tokens come about every 3 ms.
5. Link up / down several times, two links (alt 2 to 4), close during SCO:
   no "did not return" errors, alt 0 selected, the next open works (no
   error 5).
6. High speed adapters: the read packet counts and especially writes
   (WinUSB may reject writes that are not whole frames).
7. Unplug during SCO ends the thread cleanly.
8. BTstack's H2 assert that the controller's SCO buffer length is >= 48.
9. Latency of the 20-slot OUT ring (60 ms); lower `SCO_OUT_SLOTS` if too
   high.
10. Whether controllers align SCO IN to 3-packet boundaries (how fast the
    reassembly recovers after a lost packet).

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
/// bt_host::voice::HostError
pub enum HostError { NotEnabled, NotConnected, Busy, Refused(u8), NoAnswer }
```

State (W1): implemented in `bt-host/src/voice.rs`. `voice_start` fails with
`NotConnected` until the service level connection is up (callers retry),
`NotEnabled` without `HostConfig::voice`. `VoiceTx::fill` gets a buffer of
`sco_packet_length - 3` bytes on every SCO can-send-now (TCP / emulator: 60
bytes, CVSD 16-bit and transparent alike; USB H2: 48 / 24), `VoiceRx::packet`
every received payload. `Established` is also reported when the headphones
open the audio connection themselves; nothing is sent then and
`voice_start` returns `Busy` while it is open (W3 decides what to do).
`Released` comes after `voice_stop`, a release by the headphones, or the
service level connection going down. The core sets `HostConfig::voice` only
for the developer CLI's `--voice-probe` (`Service::set_voice_probe`); W3
replaces that with `headset_mic_in_force()`.

### Headset mode in the session: state (W3)

Done for classic devices, verified on the emulator only
(`crates/core/src/voice.rs`, `HeadsetMode`, driven from `stream.rs`):

- `state.rs` `start_profile` sets `req.headset_mic` (in force and an output
  chosen) and `req.mic_output = Some(mic_device_id)`; the `profile.save`
  InPlace arm calls `Service::set_headset_mic(on, mic_output) -> bool`. False
  (no classic stream streaming, or a stack started without voice when
  turning on): the save restarts the profile as for any other change.
- `host_config`: `c.voice = voice_wanted || voice_probe`, where
  `voice_wanted` is the starting request's `headset_mic` (classic) or set by
  `set_headset_mic(true, ..)`. `ensure_host` restarts a running stack
  without voice when voice is wanted.
- Events: the host-events thread queues `HostEvent::Voice` for the session
  (`Inner::take_voice_events`) and wakes its tick; `take_voice_change()` is
  polled like `take_source_change()`. The session emits
  `ServiceEvent::Voice(Option<ipc::VoiceStatus>)`; `on_service_event` sets
  `StreamStatus.voice` from it (`link` "hfp", `mic_device_id` empty for a
  WAV file).
- On (each tick of a link that is fine, until it works or is given up):
  resolve the output; refuse with `error.headset_mic_loop` (a Streaming
  status message, the stream goes on) when
  `profiles::mic_output_conflicts(current source, output id, default render
  id)` says so (`Profile::mic_device_conflicts` now calls this free
  function; the session checks the source it really captures, a System
  Loopback stand-in included); `host.voice_check()` (new, see below):
  NotConnected is retried for up to 20 s (the service level connection comes
  after A2DP), Busy waits for the headphones' own Established; then stop the
  capture and the encode thread (media sink returned), `host.stop_stream()`,
  `host.voice_start(tx, rx)`.
- tx / rx are shells around `Arc<Mutex<Option<..>>>` slots: silence /
  dropped packets until armed. On `Established{codec}` the session thread
  opens the output (`Renderer` at the codec rate, prefill 40 ms, or the
  `WavSink`), a capture of the current source at `preferred_rate = codec
  rate` into an `UplinkSink` (200 ms), and installs encoder + uplink and
  decoder + producer under the lock. The BTstack thread only `try_lock`s
  (contended only while arming / disarming: one packet of silence), and the
  armed path allocates nothing.
- Off, `Released`, `Failed`, no Established within 10 s, the render device
  ending, an output that does not open: `voice_stop` (when asked for),
  disarm (capture, output, codecs dropped on the session thread), then
  `host.start_stream()` + a fresh encoder and pipeline from the media sink
  + the A2DP capture again (`A2dpSide::resume`, the steps a reconnect
  takes). A resume that fails ends the session with its error. Failures give
  `status.headset_mic_failed` / `status.headset_mic_output_failed` (new keys,
  all 8 languages) and hold headset mode off until the next change.
- Released by the headphones while on: back to A2DP and the request is
  cleared (as if switched off); saving the setting again or a new stream
  opens it again. Chosen over keeping it requested so a headset that hangs
  up is not reopened in a loop.
- The headphones open the audio connection themselves: with headset mode
  on (and no conflict) it is taken over (`host.voice_attach`, new) and
  armed; otherwise `voice_stop` closes it.
- Link loss while on: the voice link is dropped, the A2DP capture started
  again and the normal reconnect builds the pipeline; headset mode stays
  requested, so the reconnected stream opens the voice link again (stale
  voice events are dropped after the reconnect). Session end: `voice_stop`,
  no resume.
- While on, a source change restarts only the uplink capture, a bitpool
  change only updates the encoder config A2DP resumes with, and the A2DP
  capture's reopen / app-attach logic is paused.
- Logs: "voice: ..." (switching, codec, "A2DP suspended / resumed", a 10 s
  summary, "voice: <codec> link down after N s: sent .., uplink underruns /
  dropped / repeated, received .."), plus voice-link's own summaries.

bt-host additions (beyond W1's API, additive): `BtHost::voice_enabled()`,
`voice_check() -> Result<(), HostError>` (what `voice_start` would answer,
opening nothing), `voice_attach(tx, rx)` (an audio connection the headphones
opened gets tx / rx; NotConnected / Busy otherwise).

voice-link changes (W2's drift control, `drift.rs`): a trough more than twice
the tolerance below the target primes again once (`Adjust::Reprime`)
instead of seconds of 0.25 % steps, and the first window after priming moves
the level to the target in one step (at most a tolerance's worth, carried
over pulls). On the emulator the CVSD uplink otherwise repeated 4 samples
every 200 ms for several seconds after the start (priming leaves the trough
at the tolerance's edge with 10 ms capture blocks), audible as clicks.
Remaining: under machine load a few such steps still happen; priming to
prefill + tolerance / 2 would center the level but changes W2's tests.

`--voice-probe` (`app/src/core/src/voice_probe.rs`, developer CLI, USAGE
"Development / testing"): once streaming, retries `voice_start` until the
service level connection is up, sends a 1 kHz 16-bit tone (CVSD only makes
sense), prints "voice probe: N packets, M bytes, status counts ..." every
second. A2DP keeps streaming meanwhile (the session takes no part). Kept as
a probe of bt-host's API; the emulator tests use `--headset-mic` now.

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

State (W1): `tools/emu/hfp_audio.py` (`HfpEmuController`, `HfSide`).

- BTstack's non-H2 SCO path on TCP: the emulated controller lists Write
  Synchronous Flow Control Enable, BTstack turns it on and then counts its 8
  SCO buffers by Number Of Completed Packets, which the controller sends as
  it carries each packet in its 7.5 ms interval (CVSD 16-bit: 120 HCI bytes
  per interval = two 60-byte packets, transparent: one). Bumble's host (the
  HF) does not enable it; it sends paced itself, a full queue drops the
  oldest. Packets per second each way: 267 (CVSD), 133 (mSBC, LC3-SWB).
- eSCO only on the bus (`LmpEscoLinkReq` for both setup commands; the link
  type follows the packet types); Synchronous Connection Complete reports
  interval 12 slots, window 2, 60-byte packets, air mode CVSD / transparent
  from the AG's voice setting. BTstack picks S4 (CVSD) and T2 (mSBC,
  LC3-SWB).
- The `cvsd` HF has no codec negotiation (CVSD without `+BCS`), `msbc` offers
  CVSD + mSBC, `lc3swb` CVSD + mSBC + LC3-SWB. It accepts with Bumble's
  `ESCO_PARAMETERS` S4 / T2. It sends a 1.5 kHz 16-bit tone (cvsd) or an H2
  header + zero frame (others, not decodable), checks the 1 kHz tone of
  `--voice-probe` with a Goertzel ratio (cvsd) and prints
  "HFP rx: N packets ... tone ok 1000 Hz ratio R | tx: M packets" every second.
- `run_test.py --voice` (base port + 400) runs `--voice-probe`, not
  `--headset-mic` (W3 switches it when the session drives the voice link),
  and checks the codec in bluecaster.log ("HFP: audio connection
  established, codec X") and sink.log ("codec negotiated"), SCO out / in
  in both captures (at least a second's worth each), the probe received
  packets, the sink received packets, and for cvsd the tone verdict. No
  decoder runs on the voice payloads yet (W2's `voice_link` would be the
  place; the emulator's mSBC / LC3-SWB frames are dummies).

Verified on the emulator (2026-11-08): `--voice cvsd msbc lc3swb` pass (cvsd:
tone ratio 1.000, ~1540 SCO packets each way in 6 s; msbc / lc3swb: ~770),
`--codecs sbc` and `--le-audio stereo two-ase` still pass.

State (W3): `run_test.py --voice` runs `--headset-mic --mic-output
<out>/mic.wav` (no probe) and checks, beyond the above: "Voice: <codec> at
<rate> Hz" in BlueCaster's output; the sink's verdict on what BlueCaster
sent (cvsd: the test tone mixed to mono, 1 kHz alone or with 1.5 kHz, in
90 % of the 20 ms blocks of the last 3 s; msbc / lc3swb: each 60-byte packet
an H2 frame in sequence, at least 100 and under 2 % not); and BlueCaster's
mic.wav holds the HF's 1.5 kHz tone (amplitude 0.244 within 0.05, > 95 % of
the energy). The HF sends real frames for msbc / lc3swb:
`tools/emu/voice_frames.py`, four frames of a 1.5 kHz tone encoded by
BlueCaster's own mSBC / LC3-SWB encoders (generated by `cargo run -p
voice-link --example emu_voice_frames`; `voice-link/tests/emu_frames.rs`
checks they decode to the tone, and that the encoder's output for the test
tone mix decodes to both tones with H2 headers in sequence, which the HF
cannot decode itself). New case `cvsd-roundtrip`: `--headset-mic-toggle 3,6`
(10 s), also checks "voice: A2DP resumed", A2DP media packets at the sink for
at least 2 s after its last SCO packet, and the test tone in the last second
the sink decoded.

Verified on the emulator (2026-11-08, W3): `--voice` cvsd, msbc, lc3swb and
cvsd-roundtrip pass (mic.wav 1500 Hz amplitude 0.244 for cvsd / msbc, 0.260
lc3swb; ~1565 SCO packets each way for cvsd in 6 s, ~782 msbc / lc3swb;
round trip: ~390 A2DP media packets in 3.9 s after the voice link, both test
tones at 0.500 in the last second); `--codecs sbc aac` and `--le-audio
stereo` still pass. Not verified: real headphones (codec negotiation, eSCO,
a call UI without a call), the WASAPI render to a real cable, USB SCO.

## Open questions / risks

- WinUSB isochronous on real adapters (full vs high speed, alt setting 6 on
  Intel/Realtek) is unverified; the emulator covers only the TCP transport
  (see "USB: state (W1)" for the list).
- The HFP AG with voice on real headphones is untested: codec negotiation,
  eSCO S4 / T2 acceptance, mSBC / LC3-SWB fallback to CVSD
  (`sco_for_msbc_failed`).
- Opening HFP audio without a call: some headphones may refuse or show a call
  UI. The AG uses `hfp_ag_establish_audio_connection` with no call state.
- Headphones that close the audio connection right after it opens (no call)
  end headset mode for that request (W3 clears it on Released); they would
  need a call state from the AG.
- Feedback loop: the microphone render device must never be the capture
  source. The core refuses headset mode when `mic_device_id` equals the
  profile's virtual/output capture device, or when System Loopback is in use
  and the mic device is the default output (log + status message).
