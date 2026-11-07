//! Headset mode of a classic stream (docs/headset-mic-dev.md): the A2DP
//! stream is suspended (the link kept), the HFP audio connection carries the
//! PC's audio to the headphones and their microphone back, which is
//! rendered to the output the user chose (a virtual cable) or written to a
//! WAV file (developer CLI). Off again: the audio connection is closed and
//! A2DP resumes with a fresh pipeline.
//!
//! bt-host needs the [`VoiceTx`] / [`VoiceRx`] when the audio connection is
//! asked for, before the codec is negotiated: they are codec-agnostic shells
//! here, armed with the encoder, decoder, microphone output and uplink once
//! [`VoiceEvent::Established`] names the codec. The session thread locks
//! their slot only to arm and disarm them; the BTstack thread only tries the
//! lock (silence or a dropped packet when it is taken), so it never waits.
//!
//! The session ([`crate::stream`]) runs [`HeadsetMode::tick`] on every tick
//! of a link that is fine, [`HeadsetMode::link_lost`] when the link drops and
//! [`HeadsetMode::end`] when the session ends.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use audio_capture::{Capture, CaptureOptions, Running, Source};
use bt_host::{BtHost, HostError, VoiceEvent, VoiceRx, VoiceTx};
use voice_link::{PcmProducer, Renderer, UplinkConsumer, UplinkSink, VoiceCodec, VoiceDecoder, VoiceEncoder, WavSink};

use crate::messages::Msg;
use crate::service::{Inner, VoiceChange};
use crate::stream::{SourceRequest, StreamState};

/// The microphone render's prefill
const RENDER_PREFILL_MS: u32 = 40;
/// The uplink queue (capture to encoder)
const UPLINK_CAPACITY_MS: u32 = 200;
/// How long headset mode waits for the HFP service level connection, which
/// comes up some time after A2DP
const SLC_WAIT: Duration = Duration::from_secs(20);
/// How long an audio connection asked for may take to come up
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
/// The session's own summary while the voice link is up
const SUMMARY_EVERY: Duration = Duration::from_secs(10);

/// What the session does to its A2DP stream around the voice link
pub(crate) trait A2dp {
    /// Stops the capture and the encode thread and suspends the stream (the
    /// link stays up)
    fn suspend(&mut self);
    /// Starts the stream again, with a fresh pipeline and the capture. The
    /// error ends the session.
    fn resume(&mut self) -> Result<(), Msg>;
}

/// Where the microphone goes
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MicTarget {
    Device { id: String, name: String },
    Wav(PathBuf),
}

impl std::fmt::Display for MicTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MicTarget::Device { id, name } => write!(f, "{} ({id})", crate::diag::quoted(name)),
            MicTarget::Wav(path) => write!(f, "file {}", path.display()),
        }
    }
}

/// The microphone output of a request ([`crate::StreamRequest::mic_output`]):
/// a path ending in ".wav"; a render endpoint by id, or (command line) by a
/// part of its friendly name; none or empty: the default output. None when
/// there is no output device at all.
pub(crate) fn resolve_mic_output(spec: Option<&str>) -> Option<MicTarget> {
    let spec = spec.unwrap_or("").trim();
    if spec.to_ascii_lowercase().ends_with(".wav") {
        return Some(MicTarget::Wav(PathBuf::from(spec)));
    }
    let devices = audio_capture::render_devices().unwrap_or_else(|e| {
        tracing::warn!("voice: cannot list the output devices: {e}");
        Vec::new()
    });
    let wanted = spec.to_lowercase();
    let found = if spec.is_empty() {
        devices.iter().find(|d| d.is_default)
    } else {
        devices
            .iter()
            .find(|d| d.id == spec)
            .or_else(|| devices.iter().find(|d| d.name.to_lowercase().contains(&wanted)))
    };
    match found {
        Some(d) => Some(MicTarget::Device { id: d.id.clone(), name: d.name.clone() }),
        // Gone (or not listed): opening it says so
        None if !spec.is_empty() => Some(MicTarget::Device { id: spec.to_string(), name: spec.to_string() }),
        None => None,
    }
}

