// A fake core for `pnpm dev` in a plain browser and for the tests: sample
// state, and connect / stream / scan / driver changes / link samples / log
// lines / HCI capture / the peer receiver test simulated with timers. It
// speaks the same messages as the real core, including the revision of
// every event.
//
// URL options (browser): ?mock=fresh (first run: no profile, adapter still
// on Windows' Bluetooth driver), ?mock=error (the last connection failed),
// ?mock=offline (update checks and receivers cannot be reached),
// ?window=flyout (the flyout view).

import { FlowTracker, flowView } from "../lib/flow";
import type { ConnStatus, Transport, UpdatePhase, WindowKind } from "./transport";
import type {
  AdapterInfo,
  AudioApp,
  AudioDevice,
  CaptureState,
  Device,
  ErrorInfo,
  Event,
  FirmwareFile,
  FlowView,
  LinkSample,
  LinkTotals,
  LogLevel,
  LogLine,
  Profile,
  ReceiverInfo,
  ReceiverState,
  ServerMessage,
  Settings,
  Snapshot,
  SourceInfo,
  StreamInfo,
} from "./types";

export type MockScenario = "normal" | "fresh" | "error" | "update" | "offline";

export interface MockOptions {
  scenario?: MockScenario;
  window?: WindowKind;
  /** Multiplies every simulated delay (0 in tests: next tick) */
  speed?: number;
  /** Reports method calls (tests) */
  onCall?: (method: string, params: unknown) => void;
}

const RTL: AdapterInfo = {
  instance_id: "USB\\VID_2357&PID_0604\\00E04C239987",
  vid: 0x2357,
  pid: 0x0604,
  display_name: "RTL8761BU (0x2357:0x0604)",
  device_name: "TP-Link UB500 Adapter",
  vendor: "Realtek",
  driver: "winusb",
  driver_service: "WinUSB",
  realtek_chip: "RTL8761BU",
  firmware_present: true,
  firmware_files: ["rtl8761bu_fw.bin", "rtl8761bu_config.bin"],
  vendor_note: null,
};

export function sampleProfile(over: Partial<Profile> = {}): Profile {
  return {
    name: "WH-1000XM5 (LDAC)",
    device_address: "AC:80:0A:12:34:56",
    device_name: "WH-1000XM5",
    codec: "ldac",
    quality: "hq",
    abr: false,
    codec_fallback: true,
    sample_rate: 0,
    bit_depth: 0,
    capture_mode: "loopback",
    audio_device_id: "",
    audio_device_name: "",
    auto_switch_device: false,
    app_exe: "",
    app_name: "",
    volume_sync: "auto",
    volume_device_id: "",
    volume_device_name: "",
    max_media_payload: 1023,
    ...over,
  };
}

export function sampleSettings(over: Partial<Settings> = {}): Settings {
  return {
    language: "en",
    theme: "dark",
    start_with_windows: true,
    start_minimized: false,
    minimize_to_tray: true,
    check_updates_on_startup: true,
    include_prereleases: false,
    media_keys: true,
    remember_volume: true,
    developer_mode: false,
    bt_chip_pid: 0,
    bt_chip_fw_stem: "",
    afh: "auto",
    remote_sink: "",
    ...over,
  };
}

export function sampleSnapshot(scenario: MockScenario = "normal"): Snapshot {
  const fresh = scenario === "fresh";
  const profiles = fresh
    ? []
    : [
        sampleProfile(),
        sampleProfile({
          name: "Desk speaker (AAC)",
          device_address: "00:1A:7D:DA:71:13",
          device_name: "SRS-XB13",
          codec: "aac",
          capture_mode: "app",
          app_exe: "Spotify.exe",
          app_name: "Spotify",
        }),
      ];
  const config_dir = "C:\\Users\\you\\AppData\\Roaming\\A2DPWB";
  return {
    adapter: {
      status: "stopped",
      adapters: [fresh ? { ...RTL, driver: "bluetooth", driver_service: "BTHUSB", firmware_present: false } : RTL],
      error: null,
    },
    devices: fresh
      ? []
      : [
          {
            address: "AC:80:0A:12:34:56",
            name: "WH-1000XM5",
            saved: true,
            audio: true,
            rssi: null,
            codecs: ["sbc", "aac", "ldac"],
          },
          { address: "00:1A:7D:DA:71:13", name: "SRS-XB13", saved: true, audio: true, rssi: null, codecs: [] },
        ],
    scanning: false,
    stream:
      scenario === "error"
        ? {
            state: "error",
            message: { key: "error.connect_no_answer", args: [] },
            profile: "WH-1000XM5 (LDAC)",
            info: null,
            volume: null,
          }
        : { state: "idle", message: { key: "status.idle", args: [] }, profile: null, info: null, volume: null },
    profiles,
    active_profile: fresh ? null : "WH-1000XM5 (LDAC)",
    settings: sampleSettings(),
    update: {
      checking: false,
      available:
        scenario === "update"
          ? {
              version: "2.0.0-beta.1",
              url: "https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/releases/tag/v2.0.0-beta.1",
              prerelease: true,
              manifest_url: "https://github.com/SeiyaFunaokaJP/A2DP-Windows-Bridge/releases/download/v2.0.0-beta.1/latest.json",
            }
          : null,
      current_version: "2.0.0-alpha.1",
      error: null,
    },
    config_dir,
    log_dir: `${config_dir}\\logs`,
    capture: null,
  };
}

