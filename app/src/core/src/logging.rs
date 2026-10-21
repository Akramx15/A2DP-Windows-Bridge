//! Logging of the resident process: tracing to daily files in
//! `<config dir>\logs\` (v1 wrote one debug.log), to the console for `--cli`,
//! and to the UI's log view through a broadcast layer.
//!
//! - Files: `a2dpwb-core.YYYY-MM-DD.log` (UTC date, as tracing-appender
//!   names them), the last 7 kept. Written on a worker thread
//!   (tracing-appender's non-blocking writer, lossy when it falls behind), so
//!   logging never waits for the disk.
//! - Filter: `A2DPWB_LOG` (EnvFilter syntax, default "info"), for every
//!   output. Verbose logging ([`set_verbose`], the diagnostics' switch)
//!   adds A2DPWB's own modules at debug (24 hours) or trace (1 hour), kept in
//!   `logs\verbose.json` so that it also covers a restart (a problem at
//!   start-up); the filter is swapped while running.
//! - Panics are logged and written as crash reports (win_sys::crash, which
//!   also catches crashes in native code) to `logs\crash\`.
//! - Broadcast: events become [`ipc::LogLine`]s for subscribers
//!   ([`LogHub::subscribe`], [`LogHub::subscribe_with`]), each with its level
//!   and a bounded queue; a slow UI loses lines instead of blocking logging.
//! - The last [`RING_LINES`] lines stay in memory, and every line feeds the
//!   connection flow tracker (a2dpwb_core::diag), for the diagnostics view
//!   and "copy diagnostic info" without reading the files back.
//!
//! Cost per event without subscribers: the ring's slots are reused (their
//! strings keep their capacity once the ring is full), so formatting the
//! message into it does not allocate; the flow tracker only compares
//! prefixes.

// Until main.rs calls init() and the server subscribes; remove then

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use a2dpwb_core::diag::{ConnectionFlow, LogEntry};
use ipc::{LogLevel, LogLine, VerboseState};
use tracing::field::{Field, Visit};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry, reload};

/// Lines kept in memory for the diagnostics
pub const RING_LINES: usize = 2000;
/// Daily log files kept
pub const KEEP_FILES: usize = 7;
/// Lines queued per [`LogHub::subscribe`] receiver before new ones are
/// dropped
#[cfg_attr(not(test), allow(dead_code))]
pub const SUBSCRIBER_QUEUE: usize = 1024;
/// Lines waiting for the file writer before new ones are dropped. The
/// queue is allocated in full up front: tracing-appender's default (128 000
/// lines) alone is 4 MB of the resident core's memory.
const FILE_QUEUE_LINES: usize = 8192;
const FILE_PREFIX: &str = "a2dpwb-core";
/// How long verbose logging stays on at debug
pub const VERBOSE_FOR: Duration = Duration::from_secs(24 * 3600);
/// ...and at trace: every HCI and media packet, hundreds of MB an hour
pub const TRACE_FOR: Duration = Duration::from_secs(3600);
const VERBOSE_FILE: &str = "verbose.json";
/// A2DPWB's own modules (tracing targets): what verbose logging turns up,
/// leaving the libraries (tokio, reqwest, ...) at the base level
pub const OWN_TARGETS: &[&str] = &[
    "a2dpwb_core",
    "bt_host",
    "btstack_sys",
    "hci_transport",
    "codec",
    "audio_capture",
    "le_audio",
    "pipeline",
    "win_sys",
    "ipc",
    // Explicit targets: the USB transport, BTstack's own log, LE Audio
    "hci",
    "btstack",
    "le",
    // The USB library (through the log crate): transfer and device errors
    "nusb",
];

/// Where the logs go
#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    /// Folder of the daily files (`<config dir>\logs`); None = no files
    pub dir: Option<std::path::PathBuf>,
    /// Also to stderr (the command line)
    pub console: bool,
    /// Crash reports (minidumps) of panics and native crashes; None: none
    pub crash_dir: Option<PathBuf>,
}

