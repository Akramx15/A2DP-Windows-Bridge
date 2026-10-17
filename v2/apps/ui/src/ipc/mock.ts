// A fake core for `pnpm dev` in a plain browser and for the tests: sample
// state, and connect / stream / scan / driver changes / link samples / log
// lines simulated with timers. It speaks the same messages as the real
// core, including the revision of every event.
//
// URL options (browser): ?mock=fresh (first run: no profile, adapter still
// on Windows' Bluetooth driver), ?mock=error (the last connection failed),
// ?window=flyout (the flyout view).

import type { ConnStatus, Transport, WindowKind } from "./transport";
import type {
  AdapterInfo,
  AudioApp,
  AudioDevice,
  Device,
  ErrorInfo,
  Event,
  LinkSample,
  LogLevel,
  LogLine,
  Profile,
  ServerMessage,
  Settings,
  Snapshot,
  StreamInfo,
} from "./types";

export type MockScenario = "normal" | "fresh" | "error";

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
  return {
    adapter: {
      status: "stopped",
      adapters: [fresh ? { ...RTL, driver: "bluetooth", driver_service: "BTHUSB", firmware_present: false } : RTL],
      error: null,
    },
    devices: fresh
      ? []
      : [
          { address: "AC:80:0A:12:34:56", name: "WH-1000XM5", saved: true, audio: true, rssi: null },
          { address: "00:1A:7D:DA:71:13", name: "SRS-XB13", saved: true, audio: true, rssi: null },
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
    update: { checking: false, available: null, current_version: "2.0.0-alpha.1" },
    config_dir: "C:\\Users\\you\\AppData\\Roaming\\A2DPWB",
  };
}

const SCAN_RESULTS: Device[] = [
  { address: "AC:80:0A:12:34:56", name: "WH-1000XM5", saved: true, audio: true, rssi: -48 },
  { address: "F4:4E:FD:00:11:22", name: "Galaxy Buds2 Pro", saved: false, audio: true, rssi: -61 },
  { address: "74:45:CE:9A:BC:DE", name: "", saved: false, audio: true, rssi: -83 },
  { address: "38:18:4C:55:66:77", name: "MX Keys", saved: false, audio: false, rssi: -70 },
];

const AUDIO_DEVICES: AudioDevice[] = [
  { id: "{0.0.0.00000000}.{speakers}", name: "Speakers (Realtek(R) Audio)", is_default: true, is_virtual: false },
  { id: "{0.0.0.00000000}.{cable}", name: "CABLE Input (VB-Audio Virtual Cable)", is_default: false, is_virtual: true },
  { id: "{0.0.0.00000000}.{voicemeeter}", name: "Voicemeeter Input (VB-Audio Voicemeeter VAIO)", is_default: false, is_virtual: true },
];

const AUDIO_APPS: AudioApp[] = [
  { exe: "Spotify.exe", name: "Spotify" },
  { exe: "chrome.exe", name: "Google Chrome" },
  { exe: "foobar2000.exe", name: "foobar2000" },
];

const CODEC_INFO: Record<string, StreamInfo> = {
  ldac: { codec: "LDAC", bitrate_kbps: 990, sample_rate: 96000, channels: 2, source_sample_rate: 48000, source_channels: 2, source_bits: 32, requested_codec: null, requested_in_use: false },
  aac: { codec: "AAC", bitrate_kbps: 256, sample_rate: 44100, channels: 2, source_sample_rate: 48000, source_channels: 2, source_bits: 32, requested_codec: null, requested_in_use: false },
  auto: { codec: "LDAC", bitrate_kbps: 990, sample_rate: 48000, channels: 2, source_sample_rate: 48000, source_channels: 2, source_bits: 32, requested_codec: null, requested_in_use: false },
};

function errorInfo(code: string, key: string, args: string[] = []): ErrorInfo {
  return { code, message: { key, args } };
}

