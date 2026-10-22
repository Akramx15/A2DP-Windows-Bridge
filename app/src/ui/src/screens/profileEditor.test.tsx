// The profile editor against v1's profile dialog: saved devices renamed and
// removed, the codecs the device offers, the capture device's format

import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { setLanguage, t } from "../i18n";
import { MockCore } from "../ipc/mock";
import { createCore } from "../ipc/store";

let mock: MockCore | null = null;

function start(hash: string) {
  location.hash = hash;
  const calls: { method: string; params: unknown }[] = [];
  mock = new MockCore({ speed: 0, onCall: (method, params) => calls.push({ method, params }) });
  const core = createCore(mock);
  render(() => <App core={core} />);
  return { calls };
}

const edit = (name: string) => `#/profiles/edit/${encodeURIComponent(name)}`;
/** A new profile's device: the codecs and what follows wait for one */
const pickDevice = async () => fireEvent.click(await screen.findByRole("radio", { name: /WH-1000XM5/ }));

afterEach(async () => {
  cleanup();
  mock?.dispose();
  mock = null;
  await setLanguage("en");
});

describe("a new profile", () => {
  it("offers the codecs and the capture source once a device is chosen", async () => {
    start("#/profiles/new");
    await screen.findByRole("radio", { name: /WH-1000XM5/ });
    expect(screen.queryByRole("radiogroup", { name: t("connection.codec") })).toBeNull();
    expect(screen.queryByRole("radio", { name: t("capture.virtual") })).toBeNull();
    await pickDevice();
    expect(await screen.findByRole("radiogroup", { name: t("connection.codec") })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: t("capture.virtual") })).toBeInTheDocument();
  });

  it("folds the other devices away once one is picked", async () => {
    start("#/profiles/new");
    await pickDevice();
    await waitFor(() => expect(screen.queryByRole("radio", { name: /SRS-XB13/ })).toBeNull());
    expect(screen.getByRole("radio", { name: /WH-1000XM5/ })).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: t("ui.devices.show_others", 1) }));
    expect(await screen.findByRole("radio", { name: /SRS-XB13/ })).toBeInTheDocument();
  });
});

describe("saved devices on the Audio Devices screen", () => {
  it("renames a saved device", async () => {
    const { calls } = start("#/devices");
    fireEvent.click(await screen.findByRole("button", { name: `${t("device.edit_name")}: SRS-XB13` }));
    const dialog = await screen.findByRole("dialog");
    const input = within(dialog).getByLabelText(t("device.enter_name"));
    expect(input).toHaveValue("SRS-XB13");

    // Not empty, no "|": the core refuses those
    fireEvent.input(input, { target: { value: "a|b" } });
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.save") }));
    await within(dialog).findByText(t("ui.device.err_name"));
    expect(calls.some((c) => c.method === "device.rename")).toBe(false);

    fireEvent.input(input, { target: { value: "Desk speaker" } });
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.save") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls.find((c) => c.method === "device.rename")?.params).toEqual({
      address: "00:1A:7D:DA:71:13",
      name: "Desk speaker",
    });
    expect(await screen.findByText(/Desk speaker/)).toBeInTheDocument();
  });

  it("removes a saved device after asking", async () => {
    const { calls } = start("#/devices");
    fireEvent.click(await screen.findByRole("button", { name: `${t("ui.device.forget")}: SRS-XB13` }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("00:1A:7D:DA:71:13");
    fireEvent.click(within(dialog).getByRole("button", { name: t("ui.device.forget") }));
    await waitFor(() => expect(screen.queryByText(/SRS-XB13/)).toBeNull());
    expect(calls.find((c) => c.method === "device.forget")?.params).toEqual({ address: "00:1A:7D:DA:71:13" });
  });

  it("offers no rename or removal for devices that are not saved, but adding them", async () => {
    const { calls } = start("#/devices");
    fireEvent.click(await screen.findByRole("button", { name: t("connection.scan") }));
    const row = (await screen.findByText(/Galaxy Buds2 Pro/)).closest(".device-row") as HTMLElement;
    expect(within(row).queryByRole("button", { name: new RegExp(t("device.edit_name")) })).toBeNull();
    fireEvent.click(within(row).getByRole("button", { name: t("ui.devices.add") }));
    await waitFor(() => expect(calls.find((c) => c.method === "device.rename")?.params).toMatchObject({ name: "Galaxy Buds2 Pro" }));
    await waitFor(() => expect(within(row).queryByRole("button", { name: t("ui.devices.add") })).toBeNull());
  });

  it("makes a profile that picks the best codec for a device added without one", async () => {
    const { calls } = start("#/devices");
    fireEvent.click(await screen.findByRole("button", { name: t("connection.scan") }));
    const row = (await screen.findByText(/Galaxy Buds2 Pro/)).closest(".device-row") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: t("ui.devices.add") }));
    await waitFor(() => expect(calls.some((c) => c.method === "profile.save")).toBe(true));
    expect(calls.find((c) => c.method === "profile.save")!.params).toMatchObject({
      profile: { name: "Galaxy Buds2 Pro | Auto", codec: "auto", device_name: "Galaxy Buds2 Pro" },
      original_name: null,
    });
    // ... and connects with it, as pairing does on other systems
    await waitFor(() =>
      expect(calls.find((c) => c.method === "stream.start")?.params).toEqual({ profile: "Galaxy Buds2 Pro | Auto" }),
    );
  });

  it("leaves a running stream alone when a device is added", async () => {
    const { calls } = start("#/devices");
    fireEvent.click(await screen.findByRole("button", { name: t("connection.scan") }));
    const row = (await screen.findByText(/Galaxy Buds2 Pro/)).closest(".device-row") as HTMLElement;
    await mock!.call("stream.start", { profile: "WH-1000XM5 (LDAC)" });
    await waitFor(() => expect(screen.getAllByText(/Streaming/).length).toBeGreaterThan(0));
    calls.length = 0;
    fireEvent.click(within(row).getByRole("button", { name: t("ui.devices.add") }));
    await waitFor(() => expect(calls.some((c) => c.method === "profile.save")).toBe(true));
    expect(calls.some((c) => c.method === "stream.start")).toBe(false);
  });

  it("is only a list to choose from in the profile editor", async () => {
    start("#/profiles/new");
    await screen.findByRole("radio", { name: /WH-1000XM5/ });
    expect(screen.queryByRole("button", { name: t("connection.scan") })).toBeNull();
    expect(screen.queryByRole("button", { name: new RegExp(t("device.edit_name")) })).toBeNull();
    expect(screen.queryByRole("button", { name: new RegExp(t("ui.device.forget")) })).toBeNull();
    expect(screen.queryByLabelText(t("connection.manual"))).toBeNull();
  });
});

