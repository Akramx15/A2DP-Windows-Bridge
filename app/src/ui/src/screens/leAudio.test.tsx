// LE Audio (experimental) in the UI: no switches in Settings, the profile's
// "LE Audio device" check box and its GMAP low latency, the LE Audio entries of the device
// list, and the ISO counters of the diagnostics

import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { setLanguage, t } from "../i18n";
import { MockCore, type MockScenario } from "../ipc/mock";
import { createCore } from "../ipc/store";

let mock: MockCore | null = null;

function start(hash: string, scenario: MockScenario = "normal", speed = 0) {
  location.hash = hash;
  const calls: { method: string; params: unknown }[] = [];
  mock = new MockCore({ scenario, speed, onCall: (method, params) => calls.push({ method, params }) });
  const core = createCore(mock);
  const view = render(() => <App core={core} />);
  return { calls, core, view };
}

const named = (label: string) => (n: string) => n.startsWith(label);
const gmapCheck = () => screen.queryByRole("checkbox", { name: t("settings.le_low_latency") });
const leCheck = () => screen.queryByRole("checkbox", { name: t("profile.le_audio") });
/** A new profile's classic device: the codecs (LE Audio with them) wait for one */
const pickClassic = async () => fireEvent.click(await screen.findByRole("radio", { name: /WH-1000XM5/ }));

afterEach(async () => {
  cleanup();
  mock?.dispose();
  mock = null;
  await setLanguage("en");
});

describe("settings", () => {
  it("has no LE Audio or GMAP switch: both are chosen in the profile", async () => {
    start("#/settings");
    await screen.findByRole("switch", { name: named(t("ui.settings.developer_mode")) });
    expect(screen.queryByRole("switch", { name: named(t("settings.le_audio")) })).toBeNull();
    expect(screen.queryByRole("switch", { name: named(t("settings.le_low_latency")) })).toBeNull();
  });
});

