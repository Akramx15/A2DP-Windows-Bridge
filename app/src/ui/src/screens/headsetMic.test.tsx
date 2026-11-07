// Headset microphone (developer mode): the switch and its output on each
// device of the Codecs screen, saved with the device's profile, and the
// voice codec beside the codec while a voice link runs

import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { setLanguage, t } from "../i18n";
import { MockCore, type MockScenario } from "../ipc/mock";
import type { Profile } from "../ipc/types";
import { createCore } from "../ipc/store";
import { endSettle } from "./Codecs";

let mock: MockCore | null = null;

function start(hash: string, scenario: MockScenario = "normal") {
  location.hash = hash;
  const calls: { method: string; params: unknown }[] = [];
  mock = new MockCore({ scenario, speed: 0, onCall: (method, params) => calls.push({ method, params }) });
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
const micSwitch = (card: HTMLElement) => within(card).queryByRole("switch", { name: t("ui.headset_mic") });

describe("Codecs screen", () => {
  it("has no headset microphone outside developer mode", async () => {
    start("#/codecs");
    const wh = await screen.findByRole("region", { name: "WH-1000XM5" });
    expect(micSwitch(wh)).toBeNull();
  });

  it("saves the switch and the output with the device's profile", async () => {
    const { saves } = start("#/codecs", "voice");
    const xb = await screen.findByRole("region", { name: "SRS-XB13" });
    const sw = micSwitch(xb)!;
    expect(sw).not.toBeChecked();
    expect(within(xb).queryByRole("combobox", { name: t("ui.mic_output") })).toBeNull();
    fireEvent.click(sw);
    await waitFor(() => expect(saves().at(-1)).toMatchObject({ device_name: "SRS-XB13", headset_mic: true, mic_device_id: "" }));
    // On without an output: said so, and the call quality of a classic device
    await within(xb).findByText(t("ui.headset_mic_no_output"));
    expect(within(xb).getByText(t("ui.headset_mic_quality"))).toBeInTheDocument();
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
        headset_mic: true,
        mic_device_id: VOICEMEETER,
        mic_device_name: "Voicemeeter Input (VB-Audio Voicemeeter VAIO)",
      }),
    );
    await waitFor(() => expect(within(xb).queryByText(t("ui.headset_mic_no_output"))).toBeNull());
  });

  it("warns of call quality only for a classic device", async () => {
    start("#/codecs", "voice");
    const wh = await screen.findByRole("region", { name: "WH-1000XM5" });
    expect(micSwitch(wh)).toBeChecked();
    expect(within(wh).getByText(t("ui.headset_mic_quality"))).toBeInTheDocument();
    const select = within(wh).getByRole("combobox", { name: t("ui.mic_output") }) as HTMLSelectElement;
    expect(select.value).toBe(CABLE);
    const le = await screen.findByRole("region", { name: "Galaxy Buds2 Pro" });
    expect(micSwitch(le)).toBeChecked();
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
    fireEvent.click(micSwitch(wh)!);
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