/// Why headset mode must not render to `target` while `source` is captured
/// (the message key): the microphone would come back into the stream
pub(crate) fn conflict(target: &MicTarget, source: &SourceRequest) -> Option<&'static str> {
    match target {
        MicTarget::Device { id, .. } => {
            let default = audio_capture::default_render_device().unwrap_or_default();
            crate::profiles::mic_output_conflicts(source, id, &default)
        }
        MicTarget::Wav(_) => None,
    }
}

/// The capture of the uplink: the stream's source at the voice rate (Windows
/// converts); None for an application that does not run (silence goes up)
fn uplink_source(source: &SourceRequest) -> Option<(Source, bool)> {
    Some(match source {
        SourceRequest::TestTone => (Source::TestTone, false),
        SourceRequest::SystemLoopback => (Source::SystemLoopback, true),
        SourceRequest::VirtualDevice { id, .. } | SourceRequest::OutputDevice { id, .. } if !id.is_empty() => {
            (Source::RenderDevice { id: id.clone() }, true)
        }
        SourceRequest::VirtualDevice { .. } | SourceRequest::OutputDevice { .. } => (Source::SystemLoopback, true),
        SourceRequest::Application { exe } => {
            (Source::Application { process_id: audio_capture::find_process(exe)? }, true)
        }
    })
}

struct TxParts {
    encoder: VoiceEncoder,
    uplink: UplinkConsumer,
}

struct RxParts {
    decoder: VoiceDecoder,
    out: PcmProducer,
}

type Slot<T> = Arc<Mutex<Option<T>>>;

/// The [`VoiceTx`] bt-host holds: silence until armed
struct ShellTx(Slot<TxParts>);

impl VoiceTx for ShellTx {
    fn fill(&mut self, out: &mut [u8]) {
        match self.0.try_lock() {
            Ok(mut parts) => match parts.as_mut() {
                Some(p) => p.encoder.fill(&mut p.uplink, out),
                None => out.fill(0),
            },
            // Being armed or disarmed right now
            Err(_) => out.fill(0),
        }
    }
}

/// The [`VoiceRx`] bt-host holds: drops packets until armed
struct ShellRx(Slot<RxParts>);

impl VoiceRx for ShellRx {
    fn packet(&mut self, payload: &[u8], status: u8) {
        if let Ok(mut parts) = self.0.try_lock()
            && let Some(p) = parts.as_mut()
        {
            p.decoder.decode(payload, status, &mut p.out);
        }
    }
}

enum MicOutput {
    Device(Renderer),
    Wav(WavSink),
}

impl MicOutput {
    fn open(target: &MicTarget, rate: u32) -> Result<MicOutput, String> {
        match target {
            MicTarget::Device { id, .. } => {
                Renderer::open(id, rate, RENDER_PREFILL_MS).map(MicOutput::Device).map_err(|e| e.to_string())
            }
            MicTarget::Wav(path) => WavSink::create(path, rate).map(MicOutput::Wav).map_err(|e| e.to_string()),
        }
    }

    fn producer(&mut self) -> Option<PcmProducer> {
        match self {
            MicOutput::Device(r) => Some(r.producer()),
            MicOutput::Wav(w) => w.producer(),
        }
    }

    fn ended(&self) -> Option<String> {
        match self {
            MicOutput::Device(r) if r.ended() => Some(r.end_error().unwrap_or("ended").to_string()),
            _ => None,
        }
    }
}

/// One voice link, from asking for the audio connection to closing it
struct VoiceLink {
    tx: Slot<TxParts>,
    rx: Slot<RxParts>,
    target: MicTarget,
    asked: Instant,
    /// Since Established
    codec: Option<(VoiceCodec, Instant)>,
    output: Option<MicOutput>,
    capture: Option<Running>,
    summary_at: Instant,
}