describe("profile editor", () => {
  it("offers the LE Audio device check box only with the setting on", async () => {
    start("#/profiles/new");
    await pickClassic();
    await screen.findByRole("radio", { name: new RegExp(t("codec.ldac")) });
    expect(leCheck()).toBeNull();
    cleanup();
    mock?.dispose();

    start("#/profiles/new", "le");
    await screen.findByRole("radio", { name: /WH-1000XM5/ });
    // With the codecs: none before a device is chosen
    expect(leCheck()).toBeNull();
    await pickClassic();
    await screen.findByRole("radio", { name: new RegExp(t("codec.ldac")) });
    expect(leCheck()).not.toBeNull();
  });

  it("offers LC3 alone, without settings, for an LE Audio profile", async () => {
    start("#/profiles/new", "le");
    await pickClassic();
    expect(await screen.findByRole("radiogroup", { name: t("connection.codec") })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: t("ui.profile.codec_settings", t("codec.ldac")) })).toBeInTheDocument();

    fireEvent.click(leCheck()!);
    await waitFor(() => expect(screen.getByRole("radio", { name: (n) => n.startsWith(t("ui.codec.lc3")) })).toBeChecked());
    expect(screen.queryByRole("radio", { name: new RegExp(t("codec.ldac")) })).toBeNull();
    expect(screen.queryByRole("button", { name: t("ui.profile.codec_settings", t("codec.ldac")) })).toBeNull();
    expect(screen.queryByRole("checkbox", { name: t("connection.codec_fallback") })).toBeNull();
    expect(screen.getByText(t("tooltip.le_audio"))).toBeInTheDocument();
  });

  it("offers no LE Audio when the adapter cannot stream it (issue #86)", async () => {
    start("#/profiles/new", "le_unsupported");
    expect(await screen.findByRole("radio", { name: /SRS-XB13/ })).toBeInTheDocument();
    // ... nor the LE Audio entries of the device list
    expect(screen.queryByText(t("device.tag_le_audio_set"))).toBeNull();
    await pickClassic();
    await screen.findByRole("radio", { name: new RegExp(t("codec.ldac")) });
    expect(leCheck()).toBeNull();
  });

  it("counts an adapter not checked yet as unsupported, and says so", async () => {
    start("#/profiles/new", "le_unchecked");
    await pickClassic();
    await screen.findByRole("radio", { name: new RegExp(t("codec.ldac")) });
    expect(leCheck()).toBeNull();
    expect(screen.getByText(t("ui.profile.le_audio_unchecked"))).toBeInTheDocument();
  });

  it("keeps the check box of an LE Audio profile and says the adapter cannot stream it", async () => {
    start(`#/profiles/edit/${encodeURIComponent("Buds (LE Audio)")}`, "le_unsupported");
    await waitFor(() => expect(leCheck()).toBeChecked());
    expect(screen.getByText(t("ui.profile.le_audio_unsupported"))).toBeInTheDocument();
  });

  it("names the profile after the device with LE Audio and saves it as one", async () => {
    const { calls } = start("#/profiles/new", "le");
    // The saved LE Audio entry of the earbuds: picking it ticks the check box
    const row = await screen.findByText(t("device.tag_le_audio_set"));
    fireEvent.click(within(row.closest(".device-row") as HTMLElement).getByRole("radio"));
    await waitFor(() => expect(leCheck()).toBeChecked());
    expect(screen.getByLabelText("Profile Name")).toHaveValue("Galaxy Buds2 Pro LE Audio");

    // Untick: the classic name; tick again
    fireEvent.click(leCheck()!);
    await waitFor(() => expect(screen.getByLabelText("Profile Name")).toHaveValue("Galaxy Buds2 Pro"));
    fireEvent.click(leCheck()!);
    await waitFor(() => expect(screen.getByLabelText("Profile Name")).toHaveValue("Galaxy Buds2 Pro LE Audio"));

    fireEvent.click(screen.getByRole("button", { name: t("modal.save") }));
    await waitFor(() => expect(calls.some((c) => c.method === "profile.save")).toBe(true));
    const saved = (calls.find((c) => c.method === "profile.save")!.params as { profile: Record<string, unknown> }).profile;
    expect(saved).toMatchObject({
      name: "Galaxy Buds2 Pro LE Audio",
      device_address: "F4:4E:FD:00:11:22",
      le_audio: true,
      le_low_latency: true,
    });
  });

  it("offers GMAP low latency for an LE Audio profile, on by default, saved with it", async () => {
    const { calls } = start("#/profiles/new", "le");
    await screen.findByText(t("device.tag_le_audio_set"));
    expect(gmapCheck()).toBeNull();
    const row = await screen.findByText(t("device.tag_le_audio_set"));
    fireEvent.click(within(row.closest(".device-row") as HTMLElement).getByRole("radio"));
    await waitFor(() => expect(gmapCheck()).toBeChecked());
    fireEvent.click(gmapCheck()!);
    await waitFor(() => expect(gmapCheck()).not.toBeChecked());
    fireEvent.click(screen.getByRole("button", { name: t("modal.save") }));
    await waitFor(() => expect(calls.some((c) => c.method === "profile.save")).toBe(true));
    const saved = (calls.find((c) => c.method === "profile.save")!.params as { profile: Record<string, unknown> }).profile;
    expect(saved).toMatchObject({ le_audio: true, le_low_latency: false });
  });

  it("refuses application capture for LE Audio", async () => {
    const { calls } = start("#/profiles/new", "le");
    fireEvent.click(await screen.findByRole("radio", { name: /WH-1000XM5/ }));
    fireEvent.click(leCheck()!);
    fireEvent.click(screen.getByRole("radio", { name: t("capture.app") }));
    expect(await screen.findByText(t("error.le_audio_capture"))).toBeInTheDocument();
    fireEvent.change(await screen.findByLabelText(t("capture.app_select")), { target: { value: "Spotify.exe" } });
    fireEvent.click(screen.getByRole("button", { name: t("modal.save") }));
    expect(await screen.findByText(t("ui.profile.fix_problems"))).toBeInTheDocument();
    expect(screen.getAllByText(t("error.le_audio_capture")).length).toBe(2);
    expect(calls.some((c) => c.method === "profile.save")).toBe(false);
  });

  it("shows the LE Audio profile with LC3 in the list", async () => {
    start("#/profiles", "le");
    const card = (await screen.findByRole("heading", { name: "Buds (LE Audio)" })).closest("li")!;
    expect(card).toHaveTextContent(t("ui.codec.lc3"));
  });
});

