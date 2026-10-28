// The spatial effect on the Codecs screen (components/SpatialStage.tsx): the
// three modes, the stage of the chosen family, the engine folder, the stage's
// own settings, and the badge Home shows. Each save goes through the device's
// profile (saveEntry), not profile.save from the component.

import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../app/App";
import { setLanguage, t } from "../i18n";
import type { SpatialEngine } from "../ipc/bindings/SpatialEngine";
import { MockCore } from "../ipc/mock";
import { createCore } from "../ipc/store";

let mock: MockCore | null = null;

const SONY: SpatialEngine = {
  id: "sony_native",
  label: "Sony native (reference)",
  path: "C:\\A2DPWB\\engines\\sony_native\\sony_native.exe",
  family: "sony",
};
const HPP: SpatialEngine = {
  id: "personalized_hpp",
  label: "Personalized HPP + XM6 BA",
  path: "C:\\A2DPWB\\engines\\personalized_hpp\\PersonalizedHPP.exe",
  family: "sony",
};
const UPMIXER: SpatialEngine = {
  id: "upmixer",
  label: "Upmixer + personal ear file (experimental)",
  path: "C:\\A2DPWB\\engines\\upmixer\\Upmixer.exe",
  family: "upmixer",
};

/** The core's answer to spatial.engines replaced, so an empty folder and a
 *  missing Upmixer can be shown (the mock lists the same three stages) */
function withEngines(base: MockCore, engines: SpatialEngine[] | Error): MockCore {
  return new Proxy(base, {
    get(target, key) {
      if (key === "call") {
        return (method: string, params?: unknown) =>
          method === "spatial.engines"
            ? engines instanceof Error
              ? Promise.reject(engines)
              : Promise.resolve(engines)
            : target.call(method, params);
      }
      const value = Reflect.get(target, key);
      return typeof value === "function" ? value.bind(target) : value;
    },
  }) as MockCore;
}

function start(hash: string, engines?: SpatialEngine[] | Error) {
  location.hash = hash;
  const calls: { method: string; params: unknown }[] = [];
  const base = new MockCore({ speed: 0, onCall: (method, params) => calls.push({ method, params }) });
  mock = base;
  const core = createCore(engines ? withEngines(base, engines) : base);
  render(() => <App core={core} />);
  return { calls, core };
}

/** A device's section on the Codecs screen */
const section = (name: string) => screen.findByRole("region", { name });
/** The effect's block in it */
const effect = (where: HTMLElement) => within(where).findByRole("group", { name: t("profile.spatial_engine") });
/** The folder's stages are read: a mode can be chosen */
const ready = (block: HTMLElement) =>
  waitFor(() => expect(within(block).getByRole("radio", { name: t("profile.spatial_sony") })).not.toBeDisabled(), {
    timeout: 3000,
  });
/** The profiles saved, in order */
const saves = (calls: { method: string; params: unknown }[]) =>
  calls
    .filter((c) => c.method === "profile.save")
    .map((c) => c.params as { profile: Record<string, unknown>; original_name: string | null });
/** The saves so far, waited for (the suite runs many screens at once) */
const saved = (calls: { method: string; params: unknown }[], n: number) =>
  waitFor(() => expect(saves(calls)).toHaveLength(n), { timeout: 4000 });

afterEach(async () => {
  cleanup();
  mock?.dispose();
  mock = null;
  localStorage.clear();
  await setLanguage("en");
});