const SCAN_RESULTS: Device[] = [
  { address: "AC:80:0A:12:34:56", name: "WH-1000XM5", saved: true, audio: true, rssi: -48, codecs: [] },
  { address: "F4:4E:FD:00:11:22", name: "Galaxy Buds2 Pro", saved: false, audio: true, rssi: -61, codecs: [] },
  { address: "74:45:CE:9A:BC:DE", name: "", saved: false, audio: true, rssi: -83, codecs: [] },
  { address: "38:18:4C:55:66:77", name: "MX Keys", saved: false, audio: false, rssi: -70, codecs: [] },
];

const AUDIO_DEVICES: AudioDevice[] = [
  { id: "{0.0.0.00000000}.{speakers}", name: "Speakers (Realtek(R) Audio)", is_default: true, is_virtual: false, mix_rate: 48000, mix_channels: 2, mix_bits: 32 },
  { id: "{0.0.0.00000000}.{cable}", name: "CABLE Input (VB-Audio Virtual Cable)", is_default: false, is_virtual: true, mix_rate: 44100, mix_channels: 2, mix_bits: 24 },
  { id: "{0.0.0.00000000}.{voicemeeter}", name: "Voicemeeter Input (VB-Audio Voicemeeter VAIO)", is_default: false, is_virtual: true, mix_rate: 48000, mix_channels: 8, mix_bits: 32 },
];

const AUDIO_APPS: AudioApp[] = [
  { exe: "Spotify.exe", name: "Spotify" },
  { exe: "chrome.exe", name: "Google Chrome" },
  { exe: "foobar2000.exe", name: "foobar2000" },
];

const FIRMWARE_FILES: FirmwareFile[] = [
  { stem: "rtl8761bu", display_name: "RTL8761BU", pid: 0x8771, has_config: true },
  { stem: "rtl8723du", display_name: "RTL8723DU", pid: null, has_config: false },
];

const RECEIVER: ReceiverInfo = {
  ip: "192.168.1.42",
  port: 51201,
  host: "sink-pi",
  bt_address: "DC:A6:32:01:02:03",
  bt_name: "a2dpwb-sink",
  codecs: ["SBC", "AAC", "aptX", "aptX HD", "LDAC"],
  mtu: 895,
  buffer_ms: 200,
};

const CODEC_INFO: Record<string, StreamInfo> = {
  ldac: { codec: "LDAC", bitrate_kbps: 990, sample_rate: 96000, channels: 2, source_sample_rate: 48000, source_channels: 2, source_bits: 32, requested_codec: null, requested_in_use: false, source: null },
  aac: { codec: "AAC", bitrate_kbps: 256, sample_rate: 44100, channels: 2, source_sample_rate: 48000, source_channels: 2, source_bits: 32, requested_codec: null, requested_in_use: false, source: null },
  sbc: { codec: "SBC", bitrate_kbps: 328, sample_rate: 48000, channels: 2, source_sample_rate: 48000, source_channels: 2, source_bits: 32, requested_codec: null, requested_in_use: false, source: null },
  auto: { codec: "LDAC", bitrate_kbps: 990, sample_rate: 48000, channels: 2, source_sample_rate: 48000, source_channels: 2, source_bits: 32, requested_codec: null, requested_in_use: false, source: null },
};

/** What a connection to the mock's devices learns: the codecs they offer */
const DEVICE_CODECS = ["sbc", "aac", "ldac"];

const ZERO: LinkTotals = { packets: 0, bytes: 0, dropped: 0, capture_frames: 0, capture_dropped: 0 };

function errorInfo(code: string, key: string, args: string[] = []): ErrorInfo {
  return { code, message: { key, args } };
}

/** What a stream sends: a profile's or stream.start_custom's */
interface StreamTarget {
  address: string;
  codec: string;
  profile: string | null;
  source: SourceInfo;
  sampleRate: number | null;
}

function sourceOf(mode: string, id: string, name: string): SourceInfo {
  switch (mode) {
    case "tone":
      return { kind: "tone", name: "", waiting: false };
    case "virtual":
      return { kind: "virtual", name: name || id, waiting: false };
    case "app":
      return { kind: "app", name: id, waiting: false };
    default:
      return { kind: "loopback", name: "", waiting: false };
  }
}

export class MockCore implements Transport {
  readonly window: WindowKind;
  readonly mica = false;
  readonly version = "2.0.0-alpha.1";
  private snap: Snapshot;
  private scenario: MockScenario;
  private rev = 0;
  private onMessage: (m: ServerMessage) => void = () => {};
  private speed: number;
  private onCall?: (method: string, params: unknown) => void;
  private timers = new Set<ReturnType<typeof setTimeout>>();
  private linkTimer: ReturnType<typeof setInterval> | null = null;
  private slowTimer: ReturnType<typeof setInterval> | null = null;
  private logs: LogLevel | null = null;
  private receiverSub = false;
  /** Every line, as the core keeps them, and the flow it follows */
  private kept: LogLine[] = [];
  private tracker = new FlowTracker();
  private seq = 0;
  /** The line that set each step of the current attempt, and its first */
  private stepSeq = new Map<string, number>();
  private firstSeq: number | null = null;
  private clock = 0;
  private startedAt = Date.now();
  private dropped = 0;
  /** Cumulative counters, and those when the stream started */
  private totals: LinkTotals = { ...ZERO };
  private streamStart: LinkTotals = { ...ZERO };
  private streamStarts = 0;
  private lastTick = 0;
  private receiver: ReceiverState = {
    connection: "off",
    detail: "",
    target: "",
    receiver: null,
    stats: null,
    stats_age_ms: null,
    sent: null,
    stream_starts: 0,
  };
  /** The receiver's own counters of the stream it measures */
  private received = { id: 0, packets: 0, bytes: 0, lost: 0, underruns: 0, underrun_ms: 0 };
  private trayHintShown = false;
  private navigate: (route: string) => void = () => {};
  private updatePhase: UpdatePhase = { state: "idle" };
  private onPhase: (p: UpdatePhase) => void = () => {};

