//! A streaming session: connect, pick the codec, set up capture, encoder and
//! stream, then keep streaming and reconnect when the link drops — v1's
//! A2dpService::streaming_thread_func_inner, on the session thread of
//! [`crate::Service`].

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use audio_capture::{Capture, CaptureOptions, Format, Running, SampleFormat, Sink, Source};
use bt_host::{BtHost, ConnectFailure};
use codec::caps::{self, Unavailable};
use codec::{CodecKind, EncoderConfig, PeerParams, Quality};
use pipeline::{BufferPreset, CaptureInput, InputFormat, Pipeline, PipelineConfig, SampleType};

use crate::messages::Msg;
use crate::service::Inner;

/// What to stream (from a profile, or the command line)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamRequest {
    pub device: [u8; 6],
    pub device_name: String,
    /// None = Auto
    pub codec: Option<CodecKind>,
    pub codec_fallback: bool,
    pub quality: Quality,
    pub abr: bool,
    /// None = the device's rate
    pub sample_rate: Option<u32>,
    /// None = auto (24-bit where the codec carries it)
    pub bit_depth: Option<u8>,
    pub source: SourceRequest,
    pub volume_sync: VolumeSync,
    pub max_media_payload: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRequest {
    SystemLoopback,
    VirtualDevice { id: String, name: String, auto_switch: bool },
    Application { exe: String },
    TestTone,
}