impl VoiceLink {
    fn new(target: MicTarget) -> (VoiceLink, Box<dyn VoiceTx>, Box<dyn VoiceRx>) {
        let (tx, rx): (Slot<TxParts>, Slot<RxParts>) = Default::default();
        let now = Instant::now();
        let link = VoiceLink {
            tx: tx.clone(),
            rx: rx.clone(),
            target,
            asked: now,
            codec: None,
            output: None,
            capture: None,
            summary_at: now,
        };
        (link, Box::new(ShellTx(tx)), Box::new(ShellRx(rx)))
    }

    /// The audio connection is up with `codec`: the microphone output at its
    /// rate, the uplink capture of `source`, and the codec halves armed. Err:
    /// the output cannot be opened (the message for the user).
    fn arm(&mut self, codec: VoiceCodec, source: &SourceRequest) -> Result<(), Msg> {
        let rate = codec.rate();
        let mut output = MicOutput::open(&self.target, rate).map_err(|e| {
            tracing::warn!("voice: cannot open the microphone output {}: {e}", self.target);
            Msg::new("status.headset_mic_output_failed")
        })?;
        let out = output.producer().ok_or_else(|| Msg::new("status.headset_mic_output_failed"))?;
        let uplink = self.start_uplink(rate, source);
        *self.rx.lock().unwrap_or_else(|e| e.into_inner()) = Some(RxParts { decoder: VoiceDecoder::new(codec), out });
        *self.tx.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(TxParts { encoder: VoiceEncoder::new(codec), uplink });
        self.output = Some(output);
        let now = Instant::now();
        self.codec = Some((codec, now));
        self.summary_at = now + SUMMARY_EVERY;
        tracing::info!(
            "voice: {} at {rate} Hz, {} ms after asking; microphone to {}, uplink from {source}",
            codec.name(),
            now.duration_since(self.asked).as_millis(),
            self.target
        );
        Ok(())
    }

    /// A capture of `source` into a new uplink (its consumer returned)
    fn start_uplink(&mut self, rate: u32, source: &SourceRequest) -> UplinkConsumer {
        if let Some(r) = self.capture.take() {
            r.stop();
        }
        let (sink, consumer) = UplinkSink::new(rate, UPLINK_CAPACITY_MS);
        match uplink_source(source) {
            Some((src, fill_silence)) => {
                let options = CaptureOptions { preferred_rate: Some(rate), fill_silence };
                match Capture::open(&src, &options).and_then(|c| {
                    let f = c.format();
                    c.start(Box::new(sink)).map(|r| (r, f))
                }) {
                    Ok((r, f)) => {
                        tracing::debug!(
                            "voice: uplink capture {} Hz, {} channel(s), {:?}",
                            f.sample_rate,
                            f.channels,
                            f.sample_format
                        );
                        self.capture = Some(r);
                    }
                    Err(e) => tracing::warn!("voice: cannot capture {source} for the headphones, sending silence: {e}"),
                }
            }
            None => tracing::info!("voice: {source} is not running, sending silence to the headphones"),
        }
        consumer
    }