  constructor(opts: MockOptions = {}) {
    this.scenario = opts.scenario ?? "normal";
    this.snap = sampleSnapshot(opts.scenario);
    this.window = opts.window ?? "main";
    this.speed = opts.speed ?? 1;
    this.onCall = opts.onCall;
  }

  // ---- Transport ---------------------------------------------------------

  async start(onMessage: (m: ServerMessage) => void, onStatus: (s: ConnStatus) => void) {
    this.onMessage = onMessage;
    onStatus({ state: "connecting" });
    this.later(400, () => {
      onMessage({ type: "welcome", protocol: 1, server: "a2dpwb-core/2.0.0-alpha.1 (mock)" });
      onStatus({ state: "connected", server: "a2dpwb-core/2.0.0-alpha.1 (mock)" });
      onMessage({ type: "snapshot", rev: this.rev, state: structuredClone(this.snap) });
    });
  }

  async call(method: string, params?: unknown): Promise<unknown> {
    // What the real transport does: the parameters cross as JSON (and stop
    // being store proxies)
    params = params === undefined ? undefined : JSON.parse(JSON.stringify(params));
    this.onCall?.(method, params);
    await new Promise((r) => this.later(30, () => r(undefined)));
    const p = (params ?? {}) as Record<string, unknown>;
    switch (method) {
      case "state.snapshot":
        return structuredClone(this.snap);
      case "stream.start":
        return this.startStream(String(p.profile));
      case "stream.start_custom":
        return this.startCustom(p);
      case "stream.stop":
        this.stopStream();
        return null;
      case "stream.set_volume":
        if (this.snap.stream.state === "streaming") {
          this.snap.stream.volume = Math.max(0, Math.min(127, Number(p.volume)));
          this.emit({ kind: "stream", value: this.snap.stream });
        }
        return null;
      case "device.scan":
        return this.scan(Number(p.seconds ?? 10));
      case "device.forget":
        this.snap.devices = this.snap.devices.filter((d) => d.address !== p.address);
        this.emit({ kind: "devices", value: this.snap.devices });
        return null;
      case "device.rename": {
        const name = String(p.name ?? "").trim();
        if (!name) throw errorInfo("device.invalid_name", "ipc.error.bad_request", ["device.rename", `name "${name}"`]);
        const address = String(p.address).toUpperCase();
        const d = this.snap.devices.find((x) => x.address === address);
        if (d) {
          d.name = name;
          d.saved = true;
        } else {
          this.snap.devices.push({ address, name, saved: true, audio: true, rssi: null, codecs: [] });
        }
        this.emit({ kind: "devices", value: this.snap.devices });
        return null;
      }
      case "profile.save": {
        const profile = p.profile as Profile;
        const original = (p.original_name as string | null) ?? profile.name;
        if (!profile.name.trim()) throw errorInfo("profile.invalid", "profile.name");
        const i = this.snap.profiles.findIndex((x) => x.name === original);
        if (i >= 0) this.snap.profiles[i] = profile;
        else this.snap.profiles.push(profile);
        if (!this.snap.active_profile || this.snap.active_profile === original) this.snap.active_profile = profile.name;
        const known = this.snap.devices.find((d) => d.address === profile.device_address);
        if (known) known.saved = true;
        else
          this.snap.devices.push({
            address: profile.device_address,
            name: profile.device_name,
            saved: true,
            audio: true,
            rssi: null,
            codecs: [],
          });
        this.emit({ kind: "devices", value: this.snap.devices });
        this.emitProfiles();
        return null;
      }
      case "profile.delete":
        this.snap.profiles = this.snap.profiles.filter((x) => x.name !== p.name);
        if (this.snap.active_profile === p.name) this.snap.active_profile = this.snap.profiles[0]?.name ?? null;
        this.emitProfiles();
        return null;
      case "profile.set_active":
        this.snap.active_profile = String(p.name);
        this.emitProfiles();
        return null;
      case "settings.set": {
        const before = this.snap.settings;
        this.snap.settings = p.settings as Settings;
        this.emit({ kind: "settings", value: this.snap.settings });
        if (before.bt_chip_pid !== this.snap.settings.bt_chip_pid || before.bt_chip_fw_stem !== this.snap.settings.bt_chip_fw_stem) {
          // The real core releases an idle stack so the firmware loads again
          this.log("info", "a2dpwb_core_exe::state", "adapter settings changed: Bluetooth stopped, it starts again at the next use");
          if (this.snap.stream.state === "idle" || this.snap.stream.state === "error") this.setAdapter("stopped");
        }
        return null;
      }
      case "adapter.list":
        return this.snap.adapter.adapters;
      case "adapter.assign_winusb":
      case "adapter.restore_driver":
        return this.changeDriver(String(p.instance_id), method === "adapter.assign_winusb");
      case "adapter.restart":
        this.setAdapter("initializing");
        this.later(900, () => this.setAdapter("ready"));
        return null;
      case "adapter.firmware_files":
        return FIRMWARE_FILES;
      case "audio.devices":
        return AUDIO_DEVICES;
      case "audio.apps":
        return AUDIO_APPS;
      case "diag.subscribe":
        this.subscribe((p.logs as LogLevel | null) ?? null, Boolean(p.link), Boolean(p.receiver));
        return null;
      case "diag.capture_start":
        return this.startCapture();
      case "diag.capture_stop":
        return this.stopCapture();
      case "diag.info":
        return { text: this.diagText(p.mask !== false) };
      case "diag.flow":
        return this.flow();
      case "diag.recent_logs": {
        const order: LogLevel[] = ["error", "warn", "info", "debug", "trace"];
        const max = order.indexOf(p.level as LogLevel);
        return this.kept.filter((l) => order.indexOf(l.level) <= max);
      }
      case "receiver.discover":
        await new Promise((r) => this.later(1500, () => r(undefined)));
        return this.scenario === "offline" ? [] : [RECEIVER];
      case "receiver.connect":
        return this.connectReceiver(String(p.target ?? ""));
      case "receiver.disconnect":
        this.receiver = { ...this.receiver, connection: "off", detail: "", target: "", receiver: null, stats: null, stats_age_ms: null };
        this.publishReceiver();
        return null;
      case "receiver.configure": {
        if (this.receiver.connection !== "connected") throw errorInfo("receiver.not_connected", "receiver.off");
        const r = this.receiver.receiver!;
        if (typeof p.mtu === "number" && p.mtu > 0) r.mtu = p.mtu;
        if (typeof p.buffer_ms === "number" && p.buffer_ms > 0) r.buffer_ms = p.buffer_ms;
        return null;
      }
      case "receiver.status":
        return this.receiverState();
      case "app.check_update": {
        this.snap.update = { ...this.snap.update, checking: true };
        this.emit({ kind: "update", value: this.snap.update });
        await new Promise((r) => this.later(800, () => r(undefined)));
        if (this.scenario === "offline") {
          const error = { key: "update.error", args: [] };
          this.snap.update = { ...this.snap.update, checking: false, error };
          this.emit({ kind: "update", value: this.snap.update });
          throw { code: "update.failed", message: error } satisfies ErrorInfo;
        }
        this.snap.update = { checking: false, available: null, current_version: this.snap.update.current_version, error: null };
        this.emit({ kind: "update", value: this.snap.update });
        return this.snap.update;
      }
      case "app.hidden_to_tray":
        if (!this.trayHintShown) {
          this.trayHintShown = true;
          console.info("[mock] tray balloon: tray.minimized_hint");
        }
        return null;
      case "app.quit":
        return null;
      default:
        throw errorInfo("ipc.bad_request", "ipc.error.bad_request", [method, "unknown method"]);
    }
  }

