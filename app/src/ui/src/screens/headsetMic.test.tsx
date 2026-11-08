// Headset microphone (developer mode): Off / Auto / On and the output on
// each device of the Codecs screen, saved with the device's profile, the
// mute chip on Home's card and in the flyout, and the voice codec beside
// the codec while a voice link runs

import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { setLanguage, t } from "../i18n";
import { MockCore, type MockScenario } from "../ipc/mock";
import type { Profile } from "../ipc/types";
import { createCore } from "../ipc/store";
import { endSettle } from "./Codecs";

let mock: MockCore | null = null;

function start(hash: string, scenario: MockScenario = "normal", window?: "flyout") {
  location.hash = hash;
  const calls: { method: string; params: unknown }[] = [];
  mock = new MockCore({ scenario, speed: 0, ...(window ? { window } : {}), onCall: (method, params) => calls.push({ method, params }) });
  const core = createCore(mock);
  const view = render(() => <App core={core} />);
  const saves = () =>
    calls.filter((c) => c.method === "profile.save").map((c) => (c.params as { profile: Profile }).profile);
  return { calls, core, view, saves };
}

afterEach(async () => {
  cleanup();
  endSettle();
  mock?.dispose();
  mock = null;
  await setLanguage("en");
});

const CABLE = "{0.0.0.00000000}.{cable}";
const VOICEMEETER = "{0.0.0.00000000}.{voicemeeter}";
const micModes = (card: HTMLElement) => within(card).queryByRole("radiogroup", { name: t("ui.headset_mic") });
const mode = (card: HTMLElement, m: "off" | "auto" | "on") =>
  within(micModes(card)!).getByRole("radio", { name: t(`ui.headset_mic_${m}`) });
const MUTE = () => t("ui.headset_mic_mute");
const UNMUTE = () => t("ui.headset_mic_unmute");

describe("Codecs screen", () => {
  it("has no headset microphone outside developer mode", async () => {
    start("#/codecs");
    const wh = await screen.findByRole("region", { name: "WH-1000XM5" });
    expect(micModes(wh)).toBeNull();
  });

  it("saves the mode and the output with the device's profile", async () => {
    const { saves } = start("#/codecs", "voice");
    const xb = await screen.findByRole("region", { name: "SRS-XB13" });
    expect(mode(xb, "off")).toBeChecked();
    expect(within(xb).queryByRole("combobox", { name: t("ui.mic_output") })).toBeNull();
    fireEvent.click(mode(xb, "on"));
    await waitFor(() => expect(saves().at(-1)).toMatchObject({ device_name: "SRS-XB13", headset_mic: "on", mic_device_id: "" }));
    // On without an output: said so, and the call quality of a classic device
    await within(xb).findByText(t("ui.headset_mic_no_output"));
    expect(within(xb).getByText(t("ui.headset_mic_quality"))).toBeInTheDocument();
    expect(within(xb).queryByText(t("ui.headset_mic_auto_hint"))).toBeNull();
    const select = within(xb).getByRole("combobox", { name: t("ui.mic_output") }) as HTMLSelectElement;
    // The virtual devices first
    const names = [...select.options].map((o) => o.textContent);
    expect(names.indexOf("CABLE Input (VB-Audio Virtual Cable)")).toBeLessThan(names.indexOf("Speakers (Realtek(R) Audio)"));
    const i = [...select.options].findIndex((o) => o.value === VOICEMEETER);
    select.selectedIndex = i;
    fireEvent.change(select);
    await waitFor(() =>
      expect(saves().at(-1)).toMatchObject({
        device_name: "SRS-XB13",
        headset_mic: "on",
        mic_device_id: VOICEMEETER,
        mic_device_name: "Voicemeeter Input (VB-Audio Voicemeeter VAIO)",
      }),
    );
    await waitFor(() => expect(within(xb).queryByText(t("ui.headset_mic_no_output"))).toBeNull());
  });

  it("keeps the output for Auto, with its hint, and hides it for Off", async () => {
    const { saves } = start("#/codecs", "voice");
    const wh = await screen.findByRole("region", { name: "WH-1000XM5" });
    expect(mode(wh, "on")).toBeChecked();
    fireEvent.click(mode(wh, "auto"));
    await waitFor(() => expect(saves().at(-1)).toMatchObject({ device_name: "WH-1000XM5", headset_mic: "auto", mic_device_id: CABLE }));
    await within(wh).findByText(t("ui.headset_mic_auto_hint"));
    expect(mode(wh, "auto")).toBeChecked();
    expect(within(wh).getByRole("combobox", { name: t("ui.mic_output") })).toBeInTheDocument();
    expect(within(wh).getByText(t("ui.headset_mic_quality"))).toBeInTheDocument();
    fireEvent.click(mode(wh, "off"));
    await waitFor(() => expect(saves().at(-1)).toMatchObject({ headset_mic: "off", mic_device_id: CABLE }));
    await waitFor(() => expect(within(wh).queryByRole("combobox", { name: t("ui.mic_output") })).toBeNull());
    expect(within(wh).queryByText(t("ui.headset_mic_auto_hint"))).toBeNull();
  });

  it("warns of call quality only for a classic device", async () => {
    start("#/codecs", "voice");
    const wh = await screen.findByRole("region", { name: "WH-1000XM5" });
    expect(mode(wh, "on")).toBeChecked();
    expect(within(wh).getByText(t("ui.headset_mic_quality"))).toBeInTheDocument();
    const select = within(wh).getByRole("combobox", { name: t("ui.mic_output") }) as HTMLSelectElement;
    expect(select.value).toBe(CABLE);
    const le = await screen.findByRole("region", { name: "Galaxy Buds2 Pro" });
    expect(mode(le, "on")).toBeChecked();
    expect(within(le).queryByText(t("ui.headset_mic_quality"))).toBeNull();
  });
});