/// Keeps the file writer's worker running; drop it last (at exit) so the
/// lines still queued are written
#[must_use = "dropping the guard stops the log file writer"]
pub struct LogGuard {
    _file: Option<WorkerGuard>,
}

/// Local wall-clock timestamps, "2026-10-17 12:34:56.789" (the offset is
/// read once at start: asking for it later is not reliable everywhere)
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

/// Bytes a day's log file takes before its debug and trace lines stop
/// (verbose logging at trace writes hundreds of MB an hour); info and
/// above always go on. A normal day is well under 1 MB.
pub const FILE_DAY_CAP: u64 = 100 * 1024 * 1024;

/// The UTC day (days since 1970) whose file reached [`FILE_DAY_CAP`]
static FULL_DAY: AtomicU64 = AtomicU64::new(u64::MAX);

/// Days since 1970, UTC: the log files' day
fn utc_day() -> u64 {
    unix_ms() / 86_400_000
}

/// The day's log file is full: debug and trace lines are not written
pub fn file_capped() -> bool {
    FULL_DAY.load(Ordering::Relaxed) == utc_day()
}

/// The daily file, counting what each day gets (on the writer thread)
struct CappedFile<W> {
    inner: W,
    day: u64,
    bytes: u64,
    cap: u64,
}

impl<W: std::io::Write> std::io::Write for CappedFile<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let today = utc_day();
        if today != self.day {
            self.day = today;
            self.bytes = 0;
        }
        let n = self.inner.write(buf)?;
        self.bytes += n as u64;
        if self.bytes >= self.cap && FULL_DAY.swap(today, Ordering::Relaxed) != today {
            let note = format!(
                "--- this file reached {} MB: debug and trace lines stop until the next day (UTC) ---\n",
                self.cap / (1024 * 1024)
            );
            let _ = self.inner.write_all(note.as_bytes());
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// `<dir>\a2dpwb-core.YYYY-MM-DD.log`, rotated daily, the last 7 kept
pub fn file_appender(dir: &Path) -> Result<RollingFileAppender, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(FILE_PREFIX)
        .filename_suffix("log")
        .max_log_files(KEEP_FILES)
        .build(dir)
        .map_err(|e| format!("cannot open a log file in {}: {e}", dir.display()))
}

/// Installs the global subscriber: filter, files, console, broadcast. Call
/// once, first thing in main (before other threads start: the local time
/// offset is read here).
pub fn init(opts: &LogOptions) -> LogGuard {
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let timer = LocalTimer { offset };
    // Verbose logging still on from an earlier run
    let verbose = opts.dir.as_deref().and_then(load_verbose);
    let (filter, filter_handle) = reload::Layer::new(make_filter(verbose.map(|v| v.level)));

    let mut file_error = None;
    let (file_layer, file_guard) = match opts.dir.as_deref().map(file_appender) {
        Some(Ok(appender)) => {
            let (writer, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
                .buffered_lines_limit(FILE_QUEUE_LINES)
                .finish(CappedFile { inner: appender, day: 0, bytes: 0, cap: FILE_DAY_CAP });
            // Debug and trace lines stop for the day once the file is full
            let capped = tracing_subscriber::filter::filter_fn(|meta| {
                *meta.level() <= tracing::Level::INFO || FULL_DAY.load(Ordering::Relaxed) != utc_day()
            });
            let layer = tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_timer(timer)
                .with_filter(capped);
            (Some(layer), Some(guard))
        }
        Some(Err(e)) => {
            file_error = Some(e);
            (None, None)
        }
        None => (None, None),
    };
    let console_layer = opts
        .console
        .then(|| tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_target(false).with_timer(timer));

    let hub = hub();
    hub.set_clock(offset);
    let installed = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(console_layer)
        .with(BroadcastLayer::new(hub.clone()))
        .try_init();
    if installed.is_err() {
        eprintln!("a2dpwb-core: logging was set up already");
    }
    let _ = FILTER.set(filter_handle);
    if let Some(dir) = &opts.dir {
        let _ = LOG_DIR.set(dir.clone());
    }
    *lock(&VERBOSE) = verbose;
    if let Some(e) = file_error {
        tracing::warn!("logging to files is off: {e}");
    }
    if let Some(dir) = &opts.crash_dir {
        install_crash_reports(dir);
    }
    log_header(verbose);
    LogGuard { _file: file_guard }
}

/// The first lines of a run: what runs where, for a log read without the
/// diagnostic info
fn log_header(verbose: Option<VerboseState>) {
    tracing::info!(
        "a2dpwb-core {} ({}) starting: pid {}, Windows build {}, {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH,
        std::process::id(),
        audio_capture::windows_build(),
        std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
    );
    if let Ok(env) = std::env::var("A2DPWB_LOG") {
        tracing::info!("log filter A2DPWB_LOG={env}");
    }
    if let Some(v) = verbose {
        let minutes = v.until_ms.saturating_sub(unix_ms()) / 60_000;
        tracing::info!("verbose logging ({:?}) on for {minutes} more minutes", v.level);
    }
}

/// Panics: logged, and a crash report with the backtrace. Native crashes:
/// a crash report (win_sys::crash).
fn install_crash_reports(dir: &Path) {
    win_sys::crash::install(dir, "a2dpwb-core", &format!("a2dpwb-core {}", env!("CARGO_PKG_VERSION")));
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let reason = format!("panic in thread {}: {info}", thread.name().unwrap_or("unnamed"));
        tracing::error!(target: "a2dpwb_core::crash", "{reason}");
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        if let Some(p) = win_sys::crash::write(&reason, &backtrace) {
            tracing::error!(target: "a2dpwb_core::crash", "crash report written: {}", p.display());
        }
        default_hook(info);
    }));
}