export class MockCore implements Transport {
  readonly window: WindowKind;
  readonly mica = false;
  readonly version = "2.0.0-alpha.1";
  private snap: Snapshot;
  private rev = 0;
  private onMessage: (m: ServerMessage) => void = () => {};
  private speed: number;
  private onCall?: (method: string, params: unknown) => void;
  private timers = new Set<ReturnType<typeof setTimeout>>();
  private linkTimer: ReturnType<typeof setInterval> | null = null;
  private logs: LogLevel | null = null;
  private clock = 0;
  private startedAt = Date.now();
  private capturing: string | null = null;
  private dropped = 0;
  private navigate: (route: string) => void = () => {};

  constructor(opts: MockOptions = {}) {
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
        else this.snap.devices.push({ address: profile.device_address, name: profile.device_name, saved: true, audio: true, rssi: null });
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
      case "settings.set":
        this.snap.settings = p.settings as Settings;
        this.emit({ kind: "settings", value: this.snap.settings });
        return null;
      case "adapter.list":
        return this.snap.adapter.adapters;
      case "adapter.assign_winusb":
      case "adapter.restore_driver":
        return this.changeDriver(String(p.instance_id), method === "adapter.assign_winusb");
      case "adapter.restart":
        this.setAdapter("initializing");
        this.later(900, () => this.setAdapter("ready"));
        return null;
      case "audio.devices":
        return AUDIO_DEVICES;
      case "audio.apps":
        return AUDIO_APPS;
      case "diag.subscribe":
        this.subscribe((p.logs as LogLevel | null) ?? null, Boolean(p.link));
        return null;
      case "diag.capture_start":
        this.capturing = `${this.snap.config_dir}\\hci_capture_20261017_101500.pklg`;
        this.log("info", "bt_host::capture", `HCI capture: started -> ${this.capturing}`);
        return this.capturing;
      case "diag.capture_stop":
        if (this.capturing) this.log("info", "bt_host::capture", `HCI capture: stopped (182344 bytes) -> ${this.capturing}`);
        this.capturing = null;
        return null;
      case "diag.info":
        return { text: this.diagText() };
      case "app.check_update": {
        this.snap.update = { ...this.snap.update, checking: true };
        this.emit({ kind: "update", value: this.snap.update });
        await new Promise((r) => this.later(800, () => r(undefined)));
        this.snap.update = { checking: false, available: null, current_version: this.snap.update.current_version };
        this.emit({ kind: "update", value: this.snap.update });
        return this.snap.update;
      }
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

  /** Stops every timer (tests) */
  dispose() {
    for (const t of this.timers) clearTimeout(t);
    this.timers.clear();
    if (this.linkTimer) clearInterval(this.linkTimer);
    this.linkTimer = null;
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
    if (this.logs === null || order.indexOf(level) > order.indexOf(this.logs)) return;
    const line: LogLine = { time_ms: this.now(), level, target, message };
    // Log lines take no revision of their own (as the core sends them)
    this.onMessage({ type: "event", rev: this.rev, event: { kind: "log", value: line } });
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
    this.log("info", "a2dpwb_core::stream", `stream: device ${profile.device_address} codec ${profile.codec} quality ${profile.quality} source ${profile.capture_mode}`);
    this.log("info", "a2dpwb_core::service", "state Connecting: Initializing BTstack...");
    if (this.snap.adapter.status !== "ready") this.setAdapter("initializing");
    this.later(500, () => {
      if (this.snap.stream.state !== "connecting") return;
      this.log("info", "bt_host::app", "controller: company 0x005D (Realtek), HCI version 10, LMP subversion 0x8761");
      this.log("info", "bt_host", "HCI ready after 412 ms");
      if (this.snap.adapter.status !== "ready") this.setAdapter("ready");
      this.setStream("connecting", "status.connecting_device");
      this.log("info", "bt_host", `connecting to ${profile.device_address}`);
    });
    this.later(1200, () => {
      if (this.snap.stream.state !== "connecting") return;
      this.log("info", "bt_host::app", `HCI ACL connection established to ${profile.device_address}`);
      this.log("info", "bt_host::app", "signaling connection established (cid 0x0041, handle 0x000b)");
      this.log("info", "bt_host::app", "capability discovery complete (LDAC=true aptX HD=false aptX LL=false aptX=false aptX Adaptive=false [not encodable] SBC=true AAC=true LHDC V5=false)");
      const info = CODEC_INFO[profile.codec] ?? { ...CODEC_INFO.aac, codec: profile.codec.toUpperCase() };
      this.log("info", "a2dpwb_core::stream", `codec selected: ${info.codec}`);
      this.log("info", "a2dpwb_core::stream", "capture: 48000 Hz, 2 channel(s), F32 -> 2 channel(s)");
      this.log("info", "bt_host::app", "codec configured (local 1, remote 3)");
      this.log("info", "bt_host::app", "stream established (local 1, remote 3, mtu 1005, used 1005, limit 1023)");
      this.log("info", "a2dpwb_core::stream", `encoder ready: ${info.codec} ${info.bitrate_kbps} kbps, mtu 1005, 24-bit, ABR false`);
      this.log("info", "bt_host::app", "streaming started");
    });
    this.later(1600, () => {
      if (this.snap.stream.state !== "connecting") return;
      const info = CODEC_INFO[profile.codec] ?? { ...CODEC_INFO.aac, codec: profile.codec.toUpperCase() };
      this.snap.stream = { state: "streaming", message: { key: "status.streaming", args: [] }, profile: name, info, volume: 76 };
      this.log("info", "a2dpwb_core::service", "state Streaming: Streaming");
      this.emit({ kind: "stream", value: this.snap.stream });
    });
    return null;
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
        const saved = new Set(this.snap.devices.filter((d) => d.saved).map((d) => d.address));
        const found = SCAN_RESULTS.map((d) => ({ ...d, saved: saved.has(d.address) }));
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

  private subscribe(logs: LogLevel | null, link: boolean) {
    this.logs = logs;
    if (link && !this.linkTimer) {
      this.linkTimer = setInterval(() => this.sample(), 250 * Math.max(this.speed, 0.01));
    } else if (!link && this.linkTimer) {
      clearInterval(this.linkTimer);
      this.linkTimer = null;
    }
  }

  private sample() {
    const streaming = this.snap.stream.state === "streaming";
    const t = Date.now() / 1000;
    const kbps = streaming ? (this.snap.stream.info?.bitrate_kbps ?? 0) * (0.96 + 0.06 * Math.sin(t / 2)) : 0;
    if (streaming && Math.random() < 0.04) this.dropped += 1 + Math.floor(Math.random() * 3);
    const sample: LinkSample = {
      time_ms: this.now(),
      rssi: streaming ? Math.round(-4 + 5 * Math.sin(t / 3) + Math.random() * 2) : null,
      afh_in_use: streaming ? 58 : null,
      afh_usable: streaming ? 79 : null,
      afh_avoided: streaming ? "6" : "",
      bytes_per_second: (kbps * 1000) / 8,
      packets_per_second: streaming ? 120 + Math.random() * 6 : 0,
      queue_depth: streaming ? Math.max(0, Math.round(2 + 3 * Math.sin(t) + Math.random() * 2)) : 0,
      dropped: this.dropped,
      capture_dropped: 0,
      airtime: streaming ? 0.52 + 0.05 * Math.sin(t / 2) : 0,
    };
    this.onMessage({ type: "event", rev: this.rev, event: { kind: "link", value: sample } });
  }

  private diagText() {
    const s = this.snap;
    return [
      "A2DP Windows Bridge 2.0.0-alpha.1 (mock core)",
      `Adapter: ${s.adapter.adapters.map((a) => `${a.display_name} [${a.driver_service}]`).join(", ") || "none"}`,
      `State: ${s.stream.state}${s.stream.info ? ` ${s.stream.info.codec} ${s.stream.info.bitrate_kbps} kbps` : ""}`,
      `Profiles: ${s.profiles.length}`,
      `Config: ${s.config_dir}`,
    ].join("\n");
  }
}

/** The mock as configured by the page URL (browser dev mode) */
export function mockFromUrl(): MockCore {
  const q = new URLSearchParams(location.search);
  const scenario = (q.get("mock") as MockScenario | null) ?? "normal";
  const win = q.get("window") === "flyout" ? "flyout" : "main";
  return new MockCore({ scenario, window: win });
}