describe("status", () => {
  const homeCard = (name: string) => screen.findByRole("group", { name });

  it("shows the voice codec beside the codec, in the warning colour on a classic link", async () => {
    const { core } = start("#/home", "voice");
    const wh = await homeCard("WH-1000XM5");
    await core.request("stream.start", { profile: "WH-1000XM5 (LDAC)" });
    const voice = await waitFor(() => {
      const v = wh.querySelector(".voice-codec");
      expect(v).not.toBeNull();
      return v!;
    });
    expect(voice).toHaveTextContent(t("status.voice_codec", t("codec.msbc")));
    expect(voice).toHaveClass("st-warn");
    // The sidebar's status too
    expect(document.querySelector(".status-pill .voice-codec")).toHaveTextContent("Voice: mSBC");
  });

  it("drops the voice codec when headset mode is turned off while streaming", async () => {
    const { core } = start("#/codecs", "voice");
    await core.request("stream.start", { profile: "WH-1000XM5 (LDAC)" });
    await waitFor(() => expect(core.state.snap?.stream.voice?.codec).toBe("msbc"));
    const wh = await screen.findByRole("region", { name: "WH-1000XM5" });
    fireEvent.click(mode(wh, "off"));
    await waitFor(() => expect(core.state.snap?.stream.voice ?? null).toBeNull());
    expect(core.state.snap?.stream.state).toBe("streaming");
  });

  it("shows LE Audio's voice codec without the warning", async () => {
    const { core } = start("#/home", "voice");
    const le = await homeCard("Galaxy Buds2 Pro");
    await core.request("stream.start", { profile: "Buds (LE Audio)" });
    const voice = await waitFor(() => {
      const v = le.querySelector(".voice-codec");
      expect(v).not.toBeNull();
      return v!;
    });
    expect(voice).toHaveTextContent("Voice: LC3");
    expect(voice).not.toHaveClass("st-warn");
  });
});