static FILTER: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
static VERBOSE: Mutex<Option<VerboseState>> = Mutex::new(None);

/// Milliseconds since 1970 (UTC)
pub fn unix_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn level_name(l: LogLevel) -> &'static str {
    match l {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
    }
}

/// The filter's directives: `base`, then A2DPWB's own modules at `verbose`
pub fn filter_directives(base: &str, verbose: Option<LogLevel>) -> String {
    let mut s = base.to_owned();
    if let Some(l) = verbose {
        for t in OWN_TARGETS {
            let _ = write!(s, ",{t}={}", level_name(l));
        }
    }
    s
}

fn make_filter(verbose: Option<LogLevel>) -> EnvFilter {
    let base = std::env::var("A2DPWB_LOG").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "info".into());
    EnvFilter::try_new(filter_directives(&base, verbose)).unwrap_or_else(|_| EnvFilter::new("info"))
}

/// Verbose logging only at debug and trace
fn verbose_level(level: Option<LogLevel>) -> Option<LogLevel> {
    level.filter(|l| matches!(l, LogLevel::Debug | LogLevel::Trace))
}

fn load_verbose(dir: &Path) -> Option<VerboseState> {
    let text = std::fs::read_to_string(dir.join(VERBOSE_FILE)).ok()?;
    let v: VerboseState = serde_json::from_str(&text).ok()?;
    (verbose_level(Some(v.level)).is_some() && v.until_ms > unix_ms()).then_some(v)
}

/// Verbose logging now (None: off)
pub fn verbose() -> Option<VerboseState> {
    *lock(&VERBOSE)
}

/// How long verbose logging at `level` stays on
pub fn verbose_for(level: LogLevel) -> Duration {
    if level == LogLevel::Trace { TRACE_FOR } else { VERBOSE_FOR }
}

/// Turns verbose logging on at `level` (debug or trace) for
/// [`verbose_for`], or off (None, or another level). Kept in the logs
/// folder for the next start.
pub fn set_verbose(level: Option<LogLevel>) -> Option<VerboseState> {
    let level = verbose_level(level);
    let state = level.map(|level| VerboseState { level, until_ms: unix_ms() + verbose_for(level).as_millis() as u64 });
    apply_verbose(state);
    match state {
        Some(v) => {
            tracing::info!("verbose logging on ({:?}) for {} minutes", v.level, verbose_for(v.level).as_secs() / 60)
        }
        None => tracing::info!("verbose logging off"),
    }
    state
}