  async showMain(route?: string) {
    if (route) this.navigate(route);
  }
  async hideWindow() {}
  async openUrl(url: string) {
    window.open(url, "_blank", "noopener");
  }
  async openFolder(path: string) {
    console.info("[mock] open folder", path);
  }
  async revealPath(path: string) {
    console.info("[mock] reveal", path);
  }
  async setTheme() {}
  onNavigate(cb: (route: string) => void) {
    this.navigate = cb;
  }
  onMinimized() {}

  async updateStatus() {
    return this.updatePhase;
  }
  /** Downloads in a second, then installs once nothing streams */
  async updateInstall(manifestUrl: string) {
    const version = /download\/v?([^/]+)\/latest\.json$/.exec(manifestUrl)?.[1] ?? "?";
    const total = 9_400_000;
    const set = (p: UpdatePhase) => {
      this.updatePhase = p;
      this.onPhase(p);
    };
    for (let i = 1; i <= 5; i++) {
      this.later(i * 200, () => set({ state: "downloading", version, downloaded: (total * i) / 5, total }));
    }
    const install = () => {
      if (this.snap.stream.state === "idle" || this.snap.stream.state === "error") {
        set({ state: "installing", version });
      } else {
        set({ state: "waiting", version });
        this.later(1000, install);
      }
    };
    this.later(1100, install);
  }
  onUpdatePhase(cb: (p: UpdatePhase) => void) {
    this.onPhase = cb;
  }

  /** Stops every timer (tests) */
  dispose() {
    for (const t of this.timers) clearTimeout(t);
    this.timers.clear();
    if (this.linkTimer) clearInterval(this.linkTimer);
    this.linkTimer = null;
    if (this.slowTimer) clearInterval(this.slowTimer);
    this.slowTimer = null;
  }

  // ---- Simulation --------------------------------------------------------

  private later(ms: number, f: () => void) {
    const t = setTimeout(() => {
      this.timers.delete(t);
      f();
    }, ms * this.speed);
    this.timers.add(t);
  }

  private emit(event: Event) {
    this.rev += 1;
    this.onMessage({ type: "event", rev: this.rev, event: structuredClone(event) });
  }

