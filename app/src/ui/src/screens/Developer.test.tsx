import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { t } from "../i18n";
import { MockCore } from "../ipc/mock";
import { createCore } from "../ipc/store";

let mock: MockCore | null = null;

afterEach(() => {
  cleanup();
  mock?.dispose();
  mock = null;
});

async function open() {
  location.hash = "#/developer";
  const calls: { method: string; params: unknown }[] = [];
  mock = new MockCore({ speed: 0, onCall: (method, params) => calls.push({ method, params }) });
  const core = createCore(mock);
  render(() => <App core={core} />);
  await screen.findByRole("navigation");
  return { core, calls };
}

describe("developer screen", () => {
  it("is hidden until developer mode is on", async () => {
    const { core } = await open();
    expect(screen.queryByRole("link", { name: "Developer" })).toBeNull();
    await core.request("settings.set", { settings: { ...core.state.snap!.settings, developer_mode: true } });
    expect(await screen.findByRole("link", { name: "Developer" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 1, name: "Developer" })).toBeInTheDocument();
  });

  it("saves the overrides and turns them all off", async () => {
    const { core, calls } = await open();
    await core.request("settings.set", { settings: { ...core.state.snap!.settings, developer_mode: true } });
    fireEvent.click(await screen.findByRole("switch", { name: new RegExp(t("ui.dev.rtp_marker")) }));
    await screen.findByText(t("ui.dev.active"));
    const sent = calls.filter((c) => c.method === "settings.set").pop()!.params as { settings: { dev_overrides: unknown } };
    expect(sent.settings.dev_overrides).toMatchObject({ rtp_marker: true, aac_bitrate_kbps: 0 });

    const bitrate = screen.getByLabelText(t("ui.dev.aac_bitrate")) as HTMLInputElement;
    fireEvent.input(bitrate, { target: { value: "9999" } });
    fireEvent.change(bitrate, { target: { value: "9999" } });
    await screen.findByDisplayValue("512");

    fireEvent.click(screen.getByRole("button", { name: t("ui.dev.reset") }));
    await screen.findByText(t("ui.dev.intro"));
    expect(core.state.snap!.settings.dev_overrides.rtp_marker).toBe(false);
  });
});
