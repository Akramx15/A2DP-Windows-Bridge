import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { setLanguage } from "../i18n";
import { MockCore, type MockOptions } from "../ipc/mock";
import { createCore } from "../ipc/store";

let mock: MockCore | null = null;

function start(hash: string, opts: MockOptions = {}) {
  location.hash = hash;
  const calls: { method: string; params: unknown }[] = [];
  mock = new MockCore({ speed: 0, ...opts, onCall: (method, params) => calls.push({ method, params }) });
  const core = createCore(mock);
  const view = render(() => <App core={core} />);
  return { core, calls, view };
}

afterEach(async () => {
  cleanup();
  mock?.dispose();
  mock = null;
  await setLanguage("en");
});

/** A device's card on Home (its button is "Connect" on every card) */
const homeCard = (name: string) => screen.findByRole("group", { name });

describe("Home", () => {
  it("connects and disconnects with the device's own button", async () => {
    const { calls } = start("#/home");
    const wh = await homeCard("WH-1000XM5");
    const xb = await homeCard("SRS-XB13");
    fireEvent.click(within(wh).getByRole("button", { name: "Connect" }));
    // The mock goes connecting -> streaming
    const disconnect = await within(wh).findByRole("button", { name: "Disconnect" });
    expect(calls.find((c) => c.method === "stream.start")?.params).toEqual({ profile: "WH-1000XM5 (LDAC)" });
    await waitFor(() => expect(wh).toHaveTextContent("990 kbps"));
    // The badge on Home has the bit depth too
    expect(wh.querySelector(".codec-badge")).toHaveTextContent("24-bit");
    // The other device: ready, not streaming, its own Connect
    expect(xb).toHaveTextContent("Ready");
    expect(xb).toHaveTextContent("Not streaming");
    expect(within(xb).getByRole("button", { name: "Connect" })).toBeInTheDocument();
    // The volume is the device's own: on its card only (issue #86)
    expect(within(wh).getByRole("slider", { name: "Volume" })).not.toBeDisabled();
    expect(within(xb).queryByRole("slider")).toBeNull();
    expect(screen.getAllByRole("slider", { name: "Volume" })).toHaveLength(1);
    fireEvent.click(disconnect);
    await within(wh).findByRole("button", { name: "Connect" });
    expect(calls.some((c) => c.method === "stream.stop")).toBe(true);
  });

  it("switches to another device with its Connect (one device streams at a time)", async () => {
    const { calls } = start("#/home");
    const wh = await homeCard("WH-1000XM5");
    const xb = await homeCard("SRS-XB13");
    fireEvent.click(within(wh).getByRole("button", { name: "Connect" }));
    await within(wh).findByRole("button", { name: "Disconnect" });
    fireEvent.click(within(xb).getByRole("button", { name: "Connect" }));
    await within(xb).findByRole("button", { name: "Disconnect" });
    expect(calls.filter((c) => c.method === "stream.start").map((c) => c.params)).toEqual([
      { profile: "WH-1000XM5 (LDAC)" },
      { profile: "Desk speaker (AAC)" },
    ]);
    // stream.start stops the running stream itself
    expect(calls.some((c) => c.method === "stream.stop")).toBe(false);
    expect(within(wh).getByRole("button", { name: "Connect" })).toBeInTheDocument();
  });

  it("gives a device without a profile one (Auto) when connected", async () => {
    const { calls } = start("#/home");
    await homeCard("WH-1000XM5");
    await mock!.call("device.rename", { address: "54:2F:04:4B:29:DC", name: "moto buds+" });
    const moto = await homeCard("moto buds+");
    fireEvent.click(within(moto).getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(calls.some((c) => c.method === "stream.start")).toBe(true));
    expect(calls.find((c) => c.method === "profile.save")?.params).toMatchObject({
      profile: { name: "moto buds+ | Auto", codec: "auto", device_address: "54:2F:04:4B:29:DC" },
      original_name: null,
    });
    expect(calls.find((c) => c.method === "stream.start")?.params).toEqual({ profile: "moto buds+ | Auto" });
  });

  it("shows each device, and the codec and its details of the one streaming, in the sidebar on every screen (issue #86)", async () => {
    const { core } = start("#/settings");
    const pills = () => [...document.querySelectorAll(".status-pill")] as HTMLElement[];
    await waitFor(() => expect(pills()).toHaveLength(2));
    const [wh, xb] = pills();
    expect(wh).toHaveTextContent("WH-1000XM5");
    expect(wh).not.toHaveTextContent("AC:80:0A:12:34:56");
    expect(wh).toHaveTextContent("Idle");
    expect(xb).toHaveTextContent("SRS-XB13");
    await core.request("stream.start", { profile: "WH-1000XM5 (LDAC)" });
    await waitFor(() => expect(wh.querySelector(".status-pill-codec")).toHaveTextContent("LDAC"));
    expect(wh).toHaveTextContent("Streaming");
    // One line: "LDAC 990 kbps 96 kHz 24-bit"
    expect(wh.querySelector(".status-pill-details")).toHaveTextContent(/^990 kbps [\d.]+ kHz 24-bit$/);
    expect(wh).toHaveAccessibleName(/^WH-1000XM5, .*, LDAC 990 kbps/);
    // The other one: idle, no codec
    expect(xb).toHaveTextContent("Idle");
    expect(xb.querySelector(".status-pill-codec")).toBeNull();
  });

  it("asks to exit the other A2DPWB (v1, or another v2) while it runs", async () => {
    start("#/home");
    await homeCard("WH-1000XM5");
    expect(screen.queryByText("Another A2DPWB is running — exit it first")).toBeNull();
    mock!.setV1Running(true);
    expect(await screen.findByText("Another A2DPWB is running — exit it first")).toBeInTheDocument();
    mock!.setV1Running(false);
    await waitFor(() => expect(screen.queryByText("Another A2DPWB is running — exit it first")).toBeNull());
  });

  it("shows the cause of an error and a fix", async () => {
    start("#/home", { scenario: "error" });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The device did not answer");
    expect(within(alert).getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });

  it("connects a device again after an error instead of offering Disconnect", async () => {
    const { calls } = start("#/home", { scenario: "error" });
    const wh = await homeCard("WH-1000XM5");
    expect(wh).toHaveTextContent("Error");
    expect(within(wh).queryByRole("button", { name: "Disconnect" })).toBeNull();
    fireEvent.click(within(wh).getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(calls.some((c) => c.method === "stream.start")).toBe(true));
    expect(calls.some((c) => c.method === "stream.stop")).toBe(false);
  });
});

describe("every screen renders", () => {
  const routes: [string, string][] = [
    ["#/home", "Home"],
    ["#/codecs", "Codecs"],
    // The old Profiles links lead to Codecs
    ["#/profiles", "Codecs"],
    ["#/profiles/new", "Codecs"],
    ["#/devices", "Audio Devices"],
    ["#/diagnostics", "Diagnostics"],
    ["#/adapter", "Adapters"],
    ["#/about", "About"],
    ["#/settings", "Settings"],
  ];
  for (const [hash, heading] of routes) {
    it(hash, async () => {
      start(hash);
      expect(await screen.findByRole("heading", { level: 1, name: heading })).toBeInTheDocument();
      expect(screen.getByRole("navigation")).toBeInTheDocument();
    });
  }

  it("diagnostics tabs", async () => {
    start("#/diagnostics");
    for (const name of ["Connection Flow", "Log", "Link Quality"]) {
      fireEvent.click(await screen.findByRole("tab", { name }));
      expect(screen.getByRole("tab", { name })).toHaveAttribute("aria-selected", "true");
    }
    expect(screen.queryByRole("tab", { name: "Peer Receiver Test" })).toBeNull();
  });

  it("the first-run setup when no adapter is on WinUSB", async () => {
    const { calls } = start("#/home", { scenario: "fresh" });
    expect(await screen.findByRole("heading", { level: 1, name: "Set up A2DPWB" })).toBeInTheDocument();
    mock!.setV1Running(true);
    expect(await screen.findByText("Another A2DPWB is running — exit it first")).toBeInTheDocument();
    // The setup waits while the other one holds the adapter
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
    mock!.setV1Running(false);
    await waitFor(() => expect(screen.getByRole("button", { name: "Next" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    // Step 2: one explanation and one button, which switches the driver and
    // downloads the firmware (issue #86)
    const setUp = await screen.findByRole("button", { name: "Set up this adapter" });
    expect(screen.getByText(/Windows then stops using this adapter/)).toBeInTheDocument();
    expect(screen.getByText("rtl8761bu_fw.bin, rtl8761bu_config.bin")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Assign WinUSB" })).toBeNull();
    // Another A2DPWB started meanwhile: no driver change until it exits
    mock!.setV1Running(true);
    await waitFor(() => expect(setUp).toBeDisabled());
    mock!.setV1Running(false);
    await waitFor(() => expect(setUp).toBeEnabled());
    fireEvent.click(setUp);
    await screen.findByText("The adapter is ready for A2DPWB", {}, { timeout: 5000 });
    const order = calls.map((c) => c.method).filter((m) => m === "adapter.assign_winusb" || m === "adapter.download_firmware");
    expect(order).toEqual(["adapter.assign_winusb", "adapter.download_firmware"]);
    expect(screen.getByRole("button", { name: "Next" })).not.toBeDisabled();
  });

  it("the flyout", async () => {
    start("", { window: "flyout" });
    expect(await screen.findByRole("button", { name: "Connect" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open app" })).toBeInTheDocument();
  });

  it("in Japanese", async () => {
    const { core } = start("#/settings");
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    await core.request("settings.set", { settings: { ...core.state.snap!.settings, language: "ja" } });
    expect(await screen.findByRole("link", { name: "ホーム" })).toBeInTheDocument();
  });
});

describe("update", () => {
  it("installs at once when nothing streams", async () => {
    start("#/settings", { scenario: "update" });
    fireEvent.click(await screen.findByRole("button", { name: "Update now" }));
    await screen.findByText(/Installing version 2\.0\.0-beta\.1/);
  });

  it("waits for the stream to stop", async () => {
    start("#/settings", { scenario: "update" });
    await mock!.call("stream.start", { profile: "WH-1000XM5 (LDAC)" });
    await waitFor(() => expect(mock!["snap"].stream.state).toBe("streaming"));
    fireEvent.click(await screen.findByRole("button", { name: "Update now" }));
    await screen.findByText(/It is installed when streaming stops/);
    await mock!.call("stream.stop");
    await screen.findByText(/Installing version 2\.0\.0-beta\.1/, {}, { timeout: 3000 });
  });
});
