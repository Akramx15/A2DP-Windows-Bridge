// The Codecs screen (issue #86, it replaced the profile editor: v1's
// profile dialog) and the Audio Devices screen: saved devices renamed and
// removed, the codecs each device offers and their settings, the capture
// device's format, the packet size

import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { setLanguage, t } from "../i18n";
import { MockCore } from "../ipc/mock";
import { createCore } from "../ipc/store";
import { codecSpec } from "../lib/codecs";

let mock: MockCore | null = null;

function start(hash: string) {
  location.hash = hash;
  const calls: { method: string; params: unknown }[] = [];
  mock = new MockCore({ speed: 0, onCall: (method, params) => calls.push({ method, params }) });
  const core = createCore(mock);
  render(() => <App core={core} />);
  return { calls, core };
}

/** A device's section on the Codecs screen */
const section = (name: string) => screen.findByRole("region", { name });
/** The cards can be chosen again (the last choice saved) */
const settled = (where: HTMLElement) =>
  waitFor(() => expect(within(where).getAllByRole("radio").every((r) => !(r as HTMLInputElement).disabled)).toBe(true));
/** The screen itself (the sidebar names the devices too) */
const main = () => within(screen.getByRole("main"));
/** "Auto-select the codec with the highest quality", for every device */
const autoName = (n: string) => n.startsWith(t("ui.codecs.auto_select"));
const autoSwitch = () => main().getByRole("switch", { name: autoName });
/** The profiles saved, in order */
const saves = (calls: { method: string; params: unknown }[]) =>
  calls
    .filter((c) => c.method === "profile.save")
    .map((c) => c.params as { profile: Record<string, unknown>; original_name: string | null });

afterEach(async () => {
  cleanup();
  mock?.dispose();
  mock = null;
  localStorage.clear();
  await setLanguage("en");
});