describe("headset chip", () => {
  const homeCard = (name: string) => screen.findByRole("group", { name });

  it("is not on Home outside developer mode", async () => {
    start("#/home");
    const wh = await homeCard("WH-1000XM5");
    expect(wh.querySelector(".headset-chip")).toBeNull();
  });

  it("is only on the cards of devices with a microphone output", async () => {
    start("#/home", "voice");
    const wh = await homeCard("WH-1000XM5");
    await within(wh).findByRole("button", { name: MUTE() });
    const xb = await homeCard("SRS-XB13");
    expect(xb.querySelector(".headset-chip")).toBeNull();
  });

  it("mutes and unmutes on Home, keeping the mode chosen on Codecs", async () => {
    const { saves } = start("#/home", "voice");
    const wh = await homeCard("WH-1000XM5");
    const on = await within(wh).findByRole("button", { name: MUTE() });
    expect(on).toHaveTextContent(t("ui.headset_mic_on"));
    expect(on).toHaveClass("on");
    fireEvent.click(on);
    await waitFor(() =>
      expect(saves().at(-1)).toMatchObject({ device_name: "WH-1000XM5", headset_mic: "on", mic_muted: true }),
    );
    const muted = await within(wh).findByRole("button", { name: UNMUTE() });
    expect(muted).toHaveTextContent(t("ui.headset_mic_muted"));
    expect(muted).toHaveClass("muted");
    expect(muted).not.toHaveClass("on");
    // Each click once the last save is done
    await waitFor(() => expect(muted).toBeEnabled());
    fireEvent.click(muted);
    await waitFor(() => expect(saves().at(-1)).toMatchObject({ headset_mic: "on", mic_muted: false }));
    await within(wh).findByRole("button", { name: MUTE() });
  });

  it("shows Auto, waiting with no app recording, and is gone for Off", async () => {
    start("#/codecs", "voice");
    const card = await screen.findByRole("region", { name: "WH-1000XM5" });
    fireEvent.click(mode(card, "auto"));
    await waitFor(() => expect(mode(card, "auto")).toBeChecked());
    location.hash = "#/home";
    const wh = await homeCard("WH-1000XM5");
    const auto = await within(wh).findByRole("button", { name: MUTE() });
    expect(auto).toHaveTextContent(t("ui.headset_mic_auto"));
    expect(auto).toHaveClass("waiting");
    location.hash = "#/codecs";
    fireEvent.click(mode(await screen.findByRole("region", { name: "WH-1000XM5" }), "off"));
    location.hash = "#/home";
    const again = await homeCard("WH-1000XM5");
    await waitFor(() => expect(again.querySelector(".headset-chip")).toBeNull());
  });

  it("closes the voice link while muted and unmutes from Codecs", async () => {
    const { core, saves } = start("#/home", "voice");
    const wh = await homeCard("WH-1000XM5");
    await core.request("stream.start", { profile: "WH-1000XM5 (LDAC)" });
    await waitFor(() => expect(core.state.snap?.stream.voice?.codec).toBe("msbc"));
    const chip = await within(wh).findByRole("button", { name: MUTE() });
    await waitFor(() => expect(chip).toHaveClass("linked"));
    fireEvent.click(chip);
    await waitFor(() => expect(core.state.snap?.stream.voice ?? null).toBeNull());
    expect(core.state.snap?.stream.state).toBe("streaming");
    // Choosing a mode on Codecs unmutes
    location.hash = "#/codecs";
    const card = await screen.findByRole("region", { name: "WH-1000XM5" });
    fireEvent.click(mode(card, "auto"));
    await waitFor(() => expect(saves().at(-1)).toMatchObject({ headset_mic: "auto", mic_muted: false }));
  });

  it("is in the flyout and saves from there", async () => {
    const { core, saves } = start("", "voice", "flyout");
    await core.request("stream.start", { profile: "WH-1000XM5 (LDAC)" });
    const chip = await screen.findByRole("button", { name: MUTE() });
    expect(chip.closest(".fly-state")).not.toBeNull();
    fireEvent.click(chip);
    await waitFor(() => expect(saves().at(-1)).toMatchObject({ device_name: "WH-1000XM5", mic_muted: true }));
    await screen.findByRole("button", { name: UNMUTE() });
  });

  it("is not in the flyout outside developer mode", async () => {
    const { core } = start("", "normal", "flyout");
    await core.request("stream.start", { profile: "WH-1000XM5 (LDAC)" });
    await waitFor(() => expect(core.state.snap?.stream.state).not.toBe("idle"));
    expect(document.querySelector(".headset-chip")).toBeNull();
  });
});