  private emitProfiles() {
    this.emit({ kind: "profiles", value: { profiles: this.snap.profiles, active_profile: this.snap.active_profile } });
  }

  private now() {
    this.clock = Math.max(this.clock + 1, Date.now() - this.startedAt);
    return this.clock;
  }

  private log(level: LogLevel, target: string, message: string) {
    const order: LogLevel[] = ["error", "warn", "info", "debug", "trace"];
    this.seq += 1;
    const before = this.tracker.session;
    const was = before ? { id: before.id, steps: structuredClone(before.steps) } : null;
    const line: LogLine = { time_ms: this.now(), level, target, message, seq: this.seq, session: null };
    this.tracker.feed(line);
    // The line that set each step, as the core's flow names it
    const s = this.tracker.session;
    if (s) {
      if (!was || was.id !== s.id) {
        this.stepSeq.clear();
        this.firstSeq = this.seq;
      }
      for (const [step, st] of Object.entries(s.steps)) {
        const old = was && was.id === s.id ? was.steps[step as keyof typeof s.steps] : null;
        if (!old || old.status !== st.status || old.time_ms !== st.time_ms) {
          if (st.time_ms !== null) this.stepSeq.set(step, this.seq);
        }
      }
    }
    line.session = s?.id ?? 0;
    this.kept.push(line);
    if (this.kept.length > 2000) this.kept.shift();
    if (this.logs === null || order.indexOf(level) > order.indexOf(this.logs)) return;
    // Log lines take no revision of their own (as the core sends them)
    this.onMessage({ type: "event", rev: this.rev, event: { kind: "log", value: line } });
  }

  private flow(): FlowView {
    const v = flowView(this.tracker.session);
    const s = this.tracker.session;
    return {
      ...v,
      first_seq: s ? this.firstSeq : null,
      steps: v.steps.map((st) => ({
        ...st,
        seq: this.stepSeq.get(st.step) ?? null,
        time_ms: s && st.elapsed_ms !== null ? s.start_ms + st.elapsed_ms : null,
      })),
    };
  }

  private setAdapter(status: Snapshot["adapter"]["status"]) {
    this.snap.adapter = { ...this.snap.adapter, status, error: null };
    this.emit({ kind: "adapter", value: this.snap.adapter });
  }

  private setStream(state: Snapshot["stream"]["state"], key: string, args: string[] = []) {
    this.snap.stream = { ...this.snap.stream, state, message: { key, args } };
    this.log("info", "a2dpwb_core::service", `state ${state[0].toUpperCase()}${state.slice(1)}: ${key}`);
    this.emit({ kind: "stream", value: this.snap.stream });
  }

  private startStream(name: string): null {
    const profile = this.snap.profiles.find((x) => x.name === name);
    if (!profile) throw errorInfo("profile.not_found", "profile.none_selected");
    this.connect({
      address: profile.device_address,
      codec: profile.codec,
      profile: name,
      source: sourceOf(profile.capture_mode, profile.capture_mode === "app" ? profile.app_exe : profile.audio_device_id, profile.audio_device_name),
      sampleRate: profile.sample_rate || null,
    });
    return null;
  }

  private startCustom(p: Record<string, unknown>): null {
    const address = String(p.device ?? "").trim().toUpperCase();
    if (!/^([0-9A-F]{2}:){5}[0-9A-F]{2}$/.test(address)) {
      throw errorInfo("profile.invalid_address", "error.invalid_address_format");
    }
    const codec = String(p.codec || "auto").toLowerCase();
    if (!["auto", "ldac", "aptxhd", "aptxll", "aptx", "sbc", "aac", "lhdcv5"].includes(codec)) {
      throw errorInfo("stream.bad_codec", "ipc.error.bad_request", ["stream.start_custom", `unknown codec "${codec}"`]);
    }
    const mode = String(p.source || "loopback");
    if (!["tone", "loopback", "virtual", "app"].includes(mode)) {
      throw errorInfo("stream.bad_source", "ipc.error.bad_request", ["stream.start_custom", `unknown source "${mode}"`]);
    }
    this.connect({
      address,
      codec,
      profile: null,
      source: sourceOf(mode, String(p.source_id ?? ""), String(p.source_name ?? "")),
      sampleRate: typeof p.sample_rate === "number" && p.sample_rate > 0 ? p.sample_rate : null,
    });
    return null;
  }

