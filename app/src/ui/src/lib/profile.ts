// A new profile's defaults, the checks before profile.save, and the plain
// words a profile card shows (what it is for). The values are those of v1's
// profile dialog; the core checks again.

import { captureText } from "../components/CapturePicker";
import type { AudioDevice, Device, ParamsOf, Profile, SpatialEngine } from "../ipc/types";
import { codecSpec, PACKET_DEFAULT, PACKET_MAX, PACKET_MIN, qualityTag } from "./codecs";
import { parseAddress } from "./format";

export function newProfile(): Profile {
  return {
    name: "",
    purpose_kind: "",
    purpose_note: "",
    device_address: "",
    device_name: "",
    codec: "auto",
    quality: "hq",
    abr: false,
    codec_fallback: true,
    sample_rate: 0,
    bit_depth: 0,
    capture_mode: "loopback",
    audio_device_id: "",
    audio_device_name: "",
    // As v1 and the core: follow the virtual device as the default output
    auto_switch_device: true,
    app_exe: "",
    app_name: "",
    volume_sync: "auto",
    volume_device_id: "",
    volume_device_name: "",
    spatial_engine: false,
    spatial_engine_path: "",
    spatial_engine_registry: "",
    max_media_payload: PACKET_DEFAULT,
    le_audio: false,
    le_low_latency: true,
    sbc_bitpool: 0,
    sbcxq_bitpool: 0,
    sbc_blocks: 0,
    sbc_subbands: 0,
    sbc_allocation: "",
    sbc_channel_mode: "",
  };
}

/** A problem with a field: translation key and its arguments */
export interface Problem {
  key: string;
  args?: (string | number)[];
}

export type ProfileProblems = Partial<Record<"name" | "device" | "packet" | "audio_device" | "app", Problem>>;

/**
 * `taken`: the names of the other profiles (a rename must not overwrite
 * one of them)
 */
export function validateProfile(p: Profile, taken: readonly string[]): ProfileProblems {
  const out: ProfileProblems = {};
  const name = p.name.trim();
  if (!name) out.name = { key: "ui.profile.err_name_empty" };
  else if (taken.includes(name)) out.name = { key: "ui.profile.err_name_taken" };
  if (!p.device_address.trim()) out.device = { key: "ui.profile.err_device" };
  else if (!parseAddress(p.device_address)) out.device = { key: "error.invalid_address_format" };
  if (!Number.isInteger(p.max_media_payload) || p.max_media_payload < PACKET_MIN || p.max_media_payload > PACKET_MAX) {
    out.packet = { key: "ui.profile.err_packet", args: [PACKET_MIN, PACKET_MAX] };
  }
  if (p.capture_mode === "virtual" && !p.audio_device_id) out.audio_device = { key: "error.no_virtual_device_selected" };
  if (p.capture_mode === "app" && !p.app_exe) out.app = { key: "error.no_app_selected" };
  // LE Audio streams loopback and virtual device capture only (the core
  // refuses the stream otherwise)
  if (p.le_audio && p.capture_mode === "app") out.app = { key: "error.le_audio_capture" };
  return out;
}

/** The profile as it is saved: trimmed name, normalised address */
export function normalizeProfile(p: Profile): Profile {
  return { ...p, name: p.name.trim(), device_address: parseAddress(p.device_address) ?? p.device_address.trim() };
}

/** The codec keys a device offered when it last connected (empty: not known) */
export function knownCodecs(devices: readonly Device[], address: string): string[] {
  const a = parseAddress(address);
  if (!a) return [];
  return devices.find((d) => !d.le_audio && d.address.toUpperCase() === a)?.codecs ?? [];
}

/**
 * A device entry's identity: a dual-mode device has a classic entry and an
 * LE Audio one with the same address
 */
export function deviceKey(d: Pick<Device, "address" | "le_audio">): string {
  return `${d.address.toUpperCase()}${d.le_audio ? "/le" : ""}`;
}

/** The entries to offer: LE Audio ones only with LE Audio (experimental) on, as v1 */
export function listedDevices(devices: readonly Device[], leAudio: boolean): Device[] {
  return leAudio ? [...devices] : devices.filter((d) => !d.le_audio);
}

/** A profile's name until the user types one: the device, "<device> LE Audio" for LE Audio (v1) */
export function defaultProfileName(deviceName: string, address: string, leAudio: boolean, codec = ""): string {
  const base = deviceName || address;
  if (leAudio) return base ? `${base} LE Audio` : base;
  return base && codec ? `${base} | ${codec}` : base;
}

/**
 * The render device a capture mode records: the default output device
 * (loopback), the chosen virtual device, or none (application capture:
 * Windows converts to the rate asked for)
 */
