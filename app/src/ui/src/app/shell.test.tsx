// The app shell's v1 leftovers: update check result, update notice, tray
// hide, firmware bar, error copy, capture source, HCI capture sign, the
// setup wizard's adapter and device steps.

import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App, hideToTray, isDiagnosticsShortcut } from "./App";
import { captureRunningText } from "./indicators";
import { errorCopyText, isAppFault, sourceText, statusText } from "../components/stream";
import { setLanguage, t } from "../i18n";
import { MockCore, sampleProfile, type MockOptions } from "../ipc/mock";
import { createCore } from "../ipc/store";
import { LINKS } from "../lib/links";
import { toneParams } from "../lib/profile";

let mock: MockCore | null = null;

function start(hash: string, opts: MockOptions = {}) {
  location.hash = hash;
  const calls: { method: string; params: unknown }[] = [];
  mock = new MockCore({ speed: 0, ...opts, onCall: (method, params) => calls.push({ method, params }) });
  const core = createCore(mock);
  const view = render(() => <App core={core} />);
  return { core, calls, view, mock };
}

afterEach(async () => {
  cleanup();
  mock?.dispose();
  mock = null;
  vi.restoreAllMocks();
  await setLanguage("en");
});

describe("keyboard", () => {
  it("Ctrl+Shift+D opens the diagnostics", async () => {
    start("#/home");
    await screen.findAllByRole("button", { name: "Connect" });
    fireEvent.keyDown(window, { key: "D", code: "KeyD", ctrlKey: true, shiftKey: true });
    expect(await screen.findByRole("heading", { level: 1, name: "Diagnostics" })).toBeInTheDocument();
    expect(isDiagnosticsShortcut({ key: "d", code: "KeyD", ctrlKey: true, shiftKey: false, altKey: false, metaKey: false })).toBe(false);
    expect(isDiagnosticsShortcut({ key: "D", code: "KeyD", ctrlKey: true, shiftKey: true, altKey: true, metaKey: false })).toBe(false);
  });
});