describe("codec cards", () => {
  it("show what the device offered when it last connected", async () => {
    start(edit("WH-1000XM5 (LDAC)"));
    const ldac = await screen.findByRole("radio", { name: /LDAC/ });
    expect(ldac.closest("label")).toHaveTextContent(t("ui.profile.codec_offered"));
    expect(screen.queryByText(t("ui.profile.codecs_unknown"))).toBeNull();

    // The ones it did not offer only when asked for (issue #86): SBC, AAC
    // and LDAC are offered, Auto is always there
    expect(screen.queryByRole("radio", { name: /aptX HD/ })).toBeNull();
    expect(screen.getAllByRole("radio", { name: /./ }).filter((r) => r.closest(".codec-grid"))).toHaveLength(4);
    // Forced: shown with their "Not offered" tag
    const force = screen.getByRole("checkbox", { name: t("ui.profile.codecs_force", 8) });
    fireEvent.click(force);
    const hd = await screen.findByRole("radio", { name: /aptX HD/ });
    expect(hd.closest("label")).toHaveTextContent(t("ui.profile.codec_not_offered"));
    expect(force).toBeChecked();

    // A codec it does not offer: said plainly, with what happens instead
    fireEvent.click(hd);
    expect(await screen.findByText(t("error.codec_not_supported", "aptX HD"))).toBeInTheDocument();
    expect(screen.getByText(t("ui.profile.codec_fallback_on"))).toBeInTheDocument();
  });

  it("say it is not known before the first connection", async () => {
    start(edit("Desk speaker (AAC)"));
    expect(await screen.findByText(t("ui.profile.codecs_unknown"))).toBeInTheDocument();
    expect(screen.queryByText(t("ui.profile.codec_offered"))).toBeNull();
    expect(screen.queryByText(t("ui.profile.codec_not_offered"))).toBeNull();
  });
});

/** Opens a codec's settings ("...") and returns its dialog */
async function codecSettings(label: string) {
  fireEvent.click(await screen.findByRole("button", { name: t("ui.profile.codec_settings", t(label)) }));
  return screen.findByRole("dialog");
}