fn apply_verbose(state: Option<VerboseState>) {
    *lock(&VERBOSE) = state;
    if let Some(h) = FILTER.get()
        && let Err(e) = h.reload(make_filter(state.map(|v| v.level)))
    {
        tracing::warn!("cannot change the log filter: {e}");
    }
    if let Some(dir) = LOG_DIR.get() {
        let path = dir.join(VERBOSE_FILE);
        let _ = match state {
            Some(v) => std::fs::write(&path, serde_json::to_vec(&v).unwrap_or_default()),
            None => std::fs::remove_file(&path),
        };
    }
}

/// Ends verbose logging that ran out; true if it did now (the caller
/// publishes the change)
pub fn expire_verbose() -> bool {
    let Some(v) = verbose() else { return false };
    if v.until_ms > unix_ms() {
        return false;
    }
    apply_verbose(None);
    tracing::info!("verbose logging off (its time is over)");
    true
}

/// The process' log hub (the one [`init`] installs)
pub fn hub() -> &'static LogHub {
    static HUB: OnceLock<LogHub> = OnceLock::new();
    HUB.get_or_init(|| LogHub::new(RING_LINES))
}

fn log_level(l: &tracing::Level) -> LogLevel {
    match *l {
        tracing::Level::ERROR => LogLevel::Error,
        tracing::Level::WARN => LogLevel::Warn,
        tracing::Level::INFO => LogLevel::Info,
        tracing::Level::DEBUG => LogLevel::Debug,
        tracing::Level::TRACE => LogLevel::Trace,
    }
}

/// ipc's level back to tracing's (diag takes tracing levels)
pub fn tracing_level(l: LogLevel) -> tracing::Level {
    match l {
        LogLevel::Error => tracing::Level::ERROR,
        LogLevel::Warn => tracing::Level::WARN,
        LogLevel::Info => tracing::Level::INFO,
        LogLevel::Debug => tracing::Level::DEBUG,
        LogLevel::Trace => tracing::Level::TRACE,
    }
}

/// A line of the ring as diag reads it
pub fn entry(l: &LogLine) -> LogEntry<'_> {
    LogEntry { time_ms: l.time_ms, level: tracing_level(l.level), target: &l.target, message: &l.message }
}

type Deliver = Box<dyn Fn(&LogLine) -> bool + Send + Sync>;

struct Subscriber {
    id: u64,
    level: LogLevel,
    /// false = the subscriber is gone
    deliver: Deliver,
}

/// The last lines, as reused slots
struct Ring {
    lines: Vec<LogLine>,
    /// Oldest line once the ring is full (the next slot to reuse)
    head: usize,
    capacity: usize,
    flow: ConnectionFlow,
    /// Debug and trace lines (verbose logging) take no slot: at trace they
    /// would push every warning out of the ring within a second. They are
    /// formatted here for the flow and the subscribers, and are in the file.
    verbose: LogLine,
}

impl Ring {
    fn ordered(&self) -> impl Iterator<Item = &LogLine> {
        let (newer, older) = self.lines.split_at(self.head);
        older.iter().chain(newer.iter())
    }
}

fn empty_line() -> LogLine {
    LogLine {
        time_ms: 0,
        level: LogLevel::Info,
        target: String::new(),
        message: String::new(),
        seq: None,
        session: None,
    }
}

/// The slot for a new line: a new one until the ring is full, then the
/// oldest
fn next_slot<'a>(lines: &'a mut Vec<LogLine>, head: &mut usize, capacity: usize) -> &'a mut LogLine {
    if lines.len() < capacity {
        lines.push(empty_line());
        let last = lines.len() - 1;
        &mut lines[last]
    } else {
        let i = *head;
        *head = (*head + 1) % capacity;
        &mut lines[i]
    }
}