describe("update check", () => {
  it("a failed check shows as failed with the releases page", async () => {
    const { mock } = start("#/settings", { scenario: "offline" });
    const open = vi.spyOn(mock, "openUrl").mockResolvedValue();
    fireEvent.click(await screen.findByRole("button", { name: "Check for Updates" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Could not check for updates.");
    fireEvent.click(within(alert).getByRole("button", { name: t("ui.update.open_releases") }));
    expect(open).toHaveBeenCalledWith(LINKS.releases);
    // Not also as a transient notice
    expect(document.querySelector(".notice-error")).toBeNull();
  });

  it("a newer release shows on every screen, with the way to Settings", async () => {
    start("#/home", { scenario: "update" });
    const notice = await screen.findByText("Version 2.0.0-beta.1 is available (current: 2.0.0-alpha.1).");
    const banner = notice.closest(".banner") as HTMLElement;
    fireEvent.click(within(banner).getByRole("button", { name: t("ui.update.show") }));
    expect(await screen.findByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    // Settings has its own banner with the install button: one, not two
    expect(screen.getAllByText(/Version 2\.0\.0-beta\.1 is available/)).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Update now" })).toBeInTheDocument();
  });

  it("the update notice can be closed", async () => {
    start("#/profiles", { scenario: "update" });
    const banner = (await screen.findByText(/is available \(current/)).closest(".banner") as HTMLElement;
    fireEvent.click(within(banner).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByText(/is available \(current/)).toBeNull());
  });

  it("a crash of A2DPWB shows on every screen, ready to report, until closed", async () => {
    const { mock } = start("#/profiles", { scenario: "crash" });
    const open = vi.spyOn(mock, "openUrl").mockResolvedValue();
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText: write }, configurable: true });
    const banner = (await screen.findByText(t("ui.crash.title"))).closest(".banner") as HTMLElement;
    expect(banner).toHaveTextContent("a2dpwb-core, 2026-10-22 01:23:38");
    expect(banner).toHaveTextContent("panic in thread encode");

    fireEvent.click(within(banner).getByRole("button", { name: t("ui.diag.report_github") }));
    await waitFor(() => expect(open).toHaveBeenCalled());
    const url = new URL(open.mock.calls[0][0]);
    expect(url.searchParams.get("what_happened")).toContain("panic in thread encode");
    // The backtrace first, then the diagnostic info
    const logs = url.searchParams.get("logs")!;
    expect(logs).toContain("a2dpwb_core::encode::run");
    expect(logs.indexOf("a2dpwb_core::encode::run")).toBeLessThan(logs.indexOf("A2DP Windows Bridge"));
    expect(write.mock.calls[0][0]).toContain("```text\n");

    const seen = vi.spyOn(mock, "call");
    fireEvent.click(within(banner).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByText(t("ui.crash.title"))).toBeNull());
    expect(seen.mock.calls.some(([m]) => m === "diag.crashes_seen")).toBe(true);
  });
});

describe("tray", () => {
  it("hiding to the tray tells the core", async () => {
    const { core, calls, mock } = start("#/home");
    await screen.findByRole("heading", { level: 1, name: "Home" });
    const hide = vi.spyOn(mock, "hideWindow");
    await hideToTray(core);
    expect(hide).toHaveBeenCalled();
    expect(calls.some((c) => c.method === "app.hidden_to_tray")).toBe(true);
  });

  it("start in the tray applies only when starting with Windows", async () => {
    const { core } = start("#/settings");
    const toggle = await screen.findByRole("switch", { name: /Start in the tray/ });
    expect(toggle).not.toBeDisabled();
    await core.request("settings.set", { settings: { ...core.state.snap!.settings, start_with_windows: false } });
    await waitFor(() => expect(toggle).toBeDisabled());
  });
});

describe("Home", () => {
  it("shows the firmware bar and leads to the Adapter screen", async () => {
    const { mock } = start("#/home");
    await screen.findByRole("heading", { level: 1, name: "Home" });
    const m = mock as unknown as { snap: { adapter: { adapters: { firmware_present: boolean | null }[] } }; emit: (e: unknown) => void };
    const adapter = { ...m.snap.adapter, adapters: [{ ...m.snap.adapter.adapters[0], firmware_present: false }] };
    m.emit({ kind: "adapter", value: adapter });
    const title = await screen.findByText("Firmware not found");
    const bar = title.closest(".banner") as HTMLElement;
    expect(bar).toHaveTextContent("rtl8761bu_fw.bin, rtl8761bu_config.bin");
    // The Adapter screen's firmware actions are in the bar as well (issue #81)
    expect(within(bar).getByRole("button", { name: t("firmware.download") })).toBeInTheDocument();
    expect(within(bar).getByRole("button", { name: t("firmware.open_folder") })).toBeInTheDocument();
    fireEvent.click(within(bar).getByRole("button", { name: t("ui.home.firmware_fix") }));
    expect(await screen.findByRole("heading", { level: 1, name: "Adapters" })).toBeInTheDocument();
    expect(location.hash).toBe("#/adapter/firmware");
    expect(document.getElementById("adapter-firmware")).toBeInTheDocument();
  });

  it("copies the error with the diagnostic log", async () => {
    start("#/home", { scenario: "error" });
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText: write }, configurable: true });
    const alert = await screen.findByRole("alert");
    fireEvent.click(within(alert).getByRole("button", { name: t("ui.error.copy") }));
    await waitFor(() => expect(write).toHaveBeenCalled());
    const text = write.mock.calls[0][0] as string;
    expect(text.startsWith(`${t("error.connect_no_answer")}\n\n`)).toBe(true);
    expect(text).toContain("A2DP Windows Bridge");
    expect(await screen.findByText(t("ui.error.copied"))).toBeInTheDocument();
  });

  it("offers no bug report for a cause on the user's side", async () => {
    start("#/home", { scenario: "error" });
    const alert = await screen.findByRole("alert");
    // The device did not answer: off or out of range, the fix is the answer
    expect(within(alert).getByRole("button", { name: t("ui.error.copy") })).toBeInTheDocument();
    expect(within(alert).queryByRole("button", { name: t("ui.diag.report_github") })).toBeNull();
    expect(isAppFault("error.connect_no_answer")).toBe(false);
    expect(isAppFault("error.encoder_init", "stream.bad_codec")).toBe(false);
    expect(isAppFault("error.encoder_init")).toBe(true);
    // Not known to be the user's: A2DPWB's own
    expect(isAppFault("error.something_new")).toBe(true);
  });

  it("reports an error of A2DPWB itself on GitHub with the form filled in", async () => {
    const { mock } = start("#/home", { scenario: "error" });
    const open = vi.spyOn(mock, "openUrl").mockResolvedValue();
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText: write }, configurable: true });
    await screen.findByRole("alert");
    const m = mock as unknown as { snap: { stream: object }; emit: (e: unknown) => void };
    m.emit({ kind: "stream", value: { ...m.snap.stream, message: { key: "error.encoder_init", args: [] } } });
    const report = await screen.findByRole("button", { name: t("ui.diag.report_github") });
    fireEvent.click(report);
    await waitFor(() => expect(open).toHaveBeenCalled());
    const url = new URL(open.mock.calls[0][0]);
    expect(url.searchParams.get("template")).toBe("bug_report.yml");
    expect(url.searchParams.get("what_happened")).toContain(t("error.encoder_init"));
    expect(url.searchParams.get("logs")).toContain("A2DP Windows Bridge");
    // No bundle was saved: its field stays empty
    expect(url.searchParams.has("bundle")).toBe(false);
    expect(write).toHaveBeenCalled();
    expect(await screen.findByText(t("ui.error.report_opened"))).toBeInTheDocument();
  });

  it("shows the capture source, waiting for the application", async () => {
    const { mock } = start("#/home");
    await screen.findByRole("heading", { level: 1, name: "Home" });
    await mock.call("stream.start", { profile: "Desk speaker (AAC)" });
    await mock.call("profile.set_active", { name: "Desk speaker (AAC)" });
    expect(await screen.findByText(t("ui.source.waiting", "Spotify"))).toBeInTheDocument();
  });

  it("shows a running HCI capture app-wide", async () => {
    const { mock } = start("#/home");
    await screen.findByRole("heading", { level: 1, name: "Home" });
    await mock.call("diag.capture_start");
    const sign = await screen.findByRole("button", { name: "HCI capture running" });
    fireEvent.click(sign);
    expect(await screen.findByRole("heading", { level: 1, name: "Diagnostics" })).toBeInTheDocument();
    await mock.call("diag.capture_stop");
    await waitFor(() => expect(screen.queryByRole("button", { name: "HCI capture running" })).toBeNull());
  });
});