describe("the Codecs screen", () => {
  it("lists each saved device with its codec cards, the codec in use chosen", async () => {
    start("#/codecs");
    expect(await screen.findByRole("heading", { level: 1, name: t("ui.nav.codecs") })).toBeInTheDocument();
    const wh = await section("WH-1000XM5");
    expect(wh).toHaveTextContent("AC:80:0A:12:34:56");
    expect(wh).toHaveTextContent(t("device.tag_saved"));
    expect(within(wh).getByRole("radio", { name: /LDAC/ })).toBeChecked();
    // No Auto card: Auto is one switch for every device (issue #86), off
    // here, not the device's own
    expect(within(wh).queryByRole("radio", { name: /Auto/ })).toBeNull();
    expect(autoSwitch()).not.toBeChecked();
    expect(within(wh).queryByRole("switch", { name: autoName })).toBeNull();
    const xb = await section("SRS-XB13");
    expect(within(xb).getByRole("radio", { name: /AAC/ })).toBeChecked();
    // What is the device's own is on Audio Devices
    expect(screen.getByText(t("ui.codecs.page_hint"), { exact: false })).toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: t("capture.virtual") })).toBeNull();
  });

  it("saves the device's profile when a card is chosen, its name following the codec", async () => {
    const { calls } = start("#/codecs");
    // A device without a profile yet: Auto until one is chosen; with
    // Auto-select off its cards can be chosen
    await mock!.call("device.rename", { address: "54:2F:04:4B:29:DC", name: "moto buds+" });
    const moto = await section("moto buds+");
    expect(within(moto).getByRole("radio", { name: /AAC/ })).not.toBeDisabled();
    // One click on a card chooses it
    fireEvent.click(within(moto).getByRole("radio", { name: /AAC/ }));
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    expect(saves(calls)[0]).toMatchObject({
      profile: { name: "moto buds+ | AAC", device_address: "54:2F:04:4B:29:DC", codec: "aac" },
      original_name: null,
    });
    await settled(moto);
    fireEvent.click(within(moto).getByRole("radio", { name: /LDAC/ }));
    await waitFor(() => expect(saves(calls)).toHaveLength(2));
    expect(saves(calls)[1]).toMatchObject({
      profile: { name: "moto buds+ | LDAC", codec: "ldac" },
      original_name: "moto buds+ | AAC",
    });
    // A name of the user's own stays
    await settled(moto);
    const wh = await section("WH-1000XM5");
    fireEvent.click(within(wh).getByRole("radio", { name: /AAC/ }));
    await waitFor(() => expect(saves(calls)).toHaveLength(3));
    expect(saves(calls)[2]).toMatchObject({ profile: { name: "WH-1000XM5 (LDAC)", codec: "aac" } });
  });

  it("keeps each device's codec to that device (issue #86: LHDC asked of the EarFun after the moto buds+)", async () => {
    const { calls } = start("#/codecs");
    await mock!.call("device.rename", { address: "54:2F:04:4B:29:DC", name: "moto buds+" });
    const moto = await section("moto buds+");
    // Its codecs are not known yet: LHDC V5 is offered
    fireEvent.click(within(moto).getByRole("radio", { name: /LHDC V5/ }));
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    expect(saves(calls)[0].profile).toMatchObject({ device_address: "54:2F:04:4B:29:DC", codec: "lhdcv5" });
    await settled(moto);
    // The other devices keep theirs, on screen and in what Connect streams
    const wh = await section("WH-1000XM5");
    expect(within(wh).getByRole("radio", { name: /LDAC/ })).toBeChecked();
    expect(within(await section("SRS-XB13")).getByRole("radio", { name: /AAC/ })).toBeChecked();
    const snap = (await mock!.call("state.snapshot")) as { profiles: { device_address: string; codec: string }[] };
    expect(snap.profiles.map((p) => [p.device_address, p.codec])).toEqual([
      ["AC:80:0A:12:34:56", "ldac"],
      ["00:1A:7D:DA:71:13", "aac"],
      ["54:2F:04:4B:29:DC", "lhdcv5"],
    ]);
    // A new device starts from Auto, not from the codec chosen last
    await mock!.call("device.rename", { address: "70:5A:6F:6D:ED:1F", name: "EarFun Air Pro 4+" });
    const earfun = await section("EarFun Air Pro 4+");
    expect(within(earfun).getByRole("radio", { name: /LHDC V5/ })).not.toBeChecked();
    location.hash = "#/home";
    const card = await screen.findByRole("group", { name: "EarFun Air Pro 4+" });
    fireEvent.click(within(card).getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(saves(calls)).toHaveLength(2));
    expect(saves(calls)[1].profile).toMatchObject({ device_address: "70:5A:6F:6D:ED:1F", codec: "auto" });
  });

  it("brings a codec's settings back when it is chosen again (issue #86: AAC's VBR came back off)", async () => {
    const { calls } = start("#/codecs");
    const wh = await section("WH-1000XM5");
    const dialog = await codecSettings(wh, "codec.aac");
    fireEvent.click(within(dialog).getByRole("checkbox", { name: t("connection.vbr") }));
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    expect(saves(calls)[0].profile).toMatchObject({ codec: "aac", abr: true, codec_settings: { ldac: { quality: "hq" } } });
    await settled(wh);
    fireEvent.click(within(wh).getByRole("radio", { name: /LDAC/ }));
    await waitFor(() => expect(saves(calls)).toHaveLength(2));
    expect(saves(calls)[1].profile).toMatchObject({ codec: "ldac", abr: false, codec_settings: { aac: { abr: true } } });
    await settled(wh);
    fireEvent.click(within(wh).getByRole("radio", { name: /AAC/ }));
    await waitFor(() => expect(saves(calls)).toHaveLength(3));
    expect(saves(calls)[2].profile).toMatchObject({ codec: "aac", abr: true });
    expect(saves(calls)[2].profile.codec_settings).not.toHaveProperty("aac");
  });

  it("turns Auto-select on and off for every device, each keeping the codec chosen for it (issue #86)", async () => {
    const { calls } = start("#/codecs");
    // No page-wide fallback option any more: a codec always falls back
    expect(screen.queryByRole("checkbox", { name: /another supported codec/ })).toBeNull();
    const wh = await section("WH-1000XM5");
    fireEvent.click(within(wh).getByRole("radio", { name: /AAC/ }));
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    await settled(wh);
    fireEvent.click(autoSwitch());
    await waitFor(() => expect(autoSwitch()).toBeChecked());
    expect(calls.filter((c) => c.method === "settings.set").at(-1)?.params).toMatchObject({
      settings: { auto_codec: true },
    });
    // The WH-1000XM5 supported SBC, AAC and LDAC: Auto picks LDAC, shown
    // on the read-only cards; every device's cards are read-only
    await waitFor(() => expect(wh.querySelector(".codec-more")).toBeNull());
    const ldac = within(wh).getByRole("radio", { name: /LDAC/ });
    expect(ldac).toBeDisabled();
    expect(ldac).toBeChecked();
    expect(wh).toHaveTextContent(t("ui.codecs.auto_uses", t("codec.ldac")));
    expect(within(await section("SRS-XB13")).getByRole("radio", { name: /AAC/ })).toBeDisabled();
    // No profile changed: off again, the codec chosen is back
    fireEvent.click(autoSwitch());
    await waitFor(() => expect(within(wh).getByRole("radio", { name: /AAC/ })).toBeChecked());
    expect(within(wh).getByRole("radio", { name: /AAC/ })).not.toBeDisabled();
    expect(wh).not.toHaveTextContent(t("ui.codecs.auto_uses", t("codec.ldac")));
    expect(saves(calls)).toHaveLength(1);
  });

  it("falls back to the next supported codec below the chosen one", async () => {
    const { calls } = start("#/codecs");
    const wh = await section("WH-1000XM5");
    fireEvent.click(screen.getByRole("checkbox", { name: t("ui.profile.codecs_hide", 9) }));
    // aptX HD: not offered by the WH-1000XM5 (SBC, AAC, LDAC); aptX and
    // aptX LL are not either, so AAC, not LDAC above it
    fireEvent.click(await within(wh).findByRole("radio", { name: /aptX HD/ }));
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    expect(await within(wh).findByText(t("error.codec_not_supported", "aptX HD"))).toBeInTheDocument();
    expect(within(wh).getByText(t("ui.codecs.fallback_to", t("codec.aac")))).toBeInTheDocument();
  });

  it("hides the experimental codecs and falls back from them when they are turned off (issue #86)", async () => {
    const { calls } = start("#/codecs");
    const wh = await section("WH-1000XM5");
    fireEvent.click(screen.getByRole("checkbox", { name: t("ui.profile.codecs_hide", 9) }));
    // On by default: aptX HD shown, tagged Experimental; aptX Adaptive is not
    const hd = await within(wh).findByRole("radio", { name: /aptX HD/ });
    expect(hd.closest("label")).toHaveTextContent(t("ui.codecs.experimental_tag"));
    expect(within(wh).getByRole("radio", { name: /aptX Adaptive/ }).closest("label")).not.toHaveTextContent(
      t("ui.codecs.experimental_tag"),
    );
    fireEvent.click(hd);
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    await settled(wh);

    const box = screen.getByRole("checkbox", { name: t("ui.codecs.experimental") });
    expect(box).toBeChecked();
    fireEvent.click(box);
    await waitFor(() =>
      expect(calls.find((c) => c.method === "settings.set")?.params).toMatchObject({
        settings: { experimental_codecs: false },
      }),
    );
    // aptX HD, aptX LL, Opus, Opus 05 and FastStream are gone
    await waitFor(() => expect(within(wh).queryByRole("radio", { name: /aptX HD/ })).toBeNull());
    for (const name of [/aptX Low Latency/, /^Opus$/, /Opus 05/, /FastStream/]) {
      expect(within(wh).queryByRole("radio", { name })).toBeNull();
    }
    expect(within(wh).getByRole("radio", { name: /aptX Adaptive/ })).toBeInTheDocument();
    // The chosen aptX HD: said so, with what is used instead (aptX LL below
    // it is experimental too, so AAC)
    expect(within(wh).getByText(t("error.codec_experimental_off", "aptX HD"))).toBeInTheDocument();
    expect(within(wh).getByText(t("ui.codecs.experimental_fallback_to", t("codec.aac")))).toBeInTheDocument();
    expect(box).not.toBeChecked();
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
    expect(await main().findByText(/Desk speaker/)).toBeInTheDocument();
  });

  it("removes a saved device after asking", async () => {
    const { calls } = start("#/devices");
    fireEvent.click(await screen.findByRole("button", { name: `${t("ui.device.forget")}: SRS-XB13` }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("00:1A:7D:DA:71:13");
    fireEvent.click(within(dialog).getByRole("button", { name: t("ui.device.forget") }));
    await waitFor(() => expect(main().queryByText(/SRS-XB13/)).toBeNull());
    expect(calls.find((c) => c.method === "device.forget")?.params).toEqual({ address: "00:1A:7D:DA:71:13" });
  });

  it("offers no rename or removal for devices that are not saved, but adding them", async () => {
    const { calls } = start("#/devices");
    fireEvent.click(await screen.findByRole("button", { name: t("connection.scan") }));
    const row = (await screen.findByText(/Galaxy Buds2 Pro/)).closest(".device-row") as HTMLElement;
    expect(within(row).queryByRole("button", { name: new RegExp(t("device.edit_name")) })).toBeNull();
    fireEvent.click(within(row).getByRole("button", { name: t("ui.devices.add") }));
    await waitFor(() => expect(calls.find((c) => c.method === "device.rename")?.params).toMatchObject({ name: "Galaxy Buds2 Pro" }));
    // Saved: it moves up to the saved devices, with its settings (issue #86)
    await waitFor(() =>
      expect(main().getByText(/Galaxy Buds2 Pro/).closest(".saved-devices")).not.toBeNull(),
    );
    const entry = main().getByText(/Galaxy Buds2 Pro/).closest(".device-entry") as HTMLElement;
    expect(within(entry).queryByRole("button", { name: t("ui.devices.add") })).toBeNull();
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
});

describe("codec cards", () => {
  it("show what the device offered when it last connected", async () => {
    const { calls } = start("#/codecs");
    const wh = await section("WH-1000XM5");
    const ldac = within(wh).getByRole("radio", { name: /LDAC/ });
    expect(ldac.closest("label")).toHaveTextContent(t("ui.profile.codec_supported"));
    expect(within(wh).queryByText(t("ui.profile.codecs_unknown"))).toBeNull();

    // In green
    expect(within(ldac.closest("label")!).getByText(t("ui.profile.codec_supported")).closest(".tag")).toHaveClass("st-good");
    // The ones it did not support hidden (on by default, issue #86): SBC,
    // AAC and LDAC are supported
    expect(within(wh).queryByRole("radio", { name: /aptX HD/ })).toBeNull();
    expect(within(wh).getAllByRole("radio")).toHaveLength(3);
    const hide = screen.getByRole("checkbox", { name: t("ui.profile.codecs_hide", 9) });
    expect(hide).toBeChecked();
    // Shown: with their "Not supported" tag
    fireEvent.click(hide);
    const hd = await within(wh).findByRole("radio", { name: /aptX HD/ });
    expect(hd.closest("label")).toHaveTextContent(t("ui.profile.codec_not_supported"));
    // In red
    expect(within(hd.closest("label")!).getByText(t("ui.profile.codec_not_supported")).closest(".tag")).toHaveClass("st-bad");
    expect(hide).not.toBeChecked();

    // A codec it does not offer: said plainly, with what happens instead
    fireEvent.click(hd);
    await waitFor(() => expect(calls.some((c) => c.method === "profile.save")).toBe(true));
    expect(await within(wh).findByText(t("error.codec_not_supported", "aptX HD"))).toBeInTheDocument();
    expect(within(wh).getByText(t("ui.codecs.fallback_to", t("codec.aac")))).toBeInTheDocument();
  });

  it("say it is not known before the first connection", async () => {
    start("#/codecs");
    const xb = await section("SRS-XB13");
    expect(within(xb).getByText(t("ui.profile.codecs_unknown"))).toBeInTheDocument();
    expect(within(xb).queryByText(t("ui.profile.codec_supported"))).toBeNull();
    expect(within(xb).queryByText(t("ui.profile.codec_not_supported"))).toBeNull();
    // Each card says so, in neither green nor red
    const aac = within(xb).getByRole("radio", { name: /AAC/ }).closest("label")!;
    const tag = within(aac).getByText(t("ui.profile.codec_support_unknown")).closest(".tag")!;
    expect(tag).not.toHaveClass("st-good");
    expect(tag).not.toHaveClass("st-bad");
  });

  it("show Auto's order behind the ? beside the switch", async () => {
    start("#/codecs");
    await section("WH-1000XM5");
    const help = main().getByRole("button", { name: t("ui.codecs.auto_order") });
    const tip = document.getElementById(help.getAttribute("aria-describedby")!)!;
    expect(tip).toHaveAttribute("role", "tooltip");
    const items = within(tip).getAllByRole("listitem").map((li) => li.textContent);
    expect(items[0]).toBe(t(codecSpec("lhdcv5").label));
    expect(items.at(-1)).toBe(t(codecSpec("sbc").label));
    expect(items).toHaveLength(10);
  });
});

/** Opens a codec's settings ("...") in a device's section and returns its dialog */
async function codecSettings(where: HTMLElement, label: string) {
  fireEvent.click(await within(where).findByRole("button", { name: t("ui.profile.codec_settings", t(label)) }));
  return screen.findByRole("dialog");
}

describe("codec settings", () => {
  it("LHDC V5 offers 192 kHz and greys out the quality while ABR is on", async () => {
    start("#/codecs");
    // A device whose codecs are not known yet: all of them offered
    const xb = await section("SRS-XB13");
    const dialog = await codecSettings(xb, "codec.lhdc_v5");
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
    const ldac = await codecSettings(xb, "codec.ldac");
    fireEvent.click(within(ldac).getByRole("checkbox", { name: t("connection.abr") }));
    expect(within(ldac).getByLabelText(t("connection.quality"))).not.toBeDisabled();
    expect(within(within(ldac).getByLabelText(t("connection.sample_rate"))).queryByRole("option", { name: /192/ })).toBeNull();
  });

  it("OK chooses the codec with its settings, Cancel changes nothing", async () => {
    const { calls } = start("#/codecs");
    const wh = await section("WH-1000XM5");
    let dialog = await codecSettings(wh, "codec.aac");
    expect(within(dialog).getByText(t("ui.profile.codec_settings_apply", t("codec.aac")))).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText(t("connection.quality")), { target: { value: "sq" } });
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.cancel") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(within(wh).getByRole("radio", { name: /LDAC/ })).toBeChecked();
    expect(calls.some((c) => c.method === "profile.save")).toBe(false);

    dialog = await codecSettings(wh, "codec.aac");
    fireEvent.change(within(dialog).getByLabelText(t("connection.quality")), { target: { value: "sq" } });
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    expect(saves(calls)[0].profile).toMatchObject({ codec: "aac", quality: "sq" });
    await waitFor(() => expect(within(wh).getByRole("radio", { name: /AAC/ })).toBeChecked());
  });
});

describe("SBC settings", () => {
  it("offer what the device listed and show the bit rate as it changes", async () => {
    const { calls } = start("#/codecs");
    const dialog = await codecSettings(await section("WH-1000XM5"), "codec.sbc");
    // Auto: 16 blocks, 8 subbands, joint stereo, bitpool 53, at the highest
    // rate only; read-only where the quality list was (issue #86)
    const rate = within(dialog).getByRole("status");
    expect(rate).toBe(within(dialog).getByLabelText(t("connection.quality")));
    expect(rate).toHaveTextContent(new RegExp(`^${t("ui.profile.sbc_bitrate", 357, "48")}$`));

    // The WH-1000XM5 lists 8 subbands and loudness only (the mock's XM5_SBC)
    const subbands = within(dialog).getByLabelText(t("ui.diag.cd_subbands"));
    expect(within(subbands).getAllByRole("option").map((o) => o.textContent)).toEqual([t("codec.auto"), "8"]);
    const alloc = within(dialog).getByLabelText(t("ui.diag.cd_allocation"));
    expect(within(alloc).queryByRole("option", { name: t("ui.diag.cd_alloc.snr") })).toBeNull();
    const blocks = within(dialog).getByLabelText(t("ui.diag.cd_blocks"));
    expect(within(blocks).getAllByRole("option")).toHaveLength(5);

    // 8 blocks: shorter frames, a higher rate
    fireEvent.change(blocks, { target: { value: "8" } });
    await waitFor(() => expect(rate).toHaveTextContent(t("ui.profile.sbc_bitrate", 396, "48")));
    // Stereo at 44.1 kHz with bitpool 40
    fireEvent.change(within(dialog).getByLabelText(t("ui.diag.cd_channel_mode")), { target: { value: "stereo" } });
    fireEvent.change(within(dialog).getByLabelText(t("connection.sample_rate")), { target: { value: "44100" } });
    fireEvent.input(within(dialog).getByLabelText(t("connection.sbc_bitpool")), { target: { value: "40" } });
    // 4 + 8 + 8 * 40 / 8 = 52 bytes per 64 samples
    await waitFor(() => expect(rate).toHaveTextContent(new RegExp(`^${t("ui.profile.sbc_bitrate", 286, "44.1")}$`)));

    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    expect(saves(calls)[0].profile).toMatchObject({
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
    start("#/codecs");
    // Not among what the mock's WH-1000XM5 supported: shown when not hidden
    fireEvent.click(await screen.findByRole("checkbox", { name: t("ui.profile.codecs_hide", 9) }));
    const dialog = await codecSettings(await section("WH-1000XM5"), "codec.sbc_xq");
    expect(within(dialog).queryByLabelText(t("ui.diag.cd_channel_mode"))).toBeNull();
    // Bitpool 43 per channel at 48 kHz
    expect(within(dialog).getByRole("status")).toHaveTextContent(t("ui.profile.sbc_bitrate", 552, "48"));
  });

  it("plain SBC offers no dual channel: that is SBC XQ (issue #86)", async () => {
    start("#/codecs");
    const dialog = await codecSettings(await section("WH-1000XM5"), "codec.sbc");
    const mode = within(dialog).getByLabelText(t("ui.diag.cd_channel_mode"));
    expect(within(mode).getAllByRole("option").map((o) => o.textContent)).toEqual(
      [t("codec.auto"), ...["joint_stereo", "stereo", "mono"].map((m) => t(`ui.diag.cd_mode.${m}`))],
    );
  });

  it("SBC's ABR and frame setup do not carry to SBC XQ, nor back (issue #86)", async () => {
    const { calls } = start("#/codecs");
    fireEvent.click(await screen.findByRole("checkbox", { name: t("ui.profile.codecs_hide", 9) }));
    const wh = await section("WH-1000XM5");
    let dialog = await codecSettings(wh, "codec.sbc");
    fireEvent.click(within(dialog).getByRole("checkbox", { name: t("connection.abr") }));
    fireEvent.change(within(dialog).getByLabelText(t("ui.diag.cd_blocks")), { target: { value: "8" } });
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    expect(saves(calls)[0].profile).toMatchObject({ codec: "sbc", abr: true, sbc_blocks: 8 });
    await settled(wh);

    dialog = await codecSettings(wh, "codec.sbc_xq");
    expect(within(dialog).getByRole("checkbox", { name: t("connection.abr") })).not.toBeChecked();
    expect(within(dialog).getByLabelText(t("ui.diag.cd_blocks"))).toHaveValue("0");
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(saves(calls)).toHaveLength(2));
    expect(saves(calls)[1].profile).toMatchObject({ codec: "sbcxq", abr: false, sbc_blocks: 0 });
    expect(saves(calls)[1].profile.codec_settings).toMatchObject({ sbc: { abr: true, sbc_blocks: 8 } });
    await settled(wh);

    // SBC's own come back as they were
    dialog = await codecSettings(wh, "codec.sbc");
    expect(within(dialog).getByRole("checkbox", { name: t("connection.abr") })).toBeChecked();
    expect(within(dialog).getByLabelText(t("ui.diag.cd_blocks"))).toHaveValue("8");
  });
});

/** A saved device's entry on the Audio Devices screen (its row and settings) */
const deviceEntry = async (name: RegExp) =>
  (await within(await screen.findByRole("main")).findAllByText(name))
    .map((e) => e.closest(".device-entry"))
    .find((e) => e) as HTMLElement;

describe("a device's capture source (Audio Devices, issue #86)", () => {
  it("shows the capture device's format and warns when a profile differs", async () => {
    start("#/devices");
    const entry = await deviceEntry(/WH-1000XM5/);
    // Loopback: the default device (the mock's speakers mix 48 kHz, 32-bit)
    const line = await within(entry).findByText(t("capture.device_format", 48000, 32));
    expect(line).not.toHaveClass("field-error");

    const snap = (await mock!.call("state.snapshot")) as { profiles: { name: string }[] };
    const p = snap.profiles.find((x) => x.name === "WH-1000XM5 (LDAC)")!;
    await mock!.call("profile.save", { profile: { ...p, sample_rate: 96000 }, original_name: p.name });
    await waitFor(() => expect(entry.querySelector(".field-error")).not.toBeNull());
    const warning = entry.querySelector(".field-error")!;
    expect(warning).toHaveTextContent(t("capture.device_format", 48000, 32));
    expect(warning).toHaveTextContent(t("ui.profile.format_differs", t("connection.sample_rate")));
  });

  it("lists System Loopback, the virtual devices and the playing apps in one list", async () => {
    start("#/devices");
    const entry = await deviceEntry(/WH-1000XM5/);
    const select = within(entry).getByLabelText(t("capture.mode"));
    const virtual = await within(select).findByRole("group", { name: t("capture.virtual") });
    expect(within(virtual).getByRole("option", { name: "CABLE Input (VB-Audio Virtual Cable)" })).toBeInTheDocument();
    const apps = await within(select).findByRole("group", { name: t("capture.app") });
    expect(within(apps).getByRole("option", { name: "Spotify" })).toBeInTheDocument();
    expect(within(select).getByRole("option", { name: t("capture.loopback") })).toBeInTheDocument();
    // The long explanation is behind the info button
    expect(within(entry).queryByText(t("capture.hint_loopback"))).toBeNull();
    fireEvent.click(within(entry).getByRole("button", { name: t("ui.more_info", t("capture.mode")) }));
    expect(within(entry).getByText(t("capture.hint_loopback"))).toBeInTheDocument();
  });

  it("is saved into all the device's profiles when chosen", async () => {
    const { calls } = start("#/devices");
    const snap = (await mock!.call("state.snapshot")) as { profiles: { name: string }[] };
    const p = snap.profiles.find((x) => x.name === "WH-1000XM5 (LDAC)")!;
    await mock!.call("profile.save", { profile: { ...p, name: "XM5 AAC", codec: "aac" }, original_name: null });
    const entry = await deviceEntry(/WH-1000XM5/);
    calls.length = 0;
    expect(within(entry).queryByRole("switch", { name: (n) => n.startsWith(t("capture.auto_switch")) })).toBeNull();
    const select = within(entry).getByLabelText(t("capture.mode"));
    await within(select).findByRole("option", { name: "CABLE Input (VB-Audio Virtual Cable)" });
    fireEvent.change(select, { target: { value: "virtual:{0.0.0.00000000}.{cable}" } });
    await waitFor(() => expect(calls.filter((c) => c.method === "profile.save")).toHaveLength(2));
    // A virtual device can become the Windows default output
    expect(await within(entry).findByRole("switch", { name: (n) => n.startsWith(t("capture.auto_switch")) })).toBeInTheDocument();
    for (const c of calls.filter((x) => x.method === "profile.save")) {
      expect((c.params as { profile: Record<string, unknown> }).profile).toMatchObject({
        capture_mode: "virtual",
        audio_device_id: "{0.0.0.00000000}.{cable}",
      });
    }
  });

  it("gives a device without a profile one with its first setting", async () => {
    const { calls } = start("#/devices");
    await mock!.call("device.rename", { address: "54:2F:04:4B:29:DC", name: "moto buds+" });
    const entry = await deviceEntry(/moto buds\+/);
    const select = within(entry).getByLabelText(t("capture.mode"));
    await within(select).findByRole("option", { name: "Spotify" });
    // The source it has already: nothing to save
    fireEvent.change(select, { target: { value: "loopback" } });
    expect(calls.some((c) => c.method === "profile.save")).toBe(false);
    fireEvent.change(select, { target: { value: "app:spotify.exe" } });
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    expect(saves(calls)[0]).toMatchObject({
      profile: {
        name: "moto buds+ | Auto",
        codec: "auto",
        capture_mode: "app",
        app_exe: "Spotify.exe",
        device_address: "54:2F:04:4B:29:DC",
      },
      original_name: null,
    });
  });

  it("keeps the buttons and remembered volume per device", async () => {
    const { calls } = start("#/devices");
    const entry = await deviceEntry(/WH-1000XM5/);
    // The mock's WH-1000XM5 has its buttons off, the global setting is on
    const keys = within(entry).getByRole("switch", { name: (n) => n.startsWith(t("settings.media_keys")) });
    expect(keys).not.toBeChecked();
    fireEvent.click(keys);
    await waitFor(() => expect(calls.some((c) => c.method === "settings.set")).toBe(true));
    const set = calls.find((c) => c.method === "settings.set")!.params as {
      settings: { device_options: Record<string, unknown> };
    };
    expect(set.settings.device_options["AC:80:0A:12:34:56"]).toEqual({ media_keys: true, remember_volume: null });
    // Settings keeps only the general ones
    location.hash = "#/settings";
    await screen.findByRole("heading", { name: t("settings.header") });
    expect(screen.queryByRole("switch", { name: (n) => n.startsWith(t("settings.media_keys")) })).toBeNull();
  });

  it("says what application capture needs when the Windows build is not known", async () => {
    start("#/devices");
    const entry = await deviceEntry(/SRS-XB13/);
    fireEvent.click(within(entry).getByRole("button", { name: t("ui.more_info", t("capture.mode")) }));
    expect(await within(entry).findByText(t("ui.profile.app_capture_requirement"))).toBeInTheDocument();
    expect(within(entry).queryByText(t("ui.profile.app_capture_unsupported"))).toBeNull();
  });

  it("says plainly when this Windows cannot capture one application", async () => {
    // WebView2's client hints of Windows 10 1909 (build 18363)
    Object.defineProperty(navigator, "userAgentData", {
      configurable: true,
      value: { getHighEntropyValues: () => Promise.resolve({ platform: "Windows", platformVersion: "8.0.0" }) },
    });
    try {
      start("#/devices");
      const entry = await deviceEntry(/SRS-XB13/);
      expect(await within(entry).findByText(t("ui.profile.app_capture_unsupported"))).toBeInTheDocument();
      // Only the app it has: no playing app to choose
      const apps = within(within(entry).getByLabelText(t("capture.mode"))).getByRole("group", { name: t("capture.app") });
      expect(within(apps).getAllByRole("option")).toHaveLength(1);
    } finally {
      delete (navigator as { userAgentData?: unknown }).userAgentData;
    }
  });
});

describe("the max media packet size (the device's since issue #86)", () => {
  it("is shown only in developer mode while it has the recommended size", async () => {
    const { core } = start("#/devices");
    const entry = await deviceEntry(/WH-1000XM5/);
    expect(within(entry).queryByLabelText(t("advanced.max_packet"))).toBeNull();
    await core.request("settings.set", { settings: { ...core.state.snap!.settings, developer_mode: true } });
    expect(await within(entry).findByLabelText(t("advanced.max_packet"))).toHaveValue(1023);
  });

  it("is checked, then saved into the device's profile", async () => {
    const { calls, core } = start("#/devices");
    const entry = await deviceEntry(/WH-1000XM5/);
    await core.request("settings.set", { settings: { ...core.state.snap!.settings, developer_mode: true } });
    const packet = await within(entry).findByLabelText(t("advanced.max_packet"));
    expect(packet).toHaveValue(1023);
    fireEvent.input(packet, { target: { value: "2000" } });
    fireEvent.change(packet, { target: { value: "2000" } });
    expect(await within(entry).findByText(t("ui.profile.err_packet", 679, 1679))).toBeInTheDocument();
    expect(calls.some((c) => c.method === "profile.save")).toBe(false);
    fireEvent.input(packet, { target: { value: "800" } });
    fireEvent.change(packet, { target: { value: "800" } });
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    expect(saves(calls)[0].profile).toMatchObject({ name: "WH-1000XM5 (LDAC)", max_media_payload: 800 });
    // A size changed earlier stays in sight without developer mode, so it can be set back
    await core.request("settings.set", { settings: { ...core.state.snap!.settings, developer_mode: false } });
    expect(await within(entry).findByLabelText(t("advanced.max_packet"))).toHaveValue(800);
    // Not on the Codecs screen any more
    location.hash = "#/codecs";
    await screen.findByRole("heading", { level: 1, name: t("ui.nav.codecs") });
    expect(screen.queryByLabelText(t("advanced.max_packet"))).toBeNull();
  });
});

describe("SBC ABR", () => {
  it("is in the SBC dialog, and LDAC's ABR does not carry over to it", async () => {
    const { calls } = start("#/codecs");
    const wh = await section("WH-1000XM5");
    // LDAC with ABR on
    let dialog = await codecSettings(wh, "codec.ldac");
    fireEvent.click(within(dialog).getByRole("checkbox", { name: t("connection.abr") }));
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(saves(calls)).toHaveLength(1));
    // SBC's dialog starts with its adaptive bitpool off
    dialog = await codecSettings(wh, "codec.sbc");
    const abr = within(dialog).getByRole("checkbox", { name: t("connection.abr") });
    expect(abr).not.toBeChecked();
    fireEvent.click(abr);
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.ok") }));
    await waitFor(() => expect(saves(calls)).toHaveLength(2));
    expect(saves(calls)[1].profile).toMatchObject({ codec: "sbc", abr: true });
  });

  it("SBC XQ offers it too", async () => {
    start("#/codecs");
    fireEvent.click(await screen.findByRole("checkbox", { name: t("ui.profile.codecs_hide", 9) }));
    const dialog = await codecSettings(await section("WH-1000XM5"), "codec.sbc_xq");
    expect(within(dialog).getByRole("checkbox", { name: t("connection.abr") })).not.toBeChecked();
  });
});