struct HubInner {
    started: Instant,
    /// Local time of day (ms) at `started`
    clock: Mutex<Option<u64>>,
    ring: Mutex<Ring>,
    subscribers: Mutex<Vec<Subscriber>>,
    /// Number of subscribers: the per-event check without a lock
    active: AtomicUsize,
    next_id: AtomicU64,
    #[cfg_attr(not(test), allow(dead_code))]
    dropped: AtomicU64,
}

/// Recent lines, the connection flow, and the subscribers of the log
#[derive(Clone)]
pub struct LogHub(Arc<HubInner>);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl LogHub {
    pub fn new(capacity: usize) -> LogHub {
        let capacity = capacity.max(1);
        LogHub(Arc::new(HubInner {
            started: Instant::now(),
            clock: Mutex::new(None),
            ring: Mutex::new(Ring {
                lines: Vec::with_capacity(capacity),
                head: 0,
                capacity,
                flow: ConnectionFlow::new(),
                verbose: empty_line(),
            }),
            subscribers: Mutex::new(Vec::new()),
            active: AtomicUsize::new(0),
            next_id: AtomicU64::new(1),
            dropped: AtomicU64::new(0),
        }))
    }

    fn set_clock(&self, offset: time::UtcOffset) {
        let since = self.0.started.elapsed().as_millis() as u64;
        let now = time::OffsetDateTime::now_utc().to_offset(offset);
        let of_day = ((now.hour() as u64 * 60 + now.minute() as u64) * 60 + now.second() as u64) * 1000
            + now.millisecond() as u64;
        *lock(&self.0.clock) = Some((of_day + 86_400_000 - since % 86_400_000) % 86_400_000);
    }

    /// Local time of day (ms) at time_ms 0, for diag::format_time
    pub fn clock(&self) -> Option<u64> {
        *lock(&self.0.clock)
    }

    /// ms since the hub started (LogLine::time_ms)
    pub fn now_ms(&self) -> u64 {
        self.0.started.elapsed().as_millis() as u64
    }

    /// Lines from now on at `level` or more severe. The queue holds
    /// [`SUBSCRIBER_QUEUE`] lines; when it is full, newer lines are dropped.
    /// Dropping the receiver unsubscribes (at the next line).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn subscribe(&self, level: LogLevel) -> Receiver<LogLine> {
        let (tx, rx) = mpsc::sync_channel(SUBSCRIBER_QUEUE);
        let dropped = self.0.clone();
        self.add(
            level,
            Box::new(move |line| match tx.try_send(line.clone()) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => {
                    dropped.dropped.fetch_add(1, Ordering::Relaxed);
                    true
                }
                Err(TrySendError::Disconnected(_)) => false,
            }),
        );
        rx
    }

    /// Calls `deliver` with every line at `level` or more severe until it
    /// returns false or the [`Subscription`] is dropped. It runs on the
    /// logging thread with the subscriber list locked: it must only hand the
    /// line over (e.g. a tokio mpsc `try_send`), never block nor log.
    pub fn subscribe_with(
        &self,
        level: LogLevel,
        deliver: impl Fn(&LogLine) -> bool + Send + Sync + 'static,
    ) -> Subscription {
        let id = self.add(level, Box::new(deliver));
        Subscription { hub: self.clone(), id }
    }

    fn add(&self, level: LogLevel, deliver: Deliver) -> u64 {
        let id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
        let mut subs = lock(&self.0.subscribers);
        subs.push(Subscriber { id, level, deliver });
        self.0.active.store(subs.len(), Ordering::Release);
        id
    }

    fn remove(&self, id: u64) {
        let mut subs = lock(&self.0.subscribers);
        subs.retain(|s| s.id != id);
        self.0.active.store(subs.len(), Ordering::Release);
    }

    /// Lines dropped because a subscriber's queue was full
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn dropped(&self) -> u64 {
        self.0.dropped.load(Ordering::Relaxed)
    }

    /// The kept lines, oldest first
    pub fn recent(&self) -> Vec<LogLine> {
        lock(&self.0.ring).ordered().cloned().collect()
    }

    /// The connection flow as the log tells it
    pub fn flow(&self) -> ConnectionFlow {
        lock(&self.0.ring).flow.clone()
    }

    fn on_event(&self, event: &tracing::Event<'_>) {
        let meta = event.metadata();
        let time_ms = self.now_ms();
        let level = log_level(meta.level());
        let shared = {
            let mut ring = lock(&self.0.ring);
            let Ring { lines, head, capacity, flow, verbose } = &mut *ring;
            let slot = if level > LogLevel::Info { verbose } else { next_slot(lines, head, *capacity) };
            slot.time_ms = time_ms;
            slot.level = level;
            slot.target.clear();
            slot.target.push_str(meta.target());
            slot.message.clear();
            event.record(&mut MessageVisitor(&mut slot.message));
            flow.feed(time_ms, meta.target(), *meta.level(), &slot.message);
            // The flow's line count: steps name their line by it
            slot.seq = Some(flow.lines());
            slot.session = Some(flow.session().id);
            // A copy for the subscribers only when there are any
            (self.0.active.load(Ordering::Acquire) > 0).then(|| slot.clone())
        };
        if let Some(line) = shared {
            let mut subs = lock(&self.0.subscribers);
            subs.retain(|s| line.level > s.level || (s.deliver)(&line));
            self.0.active.store(subs.len(), Ordering::Release);
        }
    }
}