  private connect(t: StreamTarget) {
    const name = t.profile;
    const adapter = this.snap.adapter.adapters[0];
    if (!adapter || adapter.driver !== "winusb") {
      const e = errorInfo("adapter.driver_missing", "error.btstack_init");
      this.snap.stream = { state: "error", message: e.message, profile: name, info: null, volume: null };
      this.emit({ kind: "stream", value: this.snap.stream });
      throw e;
    }
    this.dropped = 0;
    this.snap.stream = { state: "connecting", message: { key: "status.initializing_btstack", args: [] }, profile: name, info: null, volume: null };
    this.emit({ kind: "stream", value: this.snap.stream });
    this.log("info", "a2dpwb_core::stream", `stream: device ${t.address} codec ${t.codec} quality hq source ${t.source.kind}`);
    this.log("info", "a2dpwb_core::service", "state Connecting: Initializing BTstack...");
    if (this.snap.adapter.status !== "ready") this.setAdapter("initializing");
    const info = (): StreamInfo => {
      const base = CODEC_INFO[t.codec] ?? { ...CODEC_INFO.aac, codec: t.codec.toUpperCase() };
      // The app is not running in the mock: application capture waits for it
      const source = { ...t.source, waiting: t.source.kind === "app" };
      return { ...base, sample_rate: t.sampleRate ?? base.sample_rate, source };
    };
    this.later(500, () => {
      if (this.snap.stream.state !== "connecting") return;
      this.log("info", "bt_host::app", "controller: company 0x005D (Realtek), HCI version 10, LMP subversion 0x8761");
      this.log("info", "bt_host", "HCI ready after 412 ms");
      if (this.snap.adapter.status !== "ready") this.setAdapter("ready");
      this.setStream("connecting", "status.connecting_device");
      this.log("info", "bt_host", `connecting to ${t.address}`);
    });
    this.later(1200, () => {
      if (this.snap.stream.state !== "connecting") return;
      this.log("info", "bt_host::app", `HCI ACL connection established to ${t.address}`);
      this.log("info", "bt_host::app", "signaling connection established (cid 0x0041, handle 0x000b)");
      this.log("info", "bt_host::app", "capability discovery complete (LDAC=true aptX HD=false aptX LL=false aptX=false aptX Adaptive=false [not encodable] SBC=true AAC=true LHDC V5=false)");
      this.learnCodecs(t.address);
      const i = info();
      this.log("info", "a2dpwb_core::stream", `codec selected: ${i.codec}`);
      this.log("info", "a2dpwb_core::stream", "capture: 48000 Hz, 2 channel(s), F32 -> 2 channel(s)");
      this.log("info", "bt_host::app", "codec configured (local 1, remote 3)");
      this.log("info", "bt_host::app", "stream established (local 1, remote 3, mtu 1005, used 1005, limit 1023)");
      this.log("info", "a2dpwb_core::stream", `encoder ready: ${i.codec} ${i.bitrate_kbps} kbps, mtu 1005, 24-bit, ABR false`);
      this.log("info", "bt_host::app", "streaming started");
    });
    this.later(1600, () => {
      if (this.snap.stream.state !== "connecting") return;
      this.streamStart = { ...this.totals };
      this.streamStarts += 1;
      if (t.address === RECEIVER.bt_address) {
        this.received = { id: this.received.id + 1, packets: 0, bytes: 0, lost: 0, underruns: 0, underrun_ms: 0 };
      }
      this.snap.stream = { state: "streaming", message: { key: "status.streaming", args: [] }, profile: name, info: info(), volume: 76 };
      this.log("info", "a2dpwb_core::service", "state Streaming: Streaming");
      this.emit({ kind: "stream", value: this.snap.stream });
    });
  }

  /** A connection learnt what the device offers (and saved it, as the core does) */
  private learnCodecs(address: string) {
    const d = this.snap.devices.find((x) => x.address === address);
    if (d) {
      d.codecs = [...DEVICE_CODECS];
      d.saved = true;
    } else {
      this.snap.devices.push({ address, name: "", saved: true, audio: true, rssi: null, codecs: [...DEVICE_CODECS] });
    }
    this.emit({ kind: "devices", value: this.snap.devices });
  }

  private stopStream() {
    const was = this.snap.stream.state;
    this.snap.stream = { state: "idle", message: { key: "status.idle", args: [] }, profile: null, info: null, volume: null };
    if (was !== "idle") this.log("info", "a2dpwb_core::stream", "streaming stopped, cleaning up");
    this.emit({ kind: "stream", value: this.snap.stream });
  }

  private scan(seconds: number): Promise<Device[]> {
    this.snap.scanning = true;
    this.emit({ kind: "scanning", value: true });
    this.log("info", "bt_host", `starting inquiry (~${seconds} s)`);
    return new Promise((resolve) => {
      this.later(Math.min(seconds, 3) * 1000, () => {
        const known = new Map(this.snap.devices.map((d) => [d.address, d]));
        const found = SCAN_RESULTS.map((d) => ({
          ...d,
          saved: known.get(d.address)?.saved ?? false,
          codecs: known.get(d.address)?.codecs ?? [],
        }));
        const merged = [...this.snap.devices.filter((d) => !found.some((f) => f.address === d.address)), ...found];
        this.snap.devices = merged;
        this.snap.scanning = false;
        this.emit({ kind: "devices", value: merged });
        this.emit({ kind: "scanning", value: false });
        resolve(found);
      });
    });
  }

  private changeDriver(instance: string, winusb: boolean): Promise<AdapterInfo[]> {
    const a = this.snap.adapter.adapters.find((x) => x.instance_id === instance);
    if (!a) return Promise.reject(errorInfo("adapter.not_found", "firmware.no_adapters"));
    return new Promise((resolve) => {
      // The UAC prompt and the driver installation take a moment
      this.later(1500, () => {
        this.snap.adapter = {
          ...this.snap.adapter,
          adapters: this.snap.adapter.adapters.map((x) =>
            x.instance_id === instance
              ? { ...x, driver: winusb ? "winusb" : "bluetooth", driver_service: winusb ? "WinUSB" : "BTHUSB" }
              : x,
          ),
        };
        this.emit({ kind: "adapter", value: this.snap.adapter });
        resolve(this.snap.adapter.adapters);
      });
    });
  }

  // ---- HCI capture ---------------------------------------------------------