describe("device list", () => {
  it("lists a dual-mode device twice with LE Audio on, with its tags", async () => {
    start("#/devices", "le");
    fireEvent.click(await screen.findByRole("button", { name: t("connection.scan") }));
    await screen.findByText(/LE Audio Speaker/);
    const rows = screen.getAllByText(/Galaxy Buds2 Pro/).map((r) => r.closest(".device-row") as HTMLElement);
    expect(rows).toHaveLength(2);
    expect(rows.map((r) => r.dataset.key).sort()).toEqual(["F4:4E:FD:00:11:22", "F4:4E:FD:00:11:22/le"]);
    const le = rows.find((r) => r.dataset.key!.endsWith("/le"))!;
    const classic = rows.find((r) => !r.dataset.key!.endsWith("/le"))!;
    expect(le).toHaveTextContent(t("device.tag_le_audio_set"));
    expect(classic).toHaveTextContent(t("device.tag_audio"));
    expect(classic).not.toHaveTextContent(t("device.tag_le_audio"));
    const speaker = screen.getByText(/LE Audio Speaker/).closest(".device-row")!;
    expect(speaker).toHaveTextContent(t("device.tag_le_audio"));
    expect(speaker).not.toHaveTextContent(t("device.tag_le_audio_set"));
  });

  it("picks one entry of a dual-mode device in the profile editor", async () => {
    start("#/profiles/new", "le");
    // The saved LE Audio entry (the classic one is not saved here)
    const buds = await screen.findAllByRole("radio", { name: /Galaxy Buds2 Pro/ });
    expect(buds).toHaveLength(1);
    expect((buds[0].closest(".device-row") as HTMLElement).dataset.key).toBe("F4:4E:FD:00:11:22/le");
    fireEvent.click(buds[0]);
    await waitFor(() => expect(buds[0]).toBeChecked());
    expect(leCheck()).toBeChecked();
    // No scanning, renaming or removing here: that is the Audio Devices screen
    expect(screen.queryByRole("button", { name: t("connection.scan") })).toBeNull();
    expect(screen.queryByRole("button", { name: /Galaxy Buds2 Pro/ })).toBeNull();
  });

  it("hides the LE Audio entries with LE Audio off", async () => {
    start("#/devices");
    fireEvent.click(await screen.findByRole("button", { name: t("connection.scan") }));
    await screen.findByText(/MX Keys/);
    expect(screen.getAllByText(/Galaxy Buds2 Pro/)).toHaveLength(1);
    expect(screen.queryByText(/LE Audio Speaker/)).toBeNull();
    expect(screen.queryByText(t("device.tag_le_audio"))).toBeNull();
  });

  it("says when it scans for LE Audio devices", async () => {
    // Slower than the other tests: the LE part of the scan lasts long enough to see
    start("#/devices", "le", 0.2);
    fireEvent.click(await screen.findByRole("button", { name: t("connection.scan") }));
    expect(await screen.findByText(t("status.le_scanning"), {}, { timeout: 3000 })).toBeInTheDocument();
    await screen.findByText(/LE Audio Speaker/, {}, { timeout: 3000 });
    expect(screen.queryByText(t("status.le_scanning"))).toBeNull();
  });

  it("forgets and renames the LE Audio entry by address and kind", async () => {
    const { calls } = start("#/devices", "le");
    const le = (await screen.findByText(t("device.tag_le_audio_set"))).closest(".device-row") as HTMLElement;
    fireEvent.click(within(le).getByRole("button", { name: `${t("device.edit_name")}: Galaxy Buds2 Pro` }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.input(within(dialog).getByLabelText(t("device.enter_name")), { target: { value: "Buds LE" } });
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.save") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls.find((c) => c.method === "device.rename")!.params).toEqual({
      address: "F4:4E:FD:00:11:22",
      name: "Buds LE",
      le_audio: true,
    });

    fireEvent.click(await screen.findByRole("button", { name: `${t("ui.device.forget")}: Buds LE` }));
    const confirm = await screen.findByRole("dialog");
    fireEvent.click(within(confirm).getByRole("button", { name: t("ui.device.forget") }));
    await waitFor(() => expect(screen.queryByText(/Buds LE/)).toBeNull());
    expect(calls.find((c) => c.method === "device.forget")!.params).toEqual({ address: "F4:4E:FD:00:11:22", le_audio: true });
  });
});

describe("diagnostics", () => {
  it("shows an LE Audio stream's ISO counters instead of the BR/EDR band", async () => {
    const { core } = start("#/diagnostics", "le");
    await screen.findByRole("heading", { level: 1, name: "Diagnostics" });
    await waitFor(() => expect(core.state.snap).not.toBeNull());
    expect(screen.queryByRole("heading", { name: t("ui.diag.iso_title") })).toBeNull();
    expect(screen.getByRole("heading", { name: t("quality.section_band") })).toBeInTheDocument();

    await core.request("stream.start", { profile: "Buds (LE Audio)" });
    await waitFor(() => expect(core.state.snap!.stream.state).toBe("streaming"));
    const group = (await screen.findByRole("heading", { name: t("ui.diag.iso_title") })).closest("section")!;
    expect(group).toHaveTextContent("LC3 48 kHz 10 ms 120 octets (96 kbps/ch), 2 ch over 2 CIS, 48_4_2");
    await waitFor(() => expect(within(group).getByText(t("ui.diag.iso_cis")).closest(".lq-row")).toHaveTextContent("2"));
    for (const key of ["ui.diag.iso_sdu_sets", "ui.diag.iso_silence", "ui.diag.iso_underruns", "ui.diag.iso_dropped"]) {
      expect(within(group).getByText(t(key)).closest(".lq-row")).toHaveTextContent(/\d/);
    }
    expect(within(group).getByText(t("ui.diag.iso_prefill")).closest(".lq-row")).toHaveTextContent(/\d+ frames/);
    expect(screen.queryByRole("heading", { name: t("quality.section_band") })).toBeNull();
    // The codec row names LC3
    expect(screen.getByText(t("quality.codec")).closest(".lq-row")).toHaveTextContent(t("ui.codec.lc3"));
  });

  it("reports the setting being off when an LE Audio profile starts", async () => {
    const { core } = start("#/", "le");
    await waitFor(() => expect(core.state.snap).not.toBeNull());
    await core.request("settings.set", { settings: { ...core.state.snap!.settings, le_audio_experimental: false } });
    await core.request("stream.start", { profile: "Buds (LE Audio)" });
    await waitFor(() => expect(core.state.snap!.stream.state).toBe("error"));
    expect(core.state.snap!.stream.message.key).toBe("error.le_audio_disabled");
  });
});