impl std::fmt::Display for SourceRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceRequest::SystemLoopback => f.write_str("system output"),
            SourceRequest::VirtualDevice { name, .. } => write!(f, "virtual device \"{name}\""),
            SourceRequest::Application { exe } => write!(f, "app {exe}"),
            SourceRequest::TestTone => f.write_str("test tone"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeSync {
    Off,
    /// The device the audio comes from (the default output in application
    /// mode)
    Auto,
    Device(String),
}

/// What the session reports about the stream
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StreamInfo {
    pub codec: String,
    pub bitrate_kbps: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub source_sample_rate: u32,
    pub source_channels: u32,
    pub source_bits: u32,
    /// The profile's codec the device did not offer (codec fallback)
    pub requested_codec: Option<String>,
    /// ... maybe because another device streams on its endpoint
    pub requested_in_use: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum StreamState {
    Idle,
    Connecting,
    Streaming,
    Reconnecting,
    Error,
}

/// Feeds captured audio into the pipeline. Shared so an application
/// capture can be replaced (the app quits and starts again) while the
/// pipeline keeps running; only one capture writes at a time, so the lock is
/// never contended.
#[derive(Clone)]
struct SharedInput(Arc<Mutex<CaptureInput>>);

impl Sink for SharedInput {
    fn audio(&mut self, data: &[u8], _frames: usize, _format: Format) {
        if let Ok(mut input) = self.0.try_lock() {
            input.push(data);
        }
    }

    fn silence(&mut self, frames: usize, _format: Format) {
        if let Ok(mut input) = self.0.try_lock() {
            input.push_silence(frames);
        }
    }
}

fn input_format(f: Format) -> InputFormat {
    InputFormat {
        sample_rate: f.sample_rate,
        channels: f.channels,
        sample: match f.sample_format {
            SampleFormat::F32 => SampleType::F32,
            SampleFormat::I16 => SampleType::I16,
            SampleFormat::I32 => SampleType::I32,
        },
    }
}

/// Application capture that follows the app: attaches by executable name and
/// notices when the process exits, so it can attach again when it is
/// started anew
struct AppSource {
    exe: String,
    sample_rate: u32,
    running: Option<Running>,
    process_id: u32,
    failed_pid: u32,
}

impl AppSource {
    fn attach(&mut self, input: &SharedInput) -> bool {
        let Some(pid) = audio_capture::find_process(&self.exe) else { return false };
        let options = CaptureOptions { preferred_rate: Some(self.sample_rate), fill_silence: true };
        let started = Capture::open(&Source::Application { process_id: pid }, &options)
            .and_then(|c| c.start(Box::new(input.clone())));
        match started {
            Ok(r) => {
                tracing::info!("capturing app {} (PID {pid})", self.exe);
                self.running = Some(r);
                self.process_id = pid;
                true
            }
            Err(e) => {
                if self.failed_pid != pid {
                    tracing::warn!("cannot capture {} (PID {pid}): {e}", self.exe);
                }
                self.failed_pid = pid;
                false
            }
        }
    }

    fn exited(&self) -> bool {
        self.running.is_some()
            && (audio_capture::process_exited(self.process_id) || self.running.as_ref().is_some_and(|r| r.ended()))
    }

    fn detach(&mut self) {
        if let Some(r) = self.running.take() {
            r.stop();
        }
    }
}

enum ActiveSource {
    Capture(Running),
    App(AppSource),
}

impl ActiveSource {
    fn stop(self) {
        match self {
            ActiveSource::Capture(r) => r.stop(),
            ActiveSource::App(mut a) => a.detach(),
        }
    }
}

/// Restores the default output device when the session ends (virtual
/// device mode switched it)
struct DefaultDeviceGuard(Option<String>);

impl Drop for DefaultDeviceGuard {
    fn drop(&mut self) {
        if let Some(id) = self.0.take()
            && let Err(e) = audio_capture::set_default_render_device(&id)
        {
            tracing::warn!("could not restore the default output device: {e}");
        }
    }
}

fn connect_error(f: Option<ConnectFailure>) -> Msg {
    Msg::new(match f {
        Some(ConnectFailure::Stalled) => "error.btstack_no_answer",
        Some(ConnectFailure::NoAnswer) => "error.connect_no_answer",
        Some(ConnectFailure::LinkLost) => "error.connect_link_lost",
        Some(ConnectFailure::Refused) => "error.connect_refused",
        Some(ConnectFailure::AuthFailed) => "error.connect_auth_failed",
        _ => "error.pairing_hint",
    })
}

/// The session thread. Returns when stopped or on an error (state reported).
pub(crate) fn run(inner: &Arc<Inner>, req: StreamRequest) {
    let addr = bt_host::addr_str(&req.device);
    tracing::info!(
        "stream: device {addr} codec {} quality {:?} bit depth {} rate {} source {}",
        req.codec.map_or("auto", |c| c.name()),
        req.quality,
        req.bit_depth.map_or("auto".into(), |b| format!("{b}-bit")),
        req.sample_rate.map_or("device's".into(), |r| format!("{r} Hz")),
        req.source
    );
    let stopped = || inner.stop_requested();

    inner.notify(StreamState::Connecting, Msg::new("status.initializing_btstack"));
    let mut host = match inner.ensure_host() {
        Ok(h) => h,
        Err(m) => return inner.notify(StreamState::Error, m),
    };
    host.set_media_payload_limit(req.max_media_payload);
    if stopped() {
        return inner.notify(StreamState::Idle, Msg::new("status.ready"));
    }

    // Remembered headphone volume, set as soon as AVRCP connects
    let restore = inner.saved_volume(&addr);
    host.set_restore_volume(restore);

    // ---- connect ----
    inner.notify(StreamState::Connecting, Msg::new("status.connecting_device"));
    let connect = |h: &BtHost| {
        h.connect_retrying(req.device, &stopped, &mut |attempt, attempts| {
            inner.notify(StreamState::Connecting, Msg::with("status.connecting_retry", &[&attempt, &attempts]));
        })
    };
    let mut result = connect(&host);
    if result.is_err() && !stopped() && host.last_connect_failure() == Some(ConnectFailure::Stalled) {
        // Not the device: the stack it went through stopped working
        inner.notify(StreamState::Connecting, Msg::new("status.initializing_btstack"));
        host = match inner.restart_host("the Bluetooth adapter stopped working", req.max_media_payload) {
            Ok(h) => h,
            Err(m) => return inner.notify(StreamState::Error, m),
        };
        if stopped() {
            return inner.notify(StreamState::Idle, Msg::new("status.ready"));
        }
        inner.notify(StreamState::Connecting, Msg::new("status.connecting_device"));
        result = connect(&host);
    }
    let remote = match result {
        Ok(caps) => caps,
        Err(_) if stopped() => return inner.notify(StreamState::Idle, Msg::new("status.ready")),
        Err(f) => return inner.notify(StreamState::Error, connect_error(Some(f))),
    };
    tracing::info!("device connected, choosing the codec");
    if stopped() {
        host.disconnect();
        return inner.notify(StreamState::Idle, Msg::new("status.ready"));
    }

    // ---- codec ----
    let choice = match caps::choose(&remote, req.codec, req.codec_fallback) {
        Ok(c) => c,
        Err(why) => {
            host.disconnect();
            let msg = match why {
                None => Msg::new("error.no_compatible_codec"),
                Some((codec, Unavailable::SepInUse { in_use, total })) => {
                    Msg::with("error.codec_sep_in_use", &[&codec.name(), &in_use, &total])
                }
                Some((codec, Unavailable::AdaptiveOnly)) => Msg::with("error.codec_adaptive_only", &[&codec.name()]),
                Some((codec, Unavailable::NotSupported)) => Msg::with("error.codec_not_supported", &[&codec.name()]),
            };
            return inner.notify(StreamState::Error, msg);
        }
    };
    let kind = choice.kind;
    let fallback_text = choice.replaced.as_ref().map(|(wanted, why)| {
        tracing::warn!("codec fallback: {} not available ({why:?}), using {}", wanted.name(), kind.name());
        let key = if matches!(why, Unavailable::SepInUse { .. }) {
            "status.connected_fallback_in_use"
        } else {
            "status.connected_fallback"
        };
        Msg::with(key, &[&wanted.name(), &kind.name()])
    });
    tracing::info!("codec selected: {}", kind.name());

    // ---- audio source ----
    inner.notify(StreamState::Connecting, Msg::new("status.initializing_audio"));
    let test_tone = req.source == SourceRequest::TestTone;
    let mut preferred = match (&req.source, req.sample_rate) {
        (_, Some(r)) => r,
        (SourceRequest::TestTone, None) => 48000,
        (SourceRequest::VirtualDevice { id, .. }, None) => audio_capture::mix_rate(Some(id)).unwrap_or(48000),
        (_, None) => audio_capture::mix_rate(None).unwrap_or(48000),
    };
    if preferred > kind.max_sample_rate() {
        tracing::info!(
            "limiting the capture rate from {preferred} to {} Hz for {}",
            kind.max_sample_rate(),
            kind.name()
        );
        preferred = kind.max_sample_rate();
    }
    // aptX family: 44.1 / 48 kHz as the sink lists them; LHDC V5: its rates.
    // WASAPI resamples to the requested rate.
    let picked = match kind {
        CodecKind::Aptx | CodecKind::AptxHd | CodecKind::AptxLl => remote.pick_aptx_sample_rate(kind, preferred),
        CodecKind::LhdcV5 => remote.pick_lhdcv5_sample_rate(preferred),
        CodecKind::Sbc | CodecKind::Aac if preferred != 44100 => 48000,
        _ => preferred,
    };
    if picked != preferred {
        tracing::info!("{}: capturing at {picked} Hz instead of {preferred} Hz (supported rate)", kind.name());
        preferred = picked;
    }

    let mut default_guard = DefaultDeviceGuard(None);
    let options = CaptureOptions { preferred_rate: Some(preferred), fill_silence: !test_tone };
    let (source, opened): (Option<Source>, Option<Capture>) = match &req.source {
        SourceRequest::TestTone => (Some(Source::TestTone), None),
        SourceRequest::SystemLoopback => (Some(Source::SystemLoopback), None),
        SourceRequest::VirtualDevice { id, name, auto_switch } => {
            if id.is_empty() {
                host.disconnect();
                return inner.notify(StreamState::Error, Msg::new("error.no_virtual_device_selected"));
            }
            if *auto_switch {
                let before = audio_capture::default_render_device();
                match audio_capture::set_default_render_device(id) {
                    Ok(()) => default_guard.0 = before,
                    Err(e) => tracing::warn!("failed to make '{name}' the default output device: {e}"),
                }
            }
            (Some(Source::RenderDevice { id: id.clone() }), None)
        }
        SourceRequest::Application { exe } => {
            let build = audio_capture::windows_build();
            let msg = if build < audio_capture::APP_CAPTURE_MIN_BUILD {
                Some(Msg::with("error.app_capture_unsupported", &[&build]))
            } else if exe.is_empty() {
                Some(Msg::new("error.no_app_selected"))
            } else {
                None
            };
            if let Some(m) = msg {
                host.disconnect();
                return inner.notify(StreamState::Error, m);
            }
            (None, None)
        }
    };
    let _ = opened;
    let capture = match &source {
        Some(s) => match Capture::open(s, &options) {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::error!("audio capture: {e}");
                host.disconnect();
                let key = if matches!(req.source, SourceRequest::VirtualDevice { .. }) {
                    "error.virtual_capture_init"
                } else {
                    "error.wasapi_init"
                };
                return inner.notify(StreamState::Error, Msg::new(key));
            }
        },
        None => None,
    };
    // Application capture: float stereo at the requested rate, known before
    // the app runs
    let format = capture.as_ref().map(|c| c.format()).unwrap_or(Format {
        sample_rate: preferred,
        channels: 2,
        sample_format: SampleFormat::F32,
    });
    let channels = (format.channels as u32).min(2);
    tracing::info!(
        "capture: {} Hz, {} channel(s), {:?} -> {channels} channel(s)",
        format.sample_rate,
        format.channels,
        format.sample_format
    );

    // LDAC, aptX HD and LHDC V5 carry 24-bit unless the profile says 16
    // (LHDC V5 only when the sink has 24-bit)
    let bits: u8 = match (kind, req.bit_depth) {
        (_, Some(16)) => 16,
        (CodecKind::Ldac | CodecKind::AptxHd, _) => 24,
        (CodecKind::LhdcV5, _) if remote.lhdcv5_24bit() => 24,
        _ => 16,
    };

    // ---- stream configuration ----
    let config = match remote.configuration(kind, format.sample_rate, channels, bits) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("{e}");
            host.disconnect();
            return inner.notify(StreamState::Error, Msg::new("error.codec_configure"));
        }
    };
    let mtu = match host.configure(&config) {
        Ok(m) => m,
        Err(_) => {
            host.disconnect();
            return if stopped() {
                inner.notify(StreamState::Idle, Msg::new("status.ready"))
            } else {
                inner.notify(StreamState::Error, Msg::new("error.codec_configure"))
            };
        }
    };
    let encoder_config = EncoderConfig {
        sample_rate: format.sample_rate,
        channels,
        quality: req.quality,
        mtu,
        bits_per_sample: bits,
        abr: req.abr && matches!(kind, CodecKind::Ldac | CodecKind::LhdcV5),
        peer: PeerParams { lhdcv5_bitrate_limits: remote.lhdcv5_bitrate_limits() },
    };
    let encoder = match codec::create(kind, &encoder_config) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("encoder: {e}");
            host.disconnect();
            return inner.notify(StreamState::Error, Msg::new("error.encoder_init"));
        }
    };
    let bitrate = encoder.bitrate_kbps();
    tracing::info!("encoder ready: {} {bitrate} kbps, mtu {mtu}, {bits}-bit, ABR {}", kind.name(), encoder_config.abr);

    if !host.start_stream() {
        host.disconnect();
        return if stopped() {
            inner.notify(StreamState::Idle, Msg::new("status.ready"))
        } else {
            inner.notify(StreamState::Error, Msg::new("error.stream_start"))
        };
    }

    inner.notify(StreamState::Streaming, fallback_text.clone().unwrap_or_else(|| Msg::new("status.connected")));
    inner.stream_info(StreamInfo {
        codec: kind.name().into(),
        bitrate_kbps: bitrate,
        sample_rate: format.sample_rate,
        channels,
        source_sample_rate: format.sample_rate,
        source_channels: format.channels as u32,
        source_bits: (format.sample_format.bytes_per_sample() * 8) as u32,
        requested_codec: choice.replaced.as_ref().map(|(w, _)| w.name().to_string()),
        requested_in_use: matches!(choice.replaced, Some((_, Unavailable::SepInUse { .. }))),
    });
    inner.save_device(&addr, &req.device_name);

    // ---- pipeline and capture ----
    let Some(sink) = host.take_media_sink() else {
        host.disconnect();
        return inner.notify(StreamState::Error, Msg::new("error.encode_thread"));
    };
    let (pipeline, input) = Pipeline::start(
        PipelineConfig {
            encoder,
            input: input_format(format),
            channels,
            mtu,
            abr: encoder_config.abr,
            buffer: BufferPreset::Standard,
        },
        sink,
        host.stats().clone(),
    );
    let input = SharedInput(Arc::new(Mutex::new(input)));
    let active = match (capture, &req.source) {
        (Some(c), _) => match c.start(Box::new(input.clone())) {
            Ok(r) => Some(ActiveSource::Capture(r)),
            Err(e) => {
                tracing::error!("audio capture start: {e}");
                None
            }
        },
        (None, SourceRequest::Application { exe }) => {
            let mut app =
                AppSource { exe: exe.clone(), sample_rate: preferred, running: None, process_id: 0, failed_pid: 0 };
            if !app.attach(&input) {
                // Not running yet is fine: the capture attaches once it is up
                tracing::info!("{exe} is not running yet, waiting for it");
            }
            Some(ActiveSource::App(app))
        }
        _ => None,
    };
    let Some(mut active) = active else {
        if let Some(s) = pipeline.stop() {
            host.return_media_sink(s);
        }
        host.disconnect();
        return inner.notify(StreamState::Error, Msg::new("error.audio_capture_start"));
    };

    // Headphone volume and a Windows volume act as one (volume_sync.rs)
    let mut volume = crate::volume_sync::EndpointChoice::for_stream(&req.volume_sync, &req.source)
        .map(crate::volume_sync::VolumeSyncer::new);

    // ---- streaming ----
    let mut pipeline = Some(pipeline);
    let mut app_ticks = 0u32;
    let mut lost_for_good = false;
    while !stopped() {
        inner.wait_tick(Duration::from_millis(200));
        if stopped() {
            break;
        }
        if let Some(v) = volume.as_mut() {
            v.tick(true, &*host);
        }
        inner.remember_volume(&addr, host.remote_volume());

        // Application capture follows the app: it may quit and start again
        if let ActiveSource::App(app) = &mut active {
            if app.exited() {
                tracing::info!("{} exited, waiting for it to start again", app.exe);
                app.detach();
            }
            app_ticks += 1;
            if app.running.is_none() && app_ticks.is_multiple_of(5) {
                app.attach(&input);
            }
        }

        // Sleep, or an adapter that stopped working, leaves a stack that
        // reports no disconnect: it is started anew and the device connected
        // again on it
        let lost = host.check_disconnected();
        let mut restart_why = if inner.btstack_stale.load(Ordering::Acquire) {
            Some("the PC resumed from sleep")
        } else if host.stalled() {
            Some("the Bluetooth adapter stopped working")
        } else {
            None
        };
        if !lost && restart_why.is_none() {
            continue;
        }
        match restart_why {
            Some(why) => tracing::warn!("{why}, reconnecting"),
            None => tracing::warn!("connection lost, reconnecting"),
        }
        inner.notify(StreamState::Reconnecting, Msg::new("status.connection_lost"));

        let mut reconnected = false;
        let mut fresh_stack = false;
        for attempt in 1..=10u32 {
            if stopped() {
                break;
            }
            inner.notify(StreamState::Reconnecting, Msg::with("status.reconnect_attempt", &[&attempt, &10]));
            // React to a stop within the wait
            let until = Instant::now() + Duration::from_secs(3);
            while Instant::now() < until && !stopped() {
                inner.wait_tick(Duration::from_millis(100));
            }
            if stopped() {
                break;
            }
            if restart_why.is_none() {
                if inner.btstack_stale.load(Ordering::Acquire) {
                    restart_why = Some("the PC resumed from sleep");
                } else if host.stalled() {
                    restart_why = Some("the Bluetooth adapter stopped working");
                }
            }
            if let Some(why) = restart_why.take() {
                // The pipeline's media sink belongs to the old stack
                if let Some(p) = pipeline.take() {
                    let _ = p.stop();
                }
                match inner.restart_host(why, req.max_media_payload) {
                    Ok(h) => {
                        host = h;
                        fresh_stack = true;
                    }
                    Err(_) => {
                        tracing::warn!("reconnect attempt {attempt} failed (no Bluetooth adapter)");
                        restart_why = Some("the last restart failed");
                        continue;
                    }
                }
                host.set_restore_volume(inner.saved_volume(&addr));
                if stopped() {
                    break;
                }
            }
            // A new stack knows neither the device nor the stream: connect
            // and configure in full
            let ok = if fresh_stack {
                host.connect(req.device).is_ok()
                    && (host.configure(&config).is_ok() && host.start_stream() || {
                        host.disconnect();
                        false
                    })
            } else {
                host.reconnect()
            };
            if ok {
                tracing::info!("reconnected on attempt {attempt}");
                reconnected = true;
                break;
            }
            tracing::warn!("reconnect attempt {attempt} failed");
        }
        if !reconnected {
            if !stopped() {
                inner.notify(StreamState::Error, Msg::new("error.reconnect_failed"));
                lost_for_good = true;
            }
            break;
        }
        // Resume with the same encoder settings; a new stack needs a new
        // pipeline (its media sink is new)
        match pipeline.as_ref() {
            Some(p) => p.reset(),
            None => {
                let Some(sink) = host.take_media_sink() else { break };
                let encoder = match codec::create(kind, &encoder_config) {
                    Ok(e) => e,
                    Err(_) => break,
                };
                let (p, new_input) = Pipeline::start(
                    PipelineConfig {
                        encoder,
                        input: input_format(format),
                        channels,
                        mtu,
                        abr: encoder_config.abr,
                        buffer: BufferPreset::Standard,
                    },
                    sink,
                    host.stats().clone(),
                );
                if let Ok(mut i) = input.0.lock() {
                    *i = new_input;
                }
                pipeline = Some(p);
            }
        }
        inner.notify(StreamState::Streaming, Msg::new("status.reconnected"));
    }

    // ---- cleanup: producer first, then the encode thread, then the stream ----
    tracing::info!("streaming stopped, cleaning up");
    active.stop();
    if let Some(p) = pipeline.take()
        && let Some(s) = p.stop()
    {
        host.return_media_sink(s);
    }
    if let Some(mut v) = volume.take() {
        v.stop();
    }
    host.stop_stream();
    host.disconnect();
    host.set_restore_volume(None);
    drop(default_guard);
    if !lost_for_good {
        inner.notify(StreamState::Idle, Msg::new("status.ready"));
    }
}