  private startCapture(): string {
    if (this.snap.capture) return this.snap.capture.path;
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    const stamp = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}_${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
    const capture: CaptureState = { path: `${this.snap.config_dir}\\hci_${stamp}.pklg`, bytes: 64, started_ms: Date.now() };
    this.snap.capture = capture;
    this.log("info", "bt_host::capture", `HCI capture: started -> ${capture.path}`);
    this.emit({ kind: "capture", value: capture });
    this.ensureSlowTimer();
    return capture.path;
  }

  private stopCapture(): CaptureState | null {
    const c = this.snap.capture;
    if (!c) return null;
    this.log("info", "bt_host::capture", `HCI capture: stopped (${c.bytes} bytes) -> ${c.path}`);
    this.snap.capture = null;
    this.emit({ kind: "capture", value: null });
    return c;
  }

  // ---- Receiver test -------------------------------------------------------

  private connectReceiver(target: string): ReceiverState {
    const t = target.trim() || "auto";
    if (/\s/.test(t)) throw errorInfo("receiver.invalid_target", "ipc.error.bad_request", ["receiver.connect", `"${t}"`]);
    if (this.snap.settings.remote_sink !== t) {
      this.snap.settings = { ...this.snap.settings, remote_sink: t };
      this.emit({ kind: "settings", value: this.snap.settings });
    }
    this.receiver = { ...this.receiver, connection: "connecting", detail: "", target: t, receiver: null, stats: null, stats_age_ms: null };
    this.publishReceiver();
    this.later(t === "auto" ? 1500 : 400, () => {
      if (this.receiver.connection === "off" || this.receiver.target !== t) return;
      if (this.scenario === "offline") {
        const detail = t === "auto" ? "no receiver found on the local network" : "connection refused";
        this.receiver = { ...this.receiver, connection: "failed", detail };
      } else {
        this.receiver = { ...this.receiver, connection: "connected", receiver: { ...RECEIVER } };
      }
      this.publishReceiver();
    });
    this.ensureSlowTimer();
    return this.receiverState();
  }

  private receiverState(): ReceiverState {
    const r = this.receiver;
    const connected = r.connection === "connected";
    const toReceiver = this.snap.stream.state === "streaming" && this.snap.stream.info !== null && connected;
    const sent = this.streamStarts > 0 ? this.since(this.totals, this.streamStart) : null;
    return {
      ...r,
      stats: connected
        ? {
            connected: toReceiver,
            streaming: toReceiver,
            device: toReceiver ? "00:E0:4C:23:99:87" : "",
            buffer_target_ms: r.receiver?.buffer_ms ?? 200,
            mtu: r.receiver?.mtu ?? 895,
            codecs: RECEIVER.codecs,
            rssi: toReceiver ? -2 : null,
            tx_power: toReceiver ? 4 : null,
            rssi_note: toReceiver ? "" : "no connection",
            afh_channels: toReceiver ? 58 : null,
            stream:
              this.received.id === 0
                ? null
                : {
                    id: this.received.id,
                    codec: this.snap.stream.info?.codec ?? "LDAC",
                    config: `${this.snap.stream.info?.codec ?? "LDAC"} ${(this.snap.stream.info?.sample_rate ?? 48000) / 1000} kHz`,
                    decoding: true,
                    packets: this.received.packets,
                    bytes: this.received.bytes,
                    frames: this.received.packets * 4,
                    lost: this.received.lost,
                    late: 0,
                    ts_errors: 0,
                    frame_errors: 0,
                    decode_errors: 0,
                    underruns: this.received.underruns,
                    overflows: 0,
                    pauses: 0,
                    idle: !toReceiver,
                    audio_ms: (this.received.packets * 1000) / 120,
                    underrun_ms: this.received.underrun_ms,
                    overflow_ms: 0,
                    max_gap_ms: 18,
                    jitter_ms: 1.8,
                    buffer_ms: toReceiver ? 185 : null,
                    interval_packets: toReceiver ? 120 : 0,
                    interval_bytes: toReceiver ? 120 * 1005 : 0,
                    interval_max_gap_ms: toReceiver ? 14 : 0,
                  },
          }
        : null,
      stats_age_ms: connected ? 400 : null,
      sent,
      stream_starts: this.streamStarts,
    };
  }

  private publishReceiver() {
    if (!this.receiverSub) return;
    // Subscription data: no revision of its own
    this.onMessage({ type: "event", rev: this.rev, event: { kind: "receiver", value: this.receiverState() } });
  }

  // ---- Counters and subscriptions -----------------------------------------

  private since(a: LinkTotals, b: LinkTotals): LinkTotals {
    return {
      packets: a.packets - b.packets,
      bytes: a.bytes - b.bytes,
      dropped: a.dropped - b.dropped,
      capture_frames: a.capture_frames - b.capture_frames,
      capture_dropped: a.capture_dropped - b.capture_dropped,
    };
  }

