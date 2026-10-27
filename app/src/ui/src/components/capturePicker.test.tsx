// The capture source choices of Audio Devices and the profile they save;
// not chosen in place on Home or in the flyout.

import { cleanup, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { MockCore, sampleProfile, type MockOptions } from "../ipc/mock";
import { createCore } from "../ipc/store";
import { captureChoices, captureValue, withCapture } from "./CapturePicker";

let mock: MockCore | null = null;

function start(hash: string, opts: MockOptions = {}) {
  location.hash = hash;
  const calls: { method: string; params: unknown }[] = [];
  mock = new MockCore({ speed: 0, ...opts, onCall: (method, params) => calls.push({ method, params }) });
  const core = createCore(mock);
  render(() => <App core={core} />);
  return { core, calls, mock };
}

afterEach(() => {
  cleanup();
  mock?.dispose();
  mock = null;
});

const DEVICES = [
  { id: "spk", name: "Speakers", is_default: true, is_virtual: false, mix_rate: null, mix_channels: null, mix_bits: null },
  { id: "cable", name: "CABLE Input", is_default: false, is_virtual: true, mix_rate: null, mix_channels: null, mix_bits: null },
];
const APPS = [{ exe: "Spotify.exe", name: "Spotify" }];

describe("choices", () => {
  it("offers virtual devices and playing apps, and keeps the profile's own", () => {
    const p = sampleProfile({ capture_mode: "app", app_exe: "foobar2000.exe", app_name: "foobar2000" });
    const c = captureChoices(p, DEVICES, APPS, true);
    expect(c.virtual.map((x) => x.label)).toEqual(["CABLE Input"]);
    expect(c.apps.map((x) => x.value)).toEqual(["app:foobar2000.exe", "app:spotify.exe"]);
    expect(c.apps[0].label).toBe("foobar2000 (not playing now)");
  });

  it("offers no apps for LE Audio or where Windows cannot capture one", () => {
    expect(captureChoices(sampleProfile({ le_audio: true }), DEVICES, APPS, true).apps).toEqual([]);
    expect(captureChoices(sampleProfile(), DEVICES, APPS, false).apps).toEqual([]);
  });

  it("sets what the capture source needs", () => {
    const p = sampleProfile();
    expect(withCapture(p, "virtual:cable", DEVICES, APPS)).toMatchObject({
      capture_mode: "virtual",
      audio_device_id: "cable",
      audio_device_name: "CABLE Input",
    });
    const a = withCapture(p, "app:spotify.exe", DEVICES, APPS);
    expect(a).toMatchObject({ capture_mode: "app", app_exe: "Spotify.exe", app_name: "Spotify" });
    expect(captureValue(a)).toBe("app:spotify.exe");
    expect(withCapture(a, "loopback", DEVICES, APPS).capture_mode).toBe("loopback");
  });
});

describe("in place", () => {
  it("is not on Home, nor in the flyout: chosen on Audio Devices (issue #86)", async () => {
    start("#/home");
    await screen.findByRole("group", { name: "WH-1000XM5" });
    expect(screen.queryByRole("combobox", { name: "Capture Source" })).toBeNull();
    cleanup();
    mock?.dispose();
    start("", { window: "flyout" });
    await screen.findByRole("button", { name: /Open app/ });
    expect(screen.queryByRole("combobox", { name: "Capture Source" })).toBeNull();
  });
});