describe("flyout", () => {
  it("shows the capture sign (the capture source is chosen in the main window)", async () => {
    const { mock } = start("", { window: "flyout" });
    await screen.findByRole("button", { name: "Connect" });
    expect(screen.queryByRole("button", { name: "HCI capture running" })).toBeNull();
    await mock.call("diag.capture_start");
    expect(await screen.findByRole("button", { name: "HCI capture running" })).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Capture Source" })).toBeNull();
  });
});

describe("setup device step", () => {
  async function toDeviceStep(opts: MockOptions = {}) {
    const r = start("#/setup", opts);
    await screen.findByRole("heading", { level: 1, name: "Set up A2DPWB" });
    for (let i = 0; i < 2; i++) {
      const next = screen.getByRole("button", { name: "Next" });
      await waitFor(() => expect(next).not.toBeDisabled());
      fireEvent.click(next);
    }
    await screen.findByRole("heading", { level: 2, name: "Find your audio devices" });
    return r;
  }

  it("three steps, the last one ends with Finish", async () => {
    await toDeviceStep();
    expect(screen.getAllByRole("listitem").map((li) => li.textContent?.replace(/^\d/, ""))).toEqual([
      "Choose the adapter",
      "Driver and firmware",
      "Find your audio devices",
    ]);
    expect(screen.queryByRole("button", { name: "Next" })).toBeNull();
    expect(screen.getByRole("button", { name: "Finish" })).toBeInTheDocument();
    // No profile name to type: Connect makes it
    expect(screen.queryByRole("textbox", { name: "Profile Name" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Create profile" })).toBeNull();
  });

  it("scans by itself; Connect makes the Auto profile and connects with it", async () => {
    const { calls, core } = await toDeviceStep({ scenario: "normal" });
    await waitFor(() => expect(calls.some((c) => c.method === "device.scan")).toBe(true));
    const name = await screen.findByText("Galaxy Buds2 Pro");
    const row = name.closest(".device-row") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(calls.some((c) => c.method === "stream.start")).toBe(true));
    expect(calls.find((c) => c.method === "stream.start")!.params).toEqual({ profile: "Galaxy Buds2 Pro | Auto" });
    const saved = core.state.snap!.profiles.find((p) => p.name === "Galaxy Buds2 Pro | Auto")!;
    expect(saved).toMatchObject({ device_address: "F4:4E:FD:00:11:22", codec: "auto" });
    expect(core.state.snap!.active_profile).toBe("Galaxy Buds2 Pro | Auto");
    expect(core.state.snap!.devices.find((d) => d.address === "F4:4E:FD:00:11:22")?.saved).toBe(true);
    expect(screen.getByRole("button", { name: "Finish" })).not.toBeDisabled();
  });

  it("a device with a profile connects with that one", async () => {
    const { calls, core } = await toDeviceStep();
    const before = core.state.snap!.profiles.length;
    const row = (await screen.findAllByText("WH-1000XM5")).map((e) => e.closest(".device-row")).find((r) => r) as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(calls.some((c) => c.method === "stream.start")).toBe(true));
    expect(calls.find((c) => c.method === "stream.start")!.params).toEqual({ profile: "WH-1000XM5 (LDAC)" });
    expect(core.state.snap!.profiles.length).toBe(before);
  });

  it("an address typed by hand connects with Use", async () => {
    const { calls } = await toDeviceStep();
    fireEvent.input(screen.getByRole("textbox", { name: "Manual:" }), { target: { value: "11:22:33:44:55:66" } });
    fireEvent.click(screen.getByRole("button", { name: "Use" }));
    await waitFor(() => expect(calls.some((c) => c.method === "stream.start")).toBe(true));
    expect(calls.find((c) => c.method === "stream.start")!.params).toEqual({ profile: "11:22:33:44:55:66 | Auto" });
  });
});

describe("setup adapter step", () => {
  it("marks the adapters A2DPWB supports, and says so in red for one it does not", async () => {
    start("#/setup", { scenario: "fresh" });
    await screen.findByRole("heading", { level: 1, name: "Set up A2DPWB" });
    const radios = screen.getAllByRole("radio");
    // The supported one first, and chosen
    const rtl = radios[0].closest(".choice") as HTMLElement;
    const mtk = radios[1].closest(".choice") as HTMLElement;
    expect(rtl).toHaveTextContent("Realtek RTL8761BU");
    expect(within(rtl).getByText("Supported")).toHaveClass("st-good");
    expect(radios[0]).toBeChecked();
    expect(mtk).toHaveTextContent("MediaTek");
    expect(mtk).not.toHaveTextContent("[unsupported]");
    expect(within(mtk).getByText("Not supported")).toHaveClass("st-bad");
    fireEvent.click(radios[1]);
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(await screen.findByText(t("ui.setup.unsupported_note"))).toBeInTheDocument();
  });
});

describe("helpers", () => {
  it("source text", () => {
    const p = sampleProfile({ app_exe: "Spotify.exe", app_name: "Spotify" });
    expect(sourceText({ kind: "tone", name: "", waiting: false }, null).text).toBe("Test tone");
    expect(sourceText({ kind: "loopback", name: "", waiting: false }, null).text).toBe("System Loopback");
    expect(sourceText({ kind: "virtual", name: "CABLE Input", waiting: false }, null).text).toBe("CABLE Input");
    expect(sourceText({ kind: "app", name: "spotify.exe", waiting: false }, p).text).toBe("Spotify");
    const waiting = sourceText({ kind: "app", name: "foo.exe", waiting: true }, p);
    expect(waiting).toMatchObject({ waiting: true, text: t("ui.source.waiting", "foo.exe") });
  });

  it("a reconnect reads as connecting, and its end as connected (issue #86)", () => {
    const msg = (key: string, ...args: string[]) => ({ key, args });
    expect(statusText({ state: "reconnecting", message: msg("status.reconnect_attempt", "1", "3") })).toBe("Connecting...");
    expect(statusText({ state: "streaming", message: msg("status.reconnected") })).toBe("Connected");
    expect(statusText({ state: "connecting", message: msg("status.initializing_btstack") })).toBe("Initializing BTstack...");
    expect(statusText({ state: "error", message: msg("error.connect_no_answer") })).toBe("Error");
    expect(statusText({ state: "idle", message: msg("") })).toBe("Idle");
  });

  it("error copy text", () => {
    expect(errorCopyText({ key: "error.connect_no_answer", args: [] })).toBe(t("error.connect_no_answer"));
    expect(errorCopyText({ key: "error.connect_no_answer", args: [] }, "stream.failed")).toMatch(/\n\[stream\.failed\]$/);
  });

  it("capture sign text from v1's title suffix", async () => {
    expect(captureRunningText()).toBe("HCI capture running");
    await setLanguage("ja");
    expect(captureRunningText()).toBe("HCI キャプチャ中");
  });

  it("tone parameters from the profile", () => {
    const p = sampleProfile({ sample_rate: 0, bit_depth: 24, codec: "aac", max_media_payload: 0 });
    expect(toneParams(p)).toEqual({
      device: "AC:80:0A:12:34:56",
      device_name: "WH-1000XM5",
      codec: "aac",
      quality: "hq",
      sample_rate: null,
      bit_depth: 24,
      source: "tone",
      max_media_payload: null,
      abr: false,
      codec_fallback: true,
    });
  });
});
