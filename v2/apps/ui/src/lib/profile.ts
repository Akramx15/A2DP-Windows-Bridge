// A new profile's defaults and the checks before profile.save. The values
// are those of v1's profile dialog; the core checks again.

import type { Profile } from "../ipc/types";
import { PACKET_DEFAULT, PACKET_MAX, PACKET_MIN } from "./codecs";
import { parseAddress } from "./format";

export function newProfile(): Profile {
  return {
    name: "",
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
    auto_switch_device: false,
    app_exe: "",
    app_name: "",
    volume_sync: "auto",
    volume_device_id: "",
    volume_device_name: "",
    max_media_payload: PACKET_DEFAULT,
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
  return out;
}

/** The profile as it is saved: trimmed name, normalised address */
export function normalizeProfile(p: Profile): Profile {
  return { ...p, name: p.name.trim(), device_address: parseAddress(p.device_address) ?? p.device_address.trim() };
}