export function captureDevice(
  p: Pick<Profile, "capture_mode" | "audio_device_id">,
  devices: readonly AudioDevice[],
): AudioDevice | null {
  if (p.capture_mode === "loopback") return devices.find((d) => d.is_default) ?? null;
  if (p.capture_mode === "virtual") return devices.find((d) => d.id === p.audio_device_id) ?? null;
  return null;
}

/**
 * Which of the profile's sample rate and bit depth differ from the capture
 * device's format (v1 update_format_info; Auto never differs)
 */
export function formatMismatch(
  p: Pick<Profile, "sample_rate" | "bit_depth">,
  d: Pick<AudioDevice, "mix_rate" | "mix_bits">,
): { rate: boolean; bits: boolean } {
  return {
    rate: p.sample_rate !== 0 && d.mix_rate !== null && p.sample_rate !== d.mix_rate,
    bits: p.bit_depth !== 0 && d.mix_bits !== null && p.bit_depth !== d.mix_bits,
  };
}

/** A device name the core saves: not empty, on one line, without "|" */
export function validDeviceName(name: string): boolean {
  const n = name.trim();
  return n.length > 0 && !/[\r\n|]/.test(n);
}

/** Whether this Windows can capture one application (build 19041 or later) */
export type AppCaptureSupport = "yes" | "no" | "unknown";

/**
 * From User-Agent Client Hints: on Windows the major of platformVersion is
 * the UniversalApiContract version, 10 from Windows 10 version 2004 (build
 * 19041) and 13+ on Windows 11; 1-8 are the earlier Windows 10 versions.
 * Unknown elsewhere or without the hints (the core still refuses the
 * stream with error.app_capture_unsupported).
 */
export function appCaptureSupportFrom(platform: string | undefined, platformVersion: string | undefined): AppCaptureSupport {
  if (platform !== "Windows" || !platformVersion) return "unknown";
  const major = Number.parseInt(platformVersion, 10);
  if (!Number.isFinite(major)) return "unknown";
  return major >= 10 ? "yes" : "no";
}

interface UaData {
  getHighEntropyValues(hints: string[]): Promise<{ platform?: string; platformVersion?: string }>;
}

export async function appCaptureSupport(): Promise<AppCaptureSupport> {
  const ua = (globalThis.navigator as { userAgentData?: UaData } | undefined)?.userAgentData;
  if (!ua?.getHighEntropyValues) return "unknown";
  try {
    const v = await ua.getHighEntropyValues(["platformVersion"]);
    return appCaptureSupportFrom(v.platform, v.platformVersion);
  } catch {
    return "unknown";
  }
}

// ---- The words a profile shows ------------------------------------------
//
// The card, the switcher and the flyout say what a profile is for: the
// device, the codec and the capture source in one line, the kind of use
// ("headphones", "AutoEQ", ...) and the user's own note about it.

/** The kinds of use a profile can have, and the key of each label */
const PURPOSE_KINDS: Record<string, string> = {
  headphones: "profile.purpose_headphones",
  autoeq: "profile.purpose_autoeq",
  speaker: "profile.purpose_speaker",
  custom: "profile.purpose_custom",
};

/** What a profile is for, as the card's pill says it (empty: not set,
 *  an unknown kind: as it is) */
export function purposeLabel(kind: string, t: (key: string) => string): string {
  const key = PURPOSE_KINDS[kind];
  return key ? t(key) : kind;
}

/** The profile in one line of plain words: the device, the codec with its
 *  quality and the capture source ("WH-1000XM6 · LDAC High (990 kbps) ·
 *  System Loopback") */
export function profileSummary(p: Profile, t: (key: string) => string): string {
  const parts: string[] = [];
  const device = p.device_name || p.device_address;
  if (device) parts.push(device);
  const codec = p.le_audio ? t("ui.codec.lc3") : t(codecSpec(p.codec).label);
  const quality = qualityTag(p, t);
  parts.push(quality ? `${codec} ${quality}` : codec);
  parts.push(captureText(p).text);
  return parts.join(" · ");
}

/** The label of the engine the profile runs (empty: none, or not among
 *  the ones the core lists) */
export function engineLabelOf(p: Profile, engines: readonly SpatialEngine[]): string {
  return engines.find((e) => e.path === p.spatial_engine_path)?.label ?? "";
}

/** stream.start_custom for the core's test tone (left 1 kHz, right
 *  1.5 kHz, as v1's) to a profile's headphones, with its sound settings */
export function toneParams(p: Profile): ParamsOf<"stream.start_custom"> {
  return {
    device: p.device_address,
    device_name: p.device_name || null,
    codec: p.codec,
    quality: p.quality,
    sample_rate: p.sample_rate || null,
    bit_depth: p.bit_depth || null,
    source: "tone",
    max_media_payload: p.max_media_payload || null,
    abr: p.abr,
    codec_fallback: p.codec_fallback,
  };
}
