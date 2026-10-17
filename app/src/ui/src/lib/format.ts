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

const ADDRESS = /\b([0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}):[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}\b/g;

/** "AC:80:0A:12:34:56" -> "AC:80:0A:xx:xx:xx" (the vendor part stays), as v1's debug console */
export function maskAddresses(text: string): string {
  return text.replace(ADDRESS, "$1:xx:xx:xx");
}

/**
 * A Bluetooth address as typed: 12 hex digits, optionally in pairs
 * separated by colons, hyphens or spaces (v1 accepted the same). Returns
 * "AA:BB:CC:DD:EE:FF" or null.
 */
export function parseAddress(input: string): string | null {
  const s = input.trim();
  if (!/^[0-9A-Fa-f]{2}([:\- ]?[0-9A-Fa-f]{2}){5}$/.test(s)) return null;
  const hex = s.replace(/[:\- ]/g, "").toUpperCase();
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