describe("codec settings", () => {
  it("LHDC V5 offers 192 kHz and greys out the quality while ABR is on", async () => {
    start("#/profiles/new");
    // A device whose codecs are not known yet: all of them offered
    fireEvent.click(await screen.findByRole("radio", { name: /SRS-XB13/ }));
    const dialog = await codecSettings("codec.lhdc_v5");
    const rate = within(dialog).getByLabelText(t("connection.sample_rate"));
    expect(within(rate).getByRole("option", { name: /192/ })).toBeInTheDocument();
    const quality = within(dialog).getByLabelText(t("connection.quality"));
    expect(quality).not.toBeDisabled();
    fireEvent.click(within(dialog).getByRole("checkbox", { name: t("connection.abr") }));
    await waitFor(() => expect(quality).toBeDisabled());
    fireEvent.click(within(dialog).getByRole("checkbox", { name: t("connection.abr") }));
    await waitFor(() => expect(quality).not.toBeDisabled());
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.cancel") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // LDAC's ABR keeps the quality, and has no 192 kHz
    const ldac = await codecSettings("codec.ldac");
    fireEvent.click(within(ldac).getByRole("checkbox", { name: t("connection.abr") }));
    expect(within(ldac).getByLabelText(t("connection.quality"))).not.toBeDisabled();
    expect(within(within(ldac).getByLabelText(t("connection.sample_rate"))).queryByRole("option", { name: /192/ })).toBeNull();
  });

  it("OK chooses the codec with its settings, Cancel changes nothing", async () => {
    const { calls } = start(edit("WH-1000XM5 (LDAC)"));
    let dialog = await codecSettings("codec.aac");
    expect(within(dialog).getByText(t("ui.profile.codec_settings_apply", t("codec.aac")))).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText(t("connection.quality")), { target: { value: "sq" } });
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.cancel") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByRole("radio", { name: /LDAC/ })).toBeChecked();

    dialog = await codecSettings("codec.aac");
    fireEvent.change(within(dialog).getByLabelText(t("connection.quality")), { target: { value: "sq" } });
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByRole("radio", { name: /AAC/ })).toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: t("modal.save") }));
    await waitFor(() => expect(calls.some((c) => c.method === "profile.save")).toBe(true));
    const saved = calls.find((c) => c.method === "profile.save")!.params as { profile: { codec: string; quality: string } };
    expect(saved.profile).toMatchObject({ codec: "aac", quality: "sq" });
  });
});

describe("SBC settings", () => {
  it("offer what the device listed and show the bit rate as it changes", async () => {
    const { calls } = start(edit("WH-1000XM5 (LDAC)"));
    const dialog = await codecSettings("codec.sbc");
    // Auto: 16 blocks, 8 subbands, joint stereo, bitpool 53, at both rates
    const rate = within(dialog).getByRole("status");
    expect(rate).toHaveTextContent(`${t("ui.profile.sbc_bitrate", 327, "44.1")} / ${t("ui.profile.sbc_bitrate", 357, "48")}`);

    // The WH-1000XM5 lists 8 subbands and loudness only (the mock's XM5_SBC)
    const subbands = within(dialog).getByLabelText(t("ui.diag.cd_subbands"));
    expect(within(subbands).getAllByRole("option").map((o) => o.textContent)).toEqual([t("codec.auto"), "8"]);
    const alloc = within(dialog).getByLabelText(t("ui.diag.cd_allocation"));
    expect(within(alloc).queryByRole("option", { name: t("ui.diag.cd_alloc.snr") })).toBeNull();
    const blocks = within(dialog).getByLabelText(t("ui.diag.cd_blocks"));
    expect(within(blocks).getAllByRole("option")).toHaveLength(5);

    // 8 blocks: shorter frames, a higher rate
    fireEvent.change(blocks, { target: { value: "8" } });
    await waitFor(() => expect(rate).toHaveTextContent(t("ui.profile.sbc_bitrate", 363, "44.1")));
    // Stereo at 44.1 kHz with bitpool 40
    fireEvent.change(within(dialog).getByLabelText(t("ui.diag.cd_channel_mode")), { target: { value: "stereo" } });
    fireEvent.change(within(dialog).getByLabelText(t("connection.sample_rate")), { target: { value: "44100" } });
    fireEvent.input(within(dialog).getByLabelText(t("connection.sbc_bitpool")), { target: { value: "40" } });
    // 4 + 8 + 8 * 40 / 8 = 52 bytes per 64 samples
    await waitFor(() => expect(rate).toHaveTextContent(new RegExp(`^${t("ui.profile.sbc_bitrate", 286, "44.1")}$`)));

    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    fireEvent.click(screen.getByRole("button", { name: t("modal.save") }));
    await waitFor(() => expect(calls.some((c) => c.method === "profile.save")).toBe(true));
    const saved = calls.find((c) => c.method === "profile.save")!.params as { profile: Record<string, unknown> };
    expect(saved.profile).toMatchObject({
      codec: "sbc",
      sbc_blocks: 8,
      sbc_subbands: 0,
      sbc_allocation: "",
      sbc_channel_mode: "stereo",
      sbc_bitpool: 40,
      sample_rate: 44100,
    });
  });

  it("SBC XQ has no channel mode of its own", async () => {
    start(edit("WH-1000XM5 (LDAC)"));
    // Not among what the mock's WH-1000XM5 offered: shown when forced
    fireEvent.click(await screen.findByRole("checkbox", { name: t("ui.profile.codecs_force", 8) }));
    const dialog = await codecSettings("codec.sbc_xq");
    expect(within(dialog).queryByLabelText(t("ui.diag.cd_channel_mode"))).toBeNull();
    expect(within(dialog).getByRole("status")).toHaveTextContent(t("ui.profile.sbc_bitrate", 551, "44.1"));
  });
});