describe("the spatial effect", () => {
  it("runs the Sony chain for a device, saved through its profile", async () => {
    const { calls } = start("#/codecs");
    const wh = await section("WH-1000XM5");
    const block = await effect(wh);
    const sony = () => within(block).getByRole("radio", { name: t("profile.spatial_sony") });
    // Its stages are read before one can be chosen
    await waitFor(() => expect(sony()).not.toBeDisabled());
    expect(within(block).getByRole("radio", { name: t("profile.spatial_upmixer") })).not.toBeDisabled();

    fireEvent.click(sony());
    await saved(calls, 1);
    // The profile's own name (it follows the codec), not a new one
    expect(saves(calls)[0]).toMatchObject({
      profile: { name: "WH-1000XM5 (LDAC)", spatial_engine: true, spatial_engine_path: SONY.path },
      original_name: "WH-1000XM5 (LDAC)",
    });
  });

  it("chooses the stage that runs inside the family", async () => {
    const { calls } = start("#/codecs");
    const wh = await section("WH-1000XM5");
    const block = await effect(wh);
    const sony = () => within(block).getByRole("radio", { name: t("profile.spatial_sony") });
    await waitFor(() => expect(sony()).not.toBeDisabled());
    fireEvent.click(sony());
    await saved(calls, 1);

    const stage = await within(block).findByLabelText(t("profile.spatial_engine_stage"));
    // The first save is done before another change is taken
    await waitFor(() => expect(stage).not.toBeDisabled(), { timeout: 3000 });
    await waitFor(() =>
      expect(within(stage).getAllByRole("option").map((o) => o.textContent)).toEqual([SONY.label, HPP.label]),
    );
    fireEvent.change(stage, { target: { value: HPP.path } });
    await saved(calls, 2);
    expect(saves(calls)[1].profile).toMatchObject({ spatial_engine: true, spatial_engine_path: HPP.path });
    expect(saves(calls)[1].original_name).toBe("WH-1000XM5 (LDAC)");

    // The other family runs its own stage
    const upmixer = () => within(block).getByRole("radio", { name: t("profile.spatial_upmixer") });
    await waitFor(() => expect(upmixer()).not.toBeDisabled());
    fireEvent.click(upmixer());
    await saved(calls, 3);
    expect(saves(calls)[2].profile).toMatchObject({ spatial_engine: true, spatial_engine_path: UPMIXER.path });
  });

  it("turns the effect off again, keeping the stage", async () => {
    const { calls } = start("#/codecs");
    const wh = await section("WH-1000XM5");
    const block = await effect(wh);
    const sony = () => within(block).getByRole("radio", { name: t("profile.spatial_sony") });
    await waitFor(() => expect(sony()).not.toBeDisabled());
    fireEvent.click(sony());
    await saved(calls, 1);

    const off = () => within(block).getByRole("radio", { name: t("profile.spatial_off") });
    await waitFor(() => expect(off()).not.toBeDisabled());
    fireEvent.click(off());
    await saved(calls, 2);
    expect(saves(calls)[1].profile).toMatchObject({ spatial_engine: false });
  });

  it("cannot run the Upmixer when no stage of it is installed, and says so", async () => {
    start("#/codecs", [SONY, HPP]);
    const block = await effect(await section("WH-1000XM5"));
    expect(await within(block).findByText(t("profile.spatial_upmixer_missing"))).toBeInTheDocument();
    expect(within(block).getByRole("radio", { name: t("profile.spatial_upmixer") })).toBeDisabled();
    expect(within(block).getByRole("radio", { name: t("profile.spatial_sony") })).not.toBeDisabled();
  });

  it("says the folder holds no engine instead of showing an empty list", async () => {
    start("#/codecs", []);
    const block = await effect(await section("WH-1000XM5"));
    expect(await within(block).findByText(t("profile.spatial_engine_none"))).toBeInTheDocument();
    expect(within(block).queryByText(t("profile.spatial_upmixer_missing"))).toBeNull();
    expect(within(block).getByRole("radio", { name: t("profile.spatial_sony") })).toBeDisabled();
  });

  it("shows a folder the core cannot read as an error, with a way to ask again", async () => {
    start("#/codecs", new Error("unreadable"));
    const block = await effect(await section("WH-1000XM5"));
    const retry = await within(block).findByRole("button", { name: t("profile.spatial_engine_retry") });
    expect(retry).toBeInTheDocument();
    expect(within(block).queryByText(t("profile.spatial_engine_none"))).toBeNull();
  });

  it("writes the engine folder and the default one through the profile", async () => {
    const { calls } = start("#/codecs");
    const block = await effect(await section("WH-1000XM5"));
    await ready(block);
    const box = within(block).getByLabelText(t("profile.spatial_engine_registry"));
    // A path copied from a JSON file: its doubled separators go
    fireEvent.input(box, { target: { value: "C:\\\\Engines\\\\sony" } });
    fireEvent.click(within(block).getByRole("button", { name: t("profile.spatial_engine_folder_save") }));
    await saved(calls, 1);
    expect(saves(calls)[0].profile).toMatchObject({ spatial_engine_registry: "C:\\Engines\\sony" });

    // The core's own folder: no path at all
    const reset = () => within(block).getByRole("button", { name: t("profile.spatial_engine_folder_default") });
    await waitFor(() => expect(reset()).not.toBeDisabled());
    fireEvent.click(reset());
    await saved(calls, 2);
    expect(saves(calls)[1].profile).toMatchObject({ spatial_engine_registry: "" });
  });

  it("shows the effect on the device's Home card", async () => {
    const { calls } = start("#/codecs");
    const block = await effect(await section("WH-1000XM5"));
    const sony = () => within(block).getByRole("radio", { name: t("profile.spatial_sony") });
    await waitFor(() => expect(sony()).not.toBeDisabled());
    fireEvent.click(sony());
    await saved(calls, 1);

    location.hash = "#/home";
    const card = await screen.findByRole("group", { name: "WH-1000XM5" });
    expect(await within(card).findByText(SONY.label)).toBeInTheDocument();
  });

  it("shows the stage's own settings and writes them with spatial.set_controls", async () => {
    const { calls } = start("#/codecs");
    const block = await effect(await section("WH-1000XM5"));
    const sony = () => within(block).getByRole("radio", { name: t("profile.spatial_sony") });
    await waitFor(() => expect(sony()).not.toBeDisabled());
    fireEvent.click(sony());
    await saved(calls, 1);

    const controls = await within(block).findByRole("button", { name: t("spatial.controls_open") });
    fireEvent.click(controls);
    const dialog = await screen.findByRole("dialog", { name: t("spatial.controls") });
    const slider = await within(dialog).findByRole("slider", { name: t("spatial.control_center_focus") });
    fireEvent.input(slider, { target: { value: "0.8" } });
    const choice = within(dialog).getByLabelText(t("spatial.control_correction"));
    expect(within(choice).getAllByRole("option").map((o) => o.textContent)).toEqual([
      t("spatial.correction_none"),
      t("spatial.correction_sony_xm6_ba"),
      t("spatial.correction_autoeq"),
    ]);
    fireEvent.click(within(dialog).getByRole("button", { name: t("modal.save") }));
    await waitFor(() => expect(calls.some((c) => c.method === "spatial.set_controls")).toBe(true));
    expect(calls.find((c) => c.method === "spatial.set_controls")?.params).toEqual({
      profile: "WH-1000XM5 (LDAC)",
      values: { center_focus: 0.8, correction: "none" },
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