    /// Another source chosen: only the uplink's capture changes
    fn change_source(&mut self, source: &SourceRequest) {
        let Some((codec, _)) = self.codec else { return };
        let uplink = self.start_uplink(codec.rate(), source);
        if let Some(parts) = self.tx.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            parts.uplink = uplink;
        }
    }

    /// Another microphone output chosen: only the output changes
    fn change_output(&mut self, target: MicTarget) -> Result<(), Msg> {
        self.target = target;
        let Some((codec, _)) = self.codec else { return Ok(()) };
        let mut output = MicOutput::open(&self.target, codec.rate()).map_err(|e| {
            tracing::warn!("voice: cannot open the microphone output {}: {e}", self.target);
            Msg::new("status.headset_mic_output_failed")
        })?;
        let out = output.producer().ok_or_else(|| Msg::new("status.headset_mic_output_failed"))?;
        if let Some(parts) = self.rx.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            parts.out = out;
        }
        // The old one stops after its producer is gone
        self.output = Some(output);
        tracing::info!("voice: microphone now to {}", self.target);
        Ok(())
    }

    fn status(&self) -> Option<ipc::VoiceStatus> {
        let (codec, _) = self.codec?;
        Some(ipc::VoiceStatus {
            codec: codec.name().into(),
            link: "hfp".into(),
            rate: codec.rate(),
            mic_device_id: match &self.target {
                MicTarget::Device { id, .. } => id.clone(),
                MicTarget::Wav(_) => String::new(),
            },
        })
    }

    fn summary(&mut self) {
        let Some((codec, since)) = self.codec else { return };
        let now = Instant::now();
        if now < self.summary_at {
            return;
        }
        self.summary_at = now + SUMMARY_EVERY;
        let up =
            self.tx.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|p| (p.encoder.stats(), p.uplink.stats()));
        let down = self.rx.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|p| p.decoder.stats());
        let render = match &self.output {
            Some(MicOutput::Device(r)) => {
                let s = r.stats();
                format!("render {} frames, {} underruns, {} overflow", s.frames_written, s.underruns, s.overflow)
            }
            Some(MicOutput::Wav(_)) => "to a file".into(),
            None => "no output".into(),
        };
        tracing::debug!(
            "voice: {} up {} s; uplink {:?}; downlink {:?}; {render}; capture {}",
            codec.name(),
            now.duration_since(since).as_secs(),
            up.map(|(e, u)| (e.bytes, u.underruns)),
            down.map(|d| (d.packets, d.bad_packets, d.concealed)),
            self.capture.as_ref().map_or(0, Running::frames)
        );
    }

    /// Everything of the link dropped: the shells bt-host may still hold
    /// send silence and drop packets
    fn disarm(&mut self) {
        let tx = self.tx.lock().unwrap_or_else(|e| e.into_inner()).take();
        let rx = self.rx.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(r) = self.capture.take() {
            r.stop();
        }
        if let Some((codec, since)) = self.codec.take() {
            let (e, d) = (tx.as_ref().map(|p| p.encoder.stats()), rx.as_ref().map(|p| p.decoder.stats()));
            let u = tx.as_ref().map(|p| p.uplink.stats()).unwrap_or_default();
            tracing::info!(
                "voice: {} link down after {} s: sent {} bytes (uplink {} underruns, {} dropped / {} repeated), \
                 received {} packets ({} bad, {} concealed)",
                codec.name(),
                since.elapsed().as_secs(),
                e.map_or(0, |e| e.bytes),
                u.underruns,
                u.dropped,
                u.duplicated,
                d.map_or(0, |d| d.packets),
                d.map_or(0, |d| d.bad_packets),
                d.map_or(0, |d| d.concealed)
            );
        }
        drop((tx, rx));
        self.output = None;
    }
}

impl Drop for VoiceLink {
    fn drop(&mut self) {
        self.disarm();
    }
}

/// Headset mode of one session
pub(crate) struct HeadsetMode {
    /// Asked for (the request, then [`crate::Service::set_headset_mic`])
    wanted: bool,
    mic_output: Option<String>,
    /// Given up for this request (refused, failed): tried again on the next
    /// change only
    held_off: bool,
    /// Until when a missing service level connection is waited for
    slc_deadline: Option<Instant>,
    link: Option<VoiceLink>,
}

impl HeadsetMode {
    pub fn new(wanted: bool, mic_output: Option<String>) -> HeadsetMode {
        HeadsetMode { wanted, mic_output, held_off: false, slc_deadline: None, link: None }
    }

    /// The voice link carries the stream (A2DP is suspended)
    pub fn active(&self) -> bool {
        self.link.is_some()
    }

    /// Another source chosen while the voice link is up
    pub fn change_source(&mut self, source: &SourceRequest) {
        if let Some(link) = self.link.as_mut() {
            link.change_source(source);
        }
    }