describe("capture source", () => {
  it("shows the capture device's format and warns when the profile differs", async () => {
    start(edit("WH-1000XM5 (LDAC)"));
    // Loopback: the default device (the mock's speakers mix 48 kHz, 32-bit)
    const line = await screen.findByText(t("capture.device_format", 48000, 32));
    expect(line).not.toHaveClass("field-error");

    const dialog = await codecSettings("codec.ldac");
    fireEvent.change(within(dialog).getByLabelText(t("connection.sample_rate")), { target: { value: "96000" } });
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(document.querySelector(".field-error")).not.toBeNull());
    const warning = document.querySelector(".field-error")!;
    expect(warning).toHaveTextContent(t("capture.device_format", 48000, 32));
    expect(warning).toHaveTextContent(t("ui.profile.format_differs", t("connection.sample_rate")));
  });

  it("follows the virtual device by default", async () => {
    start("#/profiles/new");
    await pickDevice();
    fireEvent.click(await screen.findByRole("radio", { name: t("capture.virtual") }));
    expect(await screen.findByRole("switch", { name: (n) => n.startsWith(t("capture.auto_switch")) })).toBeChecked();
  });

  it("says what application capture needs when the Windows build is not known", async () => {
    start(edit("Desk speaker (AAC)"));
    expect(await screen.findByText(t("ui.profile.app_capture_requirement"))).toBeInTheDocument();
    expect(screen.queryByText(t("ui.profile.app_capture_unsupported"))).toBeNull();
  });

  it("says plainly when this Windows cannot capture one application", async () => {
    // WebView2's client hints of Windows 10 1909 (build 18363)
    Object.defineProperty(navigator, "userAgentData", {
      configurable: true,
      value: { getHighEntropyValues: () => Promise.resolve({ platform: "Windows", platformVersion: "8.0.0" }) },
    });
    try {
      start(edit("Desk speaker (AAC)"));
      expect(await screen.findByText(t("ui.profile.app_capture_unsupported"))).toBeInTheDocument();
      expect(screen.getByLabelText(t("capture.app_select"))).toBeDisabled();
    } finally {
      delete (navigator as { userAgentData?: unknown }).userAgentData;
    }
  });
});

describe("SBC ABR", () => {
  it("is in the SBC dialog, and LDAC's ABR does not carry over to it", async () => {
    const { calls } = start(edit("WH-1000XM5 (LDAC)"));
    // LDAC with ABR on
    let dialog = await codecSettings("codec.ldac");
    fireEvent.click(within(dialog).getByRole("checkbox", { name: t("connection.abr") }));
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // SBC's dialog starts with its adaptive bitpool off
    dialog = await codecSettings("codec.sbc");
    const abr = within(dialog).getByRole("checkbox", { name: t("connection.abr") });
    expect(abr).not.toBeChecked();
    fireEvent.click(abr);
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    fireEvent.click(screen.getByRole("button", { name: t("modal.save") }));
    await waitFor(() => expect(calls.some((c) => c.method === "profile.save")).toBe(true));
    const saved = calls.find((c) => c.method === "profile.save")!.params as { profile: Record<string, unknown> };
    expect(saved.profile).toMatchObject({ codec: "sbc", abr: true });
  });

  it("SBC XQ offers it too", async () => {
    start(edit("WH-1000XM5 (LDAC)"));
    fireEvent.click(await screen.findByRole("checkbox", { name: t("ui.profile.codecs_force", 8) }));
    const dialog = await codecSettings("codec.sbc_xq");
    expect(within(dialog).getByRole("checkbox", { name: t("connection.abr") })).not.toBeChecked();
  });
});
