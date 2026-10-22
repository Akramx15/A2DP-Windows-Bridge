// Small display helpers

/** AVRCP absolute volume (0..127) as a percentage */
export const volumePercent = (v: number) => Math.round((v * 100) / 127);
export const percentToVolume = (p: number) => Math.round((p * 127) / 100);

export const kbps = (bytesPerSecond: number) => (bytesPerSecond * 8) / 1000;

export function khz(hz: number): string {
  const k = hz / 1000;
  return Number.isInteger(k) ? String(k) : k.toFixed(1);
}

/** Core uptime in ms as h:mm:ss.mmm */
export function clock(ms: number): string {
  const h = Math.floor(ms / 3_600_000);
  const m = Math.floor(ms / 60_000) % 60;
  const s = Math.floor(ms / 1000) % 60;
  const f = Math.floor(ms % 1000);
  return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}.${String(f).padStart(3, "0")}`;
}

const H = "[0-9A-Fa-f]";
/**
 * "AC:80:0A:12:34:56" or with '-'; not in a longer run (IPv6, "...:55:66"), a
 * letter before or a colon after ("AC:80:0A:12:34:56: pairing") is fine
 */
const SEPARATED = [":", "\\-"].map(
  (s) => new RegExp(`(?<!${s})(${H}{2}${s}${H}{2}${s}${H}{2}${s})${H}{2}${s}${H}{2}${s}${H}{2}(?!${H}|${s}${H}{2})`, "g"),
);
/** A word of 12 hex digits, not a GUID's last part */
const BARE = /(?<![A-Za-z0-9-])([0-9A-Fa-f]{6})[0-9A-Fa-f]{6}(?![A-Za-z0-9])/g;
const SERIAL_END = `\\\\#\\s"',;:.()[\\]{}\\u0080-\\uffff`;
/** USB\VID_0BDA&PID_8771\<serial>, a device path's #<serial># */
const USB_INSTANCE = new RegExp(`(VID_${H}{4}&PID_${H}{4}[A-Z0-9_&]*[\\\\#]+)([^${SERIAL_END}]*)`, "gi");
/** A driver record's name, USB_VID_0BDA_PID_8771_<serial> */
const USB_RECORD = new RegExp(`(VID_${H}{4}_PID_${H}{4}_)([^${SERIAL_END}]*)`, "gi");

/**
 * Masks what identifies the user's devices, as the core's mask_identifiers:
 * Bluetooth addresses "AC:80:0A:12:34:56" (or with '-') -> "AC:80:0A:xx:xx:xx"
 * (the vendor part stays, as v1's debug console), the same without
 * separators "AC800A123456" -> "AC800Axxxxxx" (Windows' Bluetooth device IDs,
 * USB serials that are the adapter's address), other USB serial numbers ->
 * x's. IDs Windows generates ("5&2C7B4E3&0&2") stay.
 */
export function maskIdentifiers(text: string): string {
  const serial = (generatedStays: boolean) => (_: string, head: string, s: string) =>
    head + ((generatedStays && s.includes("&")) || /^[0-9A-Fa-f]{12}$/.test(s) ? s : "x".repeat(s.length));
  let out = text.replace(USB_INSTANCE, serial(true)).replace(USB_RECORD, serial(false));
  for (const re of SEPARATED) out = out.replace(re, (_, head: string) => `${head}xx${head[2]}xx${head[2]}xx`);
  return out.replace(BARE, "$1xxxxxx");
}

/**
 * Full-width characters an IME types ("ＡＣ：８０") as their ASCII forms,
 * as v1 (ascii_from_fullwidth), plus the dashes a Japanese IME gives for
 * "-" ("ー", "−")
 */
export function asciiFromFullwidth(s: string): string {
  return s.replace(/[\uFF01-\uFF5E\u3000\u30FC\u2010-\u2015\u2212]/g, (c) => {
    const v = c.charCodeAt(0);
    if (v >= 0xff01 && v <= 0xff5e) return String.fromCharCode(v - 0xfee0);
    return v === 0x3000 ? " " : "-";
  });
}

/**
 * A Bluetooth address as typed: 12 hex digits, optionally in pairs
 * separated by colons, hyphens, dots or spaces, also full-width (v1
 * accepted the same). Returns "AA:BB:CC:DD:EE:FF" or null.
 */
export function parseAddress(input: string): string | null {
  const s = asciiFromFullwidth(input).trim();
  if (!/^[0-9A-Fa-f]{2}([:\-. ]?[0-9A-Fa-f]{2}){5}$/.test(s)) return null;
  const hex = s.replace(/[:\-. ]/g, "").toUpperCase();
  return (hex.match(/.{2}/g) ?? []).join(":");
}

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // No clipboard permission: the selection way
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  }
}