    /// One tick of a stream whose link is fine. The error ends the session
    /// (A2DP could not resume).
    pub fn tick(
        &mut self,
        inner: &Inner,
        host: &BtHost,
        source: &SourceRequest,
        a2dp: &mut dyn A2dp,
    ) -> Result<(), Msg> {
        if let Some(change) = inner.take_voice_change() {
            self.change(change, inner, host, a2dp)?;
        }
        for event in inner.take_voice_events() {
            self.on_event(event, inner, host, source, a2dp)?;
        }
        if let Some(link) = self.link.as_mut() {
            link.summary();
            if let Some(why) = link.output.as_ref().and_then(MicOutput::ended) {
                tracing::warn!("voice: the microphone output ended ({why}), back to A2DP");
                self.held_off = true;
                return self.leave(true, inner, host, a2dp, Some(Msg::new("status.headset_mic_output_failed")));
            }
            if link.codec.is_none() && link.asked.elapsed() > OPEN_TIMEOUT {
                tracing::warn!("voice: no audio connection {} s after asking, back to A2DP", OPEN_TIMEOUT.as_secs());
                self.held_off = true;
                return self.leave(true, inner, host, a2dp, Some(Msg::new("status.headset_mic_failed")));
            }
        }
        match (self.wanted, self.link.is_some()) {
            (true, false) if !self.held_off => self.enter(inner, host, source, a2dp),
            (false, true) => {
                tracing::info!("voice: headset mode off, back to A2DP");
                self.leave(true, inner, host, a2dp, None)
            }
            _ => Ok(()),
        }
    }

    fn change(&mut self, c: VoiceChange, inner: &Inner, host: &BtHost, a2dp: &mut dyn A2dp) -> Result<(), Msg> {
        tracing::info!(
            "voice: headset mode {} (microphone output {:?})",
            if c.on { "on" } else { "off" },
            c.mic_output.as_deref().unwrap_or("")
        );
        let output_changed = c.mic_output != self.mic_output;
        if c.on != self.wanted || output_changed {
            self.held_off = false;
            self.slc_deadline = None;
        }
        self.wanted = c.on;
        self.mic_output = c.mic_output;
        if !(c.on && output_changed) {
            return Ok(());
        }
        let Some(link) = self.link.as_mut() else { return Ok(()) };
        let changed = match resolve_mic_output(self.mic_output.as_deref()) {
            Some(target) => link.change_output(target),
            None => Err(Msg::new("status.headset_mic_output_failed")),
        };
        match changed {
            Ok(()) => {
                inner.voice_status(link.status());
                Ok(())
            }
            Err(m) => {
                self.held_off = true;
                self.leave(true, inner, host, a2dp, Some(m))
            }
        }
    }

    fn on_event(
        &mut self,
        event: VoiceEvent,
        inner: &Inner,
        host: &BtHost,
        source: &SourceRequest,
        a2dp: &mut dyn A2dp,
    ) -> Result<(), Msg> {
        match event {
            VoiceEvent::Established { codec: id, .. } => {
                let Some(codec) = VoiceCodec::from_hfp_id(id) else {
                    tracing::warn!("voice: audio connection with an unknown codec {id}, closing it");
                    self.held_off = true;
                    if self.link.is_some() {
                        return self.leave(true, inner, host, a2dp, Some(Msg::new("status.headset_mic_failed")));
                    }
                    let _ = host.voice_stop();
                    return Ok(());
                };
                match self.link.as_mut() {
                    Some(link) if link.codec.is_some() => {
                        tracing::debug!("voice: audio connection reported up again ({})", codec.name())
                    }
                    Some(link) => match link.arm(codec, source) {
                        Ok(()) => inner.voice_status(link.status()),
                        Err(m) => {
                            self.held_off = true;
                            return self.leave(true, inner, host, a2dp, Some(m));
                        }
                    },
                    // Opened by the headphones (a button on them)
                    None => return self.adopt(codec, inner, host, source, a2dp),
                }
                Ok(())
            }
            VoiceEvent::Released => {
                if self.link.is_none() {
                    return Ok(());
                }
                // The headphones closed it (or the link went): headset mode
                // ends for this request, as if switched off; a new request
                // (the setting saved again, the stream started again) opens
                // it again
                tracing::info!("voice: the headphones closed the audio connection, back to A2DP");
                self.wanted = false;
                self.leave(false, inner, host, a2dp, None)
            }
            VoiceEvent::Failed { status } => {
                if self.link.as_ref().is_none_or(|l| l.codec.is_some()) {
                    return Ok(());
                }
                tracing::warn!("voice: the audio connection failed (0x{status:02x}), back to A2DP");
                self.held_off = true;
                self.leave(false, inner, host, a2dp, Some(Msg::new("status.headset_mic_failed")))
            }
        }
    }