/// Ends a [`LogHub::subscribe_with`] subscription when dropped
pub struct Subscription {
    hub: LogHub,
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.hub.remove(self.id);
    }
}

/// The message, then other fields as " key=value" (as the fmt layer shows
/// them)
struct MessageVisitor<'a>(&'a mut String);

impl Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            let _ = write!(self.0, " {}={value}", field.name());
        }
    }
}

/// The tracing layer feeding a [`LogHub`]
pub struct BroadcastLayer {
    hub: LogHub,
}

impl BroadcastLayer {
    pub fn new(hub: LogHub) -> BroadcastLayer {
        BroadcastLayer { hub }
    }
}

impl<S: tracing::Subscriber> Layer<S> for BroadcastLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        self.hub.on_event(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2dpwb_core::diag::{Step, StepStatus};
    use std::time::Duration;

    /// Runs `f` with a subscriber that feeds `hub` (not the global one)
    fn with_hub(hub: &LogHub, f: impl FnOnce()) {
        let subscriber = tracing_subscriber::registry().with(BroadcastLayer::new(hub.clone()));
        tracing::subscriber::with_default(subscriber, f);
    }

    #[test]
    fn keeps_the_last_lines_in_order() {
        let hub = LogHub::new(3);
        with_hub(&hub, || {
            for i in 0..5 {
                tracing::info!(target: "t", "line {i}");
            }
        });
        let lines = hub.recent();
        let msgs: Vec<&str> = lines.iter().map(|l| l.message.as_str()).collect();
        assert_eq!(msgs, ["line 2", "line 3", "line 4"]);
        assert_eq!(lines[0].target, "t");
        assert!(lines.windows(2).all(|w| w[0].time_ms <= w[1].time_ms));
    }

    #[test]
    fn verbose_lines_do_not_push_warnings_out() {
        let hub = LogHub::new(3);
        let rx = hub.subscribe(LogLevel::Trace);
        with_hub(&hub, || {
            tracing::warn!(target: "t", "the warning");
            for i in 0..100 {
                tracing::trace!(target: "t", "packet {i}");
            }
            tracing::debug!(target: "t", "a decision");
        });
        let kept: Vec<String> = hub.recent().into_iter().map(|l| l.message).collect();
        assert_eq!(kept, ["the warning"]);
        // Subscribers still get them, numbered
        let got: Vec<LogLine> = rx.try_iter().collect();
        assert_eq!(got.len(), 102);
        assert_eq!((got[101].message.as_str(), got[101].seq), ("a decision", Some(102)));
    }

    #[test]
    fn reused_slots_keep_their_buffers() {
        let hub = LogHub::new(2);
        with_hub(&hub, || {
            tracing::info!("a fairly long first message to size the buffer");
            tracing::info!("another fairly long message for the second slot");
        });
        let before = {
            let ring = lock(&hub.0.ring);
            (ring.lines[0].message.as_ptr(), ring.lines[0].message.capacity())
        };
        with_hub(&hub, || tracing::info!("short"));
        let ring = lock(&hub.0.ring);
        assert_eq!(ring.lines[0].message, "short");
        // Same allocation: nothing was allocated for the new line
        assert_eq!((ring.lines[0].message.as_ptr(), ring.lines[0].message.capacity()), before);
    }

    #[test]
    fn fields_follow_the_message() {
        let hub = LogHub::new(10);
        with_hub(&hub, || {
            tracing::warn!(device = "AA:BB", attempt = 2, "connect failed");
            tracing::error!(name = "x");
        });
        let lines = hub.recent();
        assert_eq!(lines[0].message, "connect failed device=AA:BB attempt=2");
        assert_eq!(lines[0].level, LogLevel::Warn);
        assert_eq!(lines[1].message, " name=x");
        assert_eq!(lines[1].level, LogLevel::Error);
    }

    #[test]
    fn subscribers_get_their_levels() {
        let hub = LogHub::new(10);
        let warn = hub.subscribe(LogLevel::Warn);
        let all = hub.subscribe(LogLevel::Trace);
        with_hub(&hub, || {
            tracing::debug!("d");
            tracing::info!("i");
            tracing::warn!("w");
            tracing::error!("e");
        });
        let got: Vec<String> = warn.try_iter().map(|l| l.message).collect();
        assert_eq!(got, ["w", "e"]);
        assert_eq!(all.try_iter().count(), 4);
    }

    #[test]
    fn a_full_queue_drops_and_a_gone_subscriber_is_removed() {
        let hub = LogHub::new(10);
        let rx = hub.subscribe(LogLevel::Info);
        with_hub(&hub, || {
            for i in 0..SUBSCRIBER_QUEUE + 10 {
                tracing::info!("{i}");
            }
        });
        assert_eq!(rx.try_iter().count(), SUBSCRIBER_QUEUE);
        assert_eq!(hub.dropped(), 10);
        drop(rx);
        with_hub(&hub, || tracing::info!("after"));
        assert_eq!(hub.0.active.load(Ordering::Acquire), 0);
    }

    #[test]
    fn callback_subscription_ends_on_drop() {
        let hub = LogHub::new(10);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sub = {
            let seen = seen.clone();
            hub.subscribe_with(LogLevel::Info, move |l| {
                lock(&seen).push(l.message.clone());
                true
            })
        };
        with_hub(&hub, || tracing::info!("one"));
        drop(sub);
        with_hub(&hub, || tracing::info!("two"));
        assert_eq!(*lock(&seen), ["one"]);
        assert_eq!(hub.0.active.load(Ordering::Acquire), 0);
    }

    #[test]
    fn feeds_the_connection_flow() {
        let hub = LogHub::new(10);
        with_hub(&hub, || {
            tracing::info!(target: "a2dpwb_core::stream", "stream: device AA:BB:CC:DD:EE:FF codec None");
            tracing::info!(target: "bt_host", "connecting to AA:BB:CC:DD:EE:FF");
            tracing::info!(target: "bt_host::app", "signaling connection established (cid 0x0041)");
        });
        let flow = hub.flow();
        assert_eq!(flow.session().id, 1);
        assert_eq!(flow.session().step(Step::Connect).status, StepStatus::Done);
        // The same from the kept lines
        let lines = hub.recent();
        let again = ConnectionFlow::from_lines(lines.iter().map(entry));
        assert_eq!(again.session(), flow.session());
    }

    #[test]
    fn lines_carry_their_number_and_attempt() {
        let hub = LogHub::new(10);
        with_hub(&hub, || {
            tracing::info!(target: "t", "before");
            tracing::info!(target: "a2dpwb_core::stream", "stream: device AA:BB:CC:DD:EE:FF codec None");
            tracing::info!(target: "bt_host", "connecting to AA:BB:CC:DD:EE:FF");
        });
        let lines = hub.recent();
        let got: Vec<(Option<u64>, Option<u32>)> = lines.iter().map(|l| (l.seq, l.session)).collect();
        assert_eq!(got, [(Some(1), Some(0)), (Some(2), Some(1)), (Some(3), Some(1))]);
        let flow = hub.flow();
        assert_eq!(flow.session().first_seq, 2);
        assert_eq!(flow.session().step(Step::Connect).seq, 3);
    }

    #[test]
    fn clock_is_the_local_time_at_start() {
        let hub = LogHub::new(1);
        assert_eq!(hub.clock(), None);
        hub.set_clock(time::UtcOffset::UTC);
        let clock = hub.clock().unwrap();
        assert!(clock < 86_400_000);
        let now = time::OffsetDateTime::now_utc();
        let of_day = ((now.hour() as u64 * 60 + now.minute() as u64) * 60 + now.second() as u64) * 1000;
        let diff = (of_day + 86_400_000 - clock) % 86_400_000;
        assert!(!(5_000..=86_395_000).contains(&diff), "{clock} vs {of_day}");
    }

    #[test]
    fn writes_daily_files() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("a2dpwb-logs-{}", std::process::id()));
        let mut appender = file_appender(&dir).unwrap();
        appender.write_all(b"hello\n").unwrap();
        appender.flush().unwrap();
        drop(appender);
        std::thread::sleep(Duration::from_millis(50));
        let names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().any(|n| n.starts_with("a2dpwb-core.") && n.ends_with(".log")), "{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verbose_turns_up_our_modules_only() {
        assert_eq!(filter_directives("info", None), "info");
        let d = filter_directives("info", Some(LogLevel::Debug));
        assert!(d.starts_with("info,a2dpwb_core=debug,bt_host=debug,"), "{d}");
        assert!(!d.contains("tokio"));
        // Every directive parses
        assert!(EnvFilter::try_new(filter_directives("warn", Some(LogLevel::Trace))).is_ok());
        assert_eq!(verbose_level(Some(LogLevel::Info)), None);
        assert_eq!(verbose_level(Some(LogLevel::Trace)), Some(LogLevel::Trace));
    }

    #[test]
    fn verbose_survives_a_restart_until_it_runs_out() {
        let dir = std::env::temp_dir().join(format!("a2dpwb-verbose-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let on = VerboseState { level: LogLevel::Debug, until_ms: unix_ms() + 60_000 };
        std::fs::write(dir.join(VERBOSE_FILE), serde_json::to_vec(&on).unwrap()).unwrap();
        assert_eq!(load_verbose(&dir), Some(on));
        let over = VerboseState { until_ms: unix_ms() - 1, ..on };
        std::fs::write(dir.join(VERBOSE_FILE), serde_json::to_vec(&over).unwrap()).unwrap();
        assert_eq!(load_verbose(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_full_day_stops_the_verbose_lines() {
        use std::io::Write;
        let mut f = CappedFile { inner: Vec::new(), day: 0, bytes: 0, cap: 100 };
        f.write_all(&[b'x'; 60]).unwrap();
        assert!(!file_capped());
        f.write_all(&[b'x'; 60]).unwrap();
        assert!(file_capped());
        let text = String::from_utf8_lossy(&f.inner).into_owned();
        assert!(text.ends_with("debug and trace lines stop until the next day (UTC) ---\n"), "{text}");
        // The note once a day
        f.write_all(b"more").unwrap();
        assert_eq!(String::from_utf8_lossy(&f.inner).matches("---").count(), 2);
        FULL_DAY.store(u64::MAX, Ordering::Relaxed);
        assert!(!file_capped());
    }

    #[test]
    fn levels_round_trip() {
        for l in [LogLevel::Error, LogLevel::Warn, LogLevel::Info, LogLevel::Debug, LogLevel::Trace] {
            assert_eq!(log_level(&tracing_level(l)), l);
        }
    }
}