  /** The counters move while streaming (whether anyone watches or not) */
  private advance() {
    const now = Date.now();
    const dt = this.lastTick ? Math.min((now - this.lastTick) / 1000, 2) : 0;
    this.lastTick = now;
    if (this.snap.stream.state !== "streaming" || dt === 0) return;
    const info = this.snap.stream.info;
    const packets = Math.round(120 * dt);
    const bytes = Math.round(((info?.bitrate_kbps ?? 328) * 1000 * dt) / 8);
    const frames = Math.round((info?.source_sample_rate ?? 48000) * dt);
    const drops = Math.random() < 0.04 * dt * 4 ? 1 + Math.floor(Math.random() * 3) : 0;
    this.dropped += drops;
    this.totals = {
      packets: this.totals.packets + packets,
      bytes: this.totals.bytes + bytes,
      dropped: this.totals.dropped + drops,
      capture_frames: this.totals.capture_frames + frames,
      capture_dropped: this.totals.capture_dropped,
    };
    if (this.receiver.connection === "connected" && this.received.id > 0) {
      this.received.packets += packets;
      this.received.bytes += bytes;
      this.received.lost += drops;
    }
  }

  private subscribe(logs: LogLevel | null, link: boolean, receiver: boolean) {
    this.logs = logs;
    if (link && !this.linkTimer) {
      this.linkTimer = setInterval(() => this.sample(), 250 * Math.max(this.speed, 0.01));
    } else if (!link && this.linkTimer) {
      clearInterval(this.linkTimer);
      this.linkTimer = null;
    }
    const was = this.receiverSub;
    this.receiverSub = receiver;
    if (receiver) {
      if (!was) this.publishReceiver();
      this.ensureSlowTimer();
    }
  }

  /** Once a second: counters, receiver test, capture size */
  private ensureSlowTimer() {
    if (this.slowTimer) return;
    this.slowTimer = setInterval(() => {
      this.advance();
      this.publishReceiver();
      if (this.snap.capture) {
        this.snap.capture = { ...this.snap.capture, bytes: this.snap.capture.bytes + 4096 + Math.floor(Math.random() * 2048) };
        this.emit({ kind: "capture", value: this.snap.capture });
      }
    }, 1000 * Math.max(this.speed, 0.01));
  }

  private sample() {
    this.advance();
    const streaming = this.snap.stream.state === "streaming";
    const t = Date.now() / 1000;
    const kbps = streaming ? (this.snap.stream.info?.bitrate_kbps ?? 0) * (0.96 + 0.06 * Math.sin(t / 2)) : 0;
    const pps = streaming ? 120 + Math.random() * 6 : 0;
    const bps = (kbps * 1000) / 8;
    const avg = pps > 0 ? bps / pps : null;
    const slots = avg === null ? 0 : avg + 4 <= 83 ? 2 : avg + 4 <= 552 ? 4 : Math.ceil((avg + 4) / 1021) * 6;
    const sample: LinkSample = {
      time_ms: this.now(),
      rssi: streaming ? Math.round(-4 + 5 * Math.sin(t / 3) + Math.random() * 2) : null,
      afh_in_use: streaming ? 58 : null,
      afh_usable: streaming ? 79 : null,
      afh_avoided: streaming ? "6" : "",
      bytes_per_second: bps,
      packets_per_second: pps,
      queue_depth: streaming ? Math.max(0, Math.round(2 + 3 * Math.sin(t) + Math.random() * 2)) : 0,
      dropped: this.since(this.totals, this.streamStart).dropped,
      capture_dropped: 0,
      airtime: streaming ? pps * slots * 625e-6 : 0,
      avg_packet_bytes: avg,
      acl_packet: avg === null ? null : { name: avg + 4 <= 83 ? "3-DH1" : avg + 4 <= 552 ? "3-DH3" : "3-DH5", slots },
      bandwidth_share: streaming ? (pps * ((avg ?? 0) + 4) * 8) / 1000 / 2178.13 : null,
      queue_capacity: 64,
      source_rate: streaming ? (this.snap.stream.info?.source_sample_rate ?? 0) : 0,
      counters: { total: { ...this.totals }, stream_start: { ...this.streamStart }, stream_starts: this.streamStarts },
    };
    this.onMessage({ type: "event", rev: this.rev, event: { kind: "link", value: sample } });
  }

  private diagText(mask: boolean) {
    const s = this.snap;
    const text = [
      "A2DP Windows Bridge 2.0.0-alpha.1 (mock core)",
      `Adapter: ${s.adapter.adapters.map((a) => `${a.display_name} [${a.driver_service}]`).join(", ") || "none"}`,
      `Firmware files: ${s.adapter.adapters.map((a) => `${a.display_name}: ${a.firmware_present === null ? "none needed" : a.firmware_present ? "present" : "MISSING"} (${a.firmware_files.join(", ")})`).join("; ")}`,
      `State: ${s.stream.state}${s.stream.info ? ` ${s.stream.info.codec} ${s.stream.info.bitrate_kbps} kbps` : ""}`,
      `Profiles: ${s.profiles.map((p) => `${p.name} [${p.device_address}]`).join(", ")}`,
      `Config: ${s.config_dir}`,
    ].join("\n");
    // AA:BB:CC:xx:xx:xx, as the core masks them
    return mask ? text.replace(/\b([0-9A-F]{2}:[0-9A-F]{2}:[0-9A-F]{2}):[0-9A-F]{2}:[0-9A-F]{2}:[0-9A-F]{2}\b/gi, "$1:xx:xx:xx") : text;
  }
}

/** The mock as configured by the page URL (browser dev mode) */
export function mockFromUrl(): MockCore {
  const q = new URLSearchParams(location.search);
  const scenario = (q.get("mock") as MockScenario | null) ?? "normal";
  const win = q.get("window") === "flyout" ? "flyout" : "main";
  return new MockCore({ scenario, window: win });
}