    /// The headphones opened the audio connection themselves: taken over in
    /// headset mode, closed otherwise
    fn adopt(
        &mut self,
        codec: VoiceCodec,
        inner: &Inner,
        host: &BtHost,
        source: &SourceRequest,
        a2dp: &mut dyn A2dp,
    ) -> Result<(), Msg> {
        let target = (self.wanted && !self.held_off)
            .then(|| resolve_mic_output(self.mic_output.as_deref()))
            .flatten()
            .filter(|t| conflict(t, source).is_none());
        let Some(target) = target else {
            tracing::info!("voice: the headphones opened an audio connection ({}), closing it", codec.name());
            let _ = host.voice_stop();
            return Ok(());
        };
        tracing::info!("voice: the headphones opened the audio connection ({}), taking it over", codec.name());
        a2dp.suspend();
        let (mut link, tx, rx) = VoiceLink::new(target);
        if let Err(e) = host.voice_attach(tx, rx) {
            tracing::warn!("voice: cannot take the audio connection over: {e:?}");
            let _ = host.voice_stop();
            return a2dp.resume();
        }
        match link.arm(codec, source) {
            Ok(()) => {
                inner.voice_status(link.status());
                self.link = Some(link);
                Ok(())
            }
            Err(m) => {
                self.link = Some(link);
                self.held_off = true;
                self.leave(true, inner, host, a2dp, Some(m))
            }
        }
    }

    /// Headset mode asked for and no voice link: A2DP is suspended and the
    /// audio connection asked for, once the service level connection is up
    fn enter(&mut self, inner: &Inner, host: &BtHost, source: &SourceRequest, a2dp: &mut dyn A2dp) -> Result<(), Msg> {
        let Some(target) = resolve_mic_output(self.mic_output.as_deref()) else {
            tracing::warn!("voice: no output device for the microphone");
            return self.give_up(inner, Msg::new("status.headset_mic_output_failed"));
        };
        if let Some(key) = conflict(&target, source) {
            tracing::warn!("voice: the microphone output {target} feeds the stream ({source}), headset mode refused");
            return self.give_up(inner, Msg::new(key));
        }
        match host.voice_check() {
            Ok(()) => {}
            Err(HostError::NotConnected) => {
                let deadline = *self.slc_deadline.get_or_insert_with(|| Instant::now() + SLC_WAIT);
                if Instant::now() >= deadline {
                    tracing::warn!("voice: no HFP service level connection after {} s", SLC_WAIT.as_secs());
                    return self.give_up(inner, Msg::new("status.headset_mic_failed"));
                }
                return Ok(());
            }
            // The headphones are opening one: its Established decides
            Err(HostError::Busy) => return Ok(()),
            Err(e) => {
                tracing::warn!("voice: the audio connection cannot be asked for: {e:?}");
                return self.give_up(inner, Msg::new("status.headset_mic_failed"));
            }
        }
        tracing::info!("voice: headset mode on, suspending A2DP and asking for the audio connection");
        a2dp.suspend();
        let (link, tx, rx) = VoiceLink::new(target);
        match host.voice_start(tx, rx) {
            Ok(()) => {
                self.link = Some(link);
                Ok(())
            }
            Err(e) => {
                tracing::warn!("voice: the audio connection was refused: {e:?}, back to A2DP");
                drop(link);
                a2dp.resume()?;
                self.give_up(inner, Msg::new("status.headset_mic_failed"))
            }
        }
    }

