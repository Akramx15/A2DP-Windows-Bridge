//! a2dpwb-core.exe: the resident A2DPWB process (core service, tray, IPC
//! for the UI). `--cli` runs the command line instead (same core, no tray
//! and no IPC). A GUI-subsystem program: the resident process opens no
//! console window; `--cli` attaches to the console it was started from.
//! `--quit` asks a running core to exit (the installer uses it).

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

mod cli;
mod driver;
mod logging;
mod quit;
mod resident;
mod server;
mod state;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--quit") {
        // Neither logs nor the settings folder: only the running core's
        win_sys::console::attach_parent();
        std::process::exit(quit::run());
    }
    let cli_mode = args.iter().any(|a| a == "--cli");
    if cli_mode {
        win_sys::console::attach_parent();
    }
    // The command line logs to the console only; the resident process to
    // daily files in the settings folder
    let log = logging::init(&logging::LogOptions {
        dir: (!cli_mode).then(|| a2dpwb_core::paths::config_dir().join("logs")),
        console: cli_mode,
    });
    let code = if cli_mode { cli::run(&args) } else { resident::run(&args) };
    // process::exit skips destructors: flush the log first
    drop(log);
    std::process::exit(code);
}
/// The text of "copy diagnostic info"
/// The connection flow for the diagnostics view (diag.flow)
pub(crate) fn flow_view(state: &state::CoreState) -> ipc::FlowView {
    use a2dpwb_core::StreamState as S;
    let flow = logging::hub().flow();
    let sess = flow.session();
    let now = match state.snapshot().1.stream.state {
        ipc::StreamState::Idle => S::Idle,
        ipc::StreamState::Connecting => S::Connecting,
        ipc::StreamState::Streaming => S::Streaming,
        ipc::StreamState::Reconnecting => S::Reconnecting,
        ipc::StreamState::Error => S::Error,
    };
    let steps = flow
        .rows(Some(now), None)
        .into_iter()
        .map(|r| ipc::FlowStepView {
            step: r.step.key().trim_start_matches("dbgc.step.").into(),
            status: r.status.key().trim_start_matches("dbgc.status.").into(),
            elapsed_ms: r.elapsed_ms,
            detail: r.detail,
            detail_msg: r.detail_msg.map(|m| ipc::Msg { key: m.key.into(), args: m.args }),
        })
        .collect();
    ipc::FlowView {
        session: sess.id,
        start_ms: sess.start_ms,
        stop_ms: sess.stop_ms,
        reconnects: sess.reconnects,
        steps,
    }
}

pub(crate) fn diag_text(state: &state::CoreState) -> String {
    use a2dpwb_core::diag;
    let (_, s) = state.snapshot();
    let hub = logging::hub();
    let flow = hub.flow();
    let lines = hub.recent();
    let log: Vec<_> = lines.iter().map(logging::entry).collect();
    let adapters: Vec<String> = a2dpwb_core::adapter::adapters(false).iter().map(diag::adapter_line).collect();
    let settings = state.service.settings();
    let profile = s
        .active_profile
        .as_ref()
        .and_then(|n| a2dpwb_core::profiles::load(&state.config_dir()).into_iter().find(|p| &p.name == n));
    let stream_state = match s.stream.state {
        ipc::StreamState::Idle => a2dpwb_core::StreamState::Idle,
        ipc::StreamState::Connecting => a2dpwb_core::StreamState::Connecting,
        ipc::StreamState::Streaming => a2dpwb_core::StreamState::Streaming,
        ipc::StreamState::Reconnecting => a2dpwb_core::StreamState::Reconnecting,
        ipc::StreamState::Error => a2dpwb_core::StreamState::Error,
    };
    let info = s.stream.info.as_ref().map(|i| a2dpwb_core::StreamInfo {
        codec: i.codec.clone(),
        bitrate_kbps: i.bitrate_kbps,
        sample_rate: i.sample_rate,
        channels: i.channels,
        source_sample_rate: i.source_sample_rate,
        source_channels: i.source_channels,
        source_bits: i.source_bits,
        requested_codec: i.requested_codec.clone(),
        requested_in_use: i.requested_in_use,
    });
    let stats = state.service.stats();
    let capture = state.service.capture();
    let capture_path = capture.path();
    diag::diag_text(&diag::DiagInput {
        env: diag::Environment::current(),
        settings: &settings,
        state: stream_state,
        adapters: &adapters,
        firmware: None,
        profile: profile.as_ref(),
        stream: info.as_ref(),
        stats: Some(&stats),
        hci_capture: capture_path.as_deref().map(|p| (p, capture.bytes_written())),
        flow: &flow,
        rates: None,
        log: &log,
        clock: hub.clock(),
        mask: true,
    })
}
