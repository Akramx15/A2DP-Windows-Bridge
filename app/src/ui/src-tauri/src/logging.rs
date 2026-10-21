//! The window process' own log: `<config dir>\logs\a2dpwb-ui.YYYY-MM-DD.log`
//! next to the core's (the last 7 kept, local timestamps as the core's).
//! It holds what the core cannot see: the pages loading, errors in the
//! frontend, the connection to the core, and whether a window that was
//! shown actually has its page drawn (a blank or hung webview is the
//! "white window").
//!
//! Filter: `A2DPWB_LOG` (EnvFilter syntax, default "info"), as the core.
//! Verbose logging the core keeps in `logs\verbose.json` also turns this
//! process' own modules up, from its next start.
//!
//! Panics and crashes in native code (WebView2's host side) leave crash
//! reports in `logs\crash\` (win_sys::crash).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewWindow};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const FILE_PREFIX: &str = "a2dpwb-ui";
const KEEP_FILES: usize = 7;
/// A frontend message is cut to this many characters
const MAX_MESSAGE: usize = 2000;
/// A shown window's page is probed this long after the show...
const PROBE_DELAY: Duration = Duration::from_secs(3);
/// ...and must answer within this time
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);

/// The settings folder, as a2dpwb_core::paths::config_dir finds it
fn config_dir() -> PathBuf {
    match std::env::var_os("A2DPWB_CONFIG_DIR").filter(|v| !v.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => match std::env::var_os("APPDATA") {
            Some(a) => PathBuf::from(a).join("A2DPWB"),
            None => PathBuf::from("."),
        },
    }
}

/// Local wall-clock timestamps, "2026-10-21 23:11:18.502" (as the core's)
#[derive(Clone, Copy)]
struct LocalTimer {
    offset: time::UtcOffset,
}

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        let t = time::OffsetDateTime::now_utc().to_offset(self.offset);
        write!(
            w,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
            t.year(),
            u8::from(t.month()),
            t.day(),
            t.hour(),
            t.minute(),
            t.second(),
            t.millisecond()
        )
    }
}

/// Installs the file logger and the panic hook; keep the guard until exit
/// (the lines still queued are written when it drops). Call first thing in
/// main: the local time offset is read here, before other threads start.
pub fn init() -> Option<WorkerGuard> {
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let dir = config_dir().join("logs");
    let appender = std::fs::create_dir_all(&dir).ok().and_then(|()| {
        RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix(FILE_PREFIX)
            .filename_suffix("log")
            .max_log_files(KEEP_FILES)
            .build(&dir)
            .ok()
    })?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let verbose = verbose_level(&dir);
    let base = std::env::var("A2DPWB_LOG").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "info".into());
    let directives = match verbose {
        Some(l) => format!("{base},a2dpwb={l},frontend={l},win_sys={l}"),
        None => base,
    };
    let filter = tracing_subscriber::EnvFilter::try_new(directives)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let layer = tracing_subscriber::fmt::layer().with_writer(writer).with_ansi(false).with_timer(LocalTimer { offset });
    let _ = tracing_subscriber::registry().with(filter).with(layer).try_init();
    if let Some(l) = verbose {
        tracing::info!("verbose logging ({l}) as the core's");
    }
    let about = format!("a2dpwb-ui {}", env!("CARGO_PKG_VERSION"));
    win_sys::crash::install(&dir.join("crash"), "a2dpwb-ui", &about);
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let reason = format!("panic in thread {}: {info}", std::thread::current().name().unwrap_or("unnamed"));
        tracing::error!("{reason}");
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        if let Some(p) = win_sys::crash::write(&reason, &backtrace) {
            tracing::error!("crash report written: {}", p.display());
        }
        default_hook(info);
    }));
    Some(guard)
}

/// The level of the core's verbose logging while it is on ("debug",
/// "trace"): its `verbose.json` in the logs folder
fn verbose_level(dir: &std::path::Path) -> Option<&'static str> {
    let text = std::fs::read_to_string(dir.join("verbose.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
    if v.get("until_ms")?.as_u64()? <= now {
        return None;
    }
    match v.get("level")?.as_str()? {
        "debug" => Some("debug"),
        "trace" => Some("trace"),
        _ => None,
    }
}

/// A line from the frontend (uncaught errors, a failed start)
#[tauri::command]
pub fn ui_log(window: WebviewWindow, level: String, message: String) {
    let message: String = message.chars().take(MAX_MESSAGE).collect();
    let label = window.label();
    match level.as_str() {
        "error" => tracing::error!(target: "frontend", "[{label}] {message}"),
        "warn" => tracing::warn!(target: "frontend", "[{label}] {message}"),
        _ => tracing::info!(target: "frontend", "[{label}] {message}"),
    }
}

/// Probes waiting for their page's answer, by window label
#[derive(Default)]
pub struct Probes(Mutex<HashMap<String, u64>>);

/// After a window was shown: asks its page, a moment later (the first show
/// comes before the page loaded), whether it is drawn. An answer with an
/// empty root, or none at all (the renderer hung or crashed), is logged as
/// a warning.
pub fn probe(app: &AppHandle, window: &WebviewWindow) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let label = window.label().to_string();
    let (app, window) = (app.clone(), window.clone());
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(PROBE_DELAY).await;
        if !window.is_visible().unwrap_or(false) {
            return;
        }
        let Some(probes) = app.try_state::<Probes>() else { return };
        probes.0.lock().unwrap_or_else(|e| e.into_inner()).insert(label.clone(), id);
        let script = format!(
            "window.__TAURI_INTERNALS__.invoke('ui_probe', {{ id: {id}, root: document.getElementById('root')?.childElementCount ?? -1, text: document.body?.innerText.length ?? -1, hash: location.hash, ready: document.readyState }})"
        );
        if let Err(e) = window.eval(&script) {
            tracing::warn!("[{label}] cannot probe the page: {e}");
            return;
        }
        tokio::time::sleep(PROBE_TIMEOUT).await;
        let mut map = probes.0.lock().unwrap_or_else(|e| e.into_inner());
        if map.get(&label) == Some(&id) {
            map.remove(&label);
            tracing::warn!(
                "[{label}] the page did not answer within {} s: the webview is hung or its renderer crashed (blank window)",
                PROBE_TIMEOUT.as_secs()
            );
        }
    });
}

#[tauri::command]
pub fn ui_probe(
    window: WebviewWindow,
    probes: tauri::State<'_, Probes>,
    id: u64,
    root: i64,
    text: i64,
    hash: String,
    ready: String,
) {
    let label = window.label();
    let mut map = probes.0.lock().unwrap_or_else(|e| e.into_inner());
    if map.get(label) != Some(&id) {
        return;
    }
    map.remove(label);
    let hash: String = hash.chars().take(80).collect();
    if root <= 0 {
        tracing::warn!(
            "[{label}] page shown but empty (blank window): root children {root}, {text} chars of text, document {ready}, route {hash:?}"
        );
    } else {
        tracing::info!("[{label}] page shown: {text} chars of text, route {hash:?}");
    }
}