    fn give_up(&mut self, inner: &Inner, msg: Msg) -> Result<(), Msg> {
        self.held_off = true;
        inner.notify(StreamState::Streaming, msg);
        Ok(())
    }

    /// Back to A2DP: the audio connection closed (`stop`: asked to), the
    /// link's parts dropped, then the stream resumed
    fn leave(
        &mut self,
        stop: bool,
        inner: &Inner,
        host: &BtHost,
        a2dp: &mut dyn A2dp,
        msg: Option<Msg>,
    ) -> Result<(), Msg> {
        let Some(mut link) = self.link.take() else { return Ok(()) };
        if stop && let Err(e) = host.voice_stop() {
            tracing::warn!("voice: closing the audio connection: {e:?}");
        }
        let was_up = link.codec.is_some();
        link.disarm();
        if was_up {
            inner.voice_status(None);
        }
        a2dp.resume()?;
        if let Some(m) = msg {
            inner.notify(StreamState::Streaming, m);
        }
        Ok(())
    }

    /// The link to the headphones went: the voice link with it (A2DP comes
    /// back with the reconnect). True when one was up: the caller starts
    /// its capture again. Headset mode stays asked for: the reconnected
    /// stream opens it again.
    pub fn link_lost(&mut self, inner: &Inner) -> bool {
        let Some(mut link) = self.link.take() else { return false };
        tracing::info!("voice: the link went, the voice link with it");
        let was_up = link.codec.is_some();
        link.disarm();
        if was_up {
            inner.voice_status(None);
        }
        self.slc_deadline = None;
        true
    }

    /// After a reconnect: what the old connection reported is stale
    pub fn reconnected(&mut self, inner: &Inner) {
        inner.take_voice_events();
        self.slc_deadline = None;
    }

    /// The session ends: the audio connection closed, A2DP is not resumed
    pub fn end(&mut self, inner: &Inner, host: &BtHost) {
        let Some(mut link) = self.link.take() else { return };
        tracing::info!("voice: the session ends, closing the audio connection");
        let _ = host.voice_stop();
        let was_up = link.codec.is_some();
        link.disarm();
        if was_up {
            inner.voice_status(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wav_path_is_written_not_rendered() {
        assert_eq!(resolve_mic_output(Some("out\\mic.wav")), Some(MicTarget::Wav("out\\mic.wav".into())));
        assert_eq!(resolve_mic_output(Some("C:/x/MIC.WAV")), Some(MicTarget::Wav("C:/x/MIC.WAV".into())));
        assert_eq!(conflict(&MicTarget::Wav("m.wav".into()), &SourceRequest::SystemLoopback), None);
    }

    #[test]
    fn an_unknown_device_is_kept_for_opening_to_fail() {
        let id = "{0.0.0.00000000}.{not-a-device}";
        assert_eq!(resolve_mic_output(Some(id)), Some(MicTarget::Device { id: id.into(), name: id.into() }));
    }

    #[test]
    fn the_captured_device_is_a_conflict() {
        let cable = MicTarget::Device { id: "{cable}".into(), name: "CABLE Input".into() };
        let virt = SourceRequest::VirtualDevice { id: "{cable}".into(), name: "CABLE".into(), auto_switch: true };
        assert_eq!(conflict(&cable, &virt), Some("error.headset_mic_loop"));
        assert_eq!(conflict(&cable, &SourceRequest::TestTone), None);
    }

    #[test]
    fn unarmed_shells_send_silence_and_drop_packets() {
        let (link, mut tx, mut rx) = VoiceLink::new(MicTarget::Wav("unused.wav".into()));
        let mut out = [0x55u8; 60];
        tx.fill(&mut out);
        assert_eq!(out, [0u8; 60]);
        rx.packet(&[1, 2, 3], 0);
        assert!(link.status().is_none());
    }
}
