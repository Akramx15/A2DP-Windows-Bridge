// The effect of a device's profile, right on its card (Home) and in the
// flyout: the family it runs (Sony, Upmixer, off), the profile it belongs to,
// and — while the Upmixer runs — the sliders that stage declares
// (center focus, ambience spread) with spatial.set_controls.
//
// It is one part of the profile, saved as the Codecs screen saves it
// (saveEntry through the `save` prop): a device without a profile gets one.
// The engines come from one request per folder for the whole app
// (lib/spatial.ts), the same list the Codecs screen shows.

import { createEffect, createResource, createSignal, For, on, onCleanup, Show } from "solid-js";
import { errorText, useApp } from "../app/context";
import { t } from "../i18n";
import type { SpatialEngine } from "../ipc/bindings/SpatialEngine";
import type { Profile } from "../ipc/types";
import { profilesOf, registryFolder, type AudioEntry } from "../lib/profile";
import { choiceLabel, controlLabel, spatialEngines, stageLabel } from "../lib/spatial";
import { Banner, Button, Segmented, Select, Slider, type Option } from "./controls";

/** What the effect runs: a family of stages, or nothing */
type Mode = "sony" | "upmixer" | "off";

/** The stages of a family: an engine without one is a Sony stage (the first
 *  registries had no family) */
function inFamily(e: SpatialEngine, family: "sony" | "upmixer"): boolean {
  return family === "upmixer" ? e.family === "upmixer" : e.family === "sony" || e.family === "";
}

export function DeviceEffects(props: {
  entry: AudioEntry;
  /** Saves the profile as the Codecs screen does (its name follows the codec) */
  save: (edit: (p: Profile) => Profile) => Promise<string | null>;
}) {
  const app = useApp();
  const profile = () => props.entry.profile;
  const name = () => profile()?.name ?? "";
  // The folder the profile lists its stages in, empty: the core's own
  const folder = () => registryFolder(profile() ?? { spatial_engine_registry: "" });
  const engines = () => spatialEngines(app.core, folder());
  // A resource that failed throws when read: treated as no engine at all
  const list = () => {
    const resource = engines().engines;
    return resource.error ? [] : (resource() ?? []);
  };
  const [saving, setSaving] = createSignal(false);
  const busy = () => saving() || engines().engines.loading;

  const stageFor = (family: "sony" | "upmixer"): SpatialEngine | undefined => {
    const mine = list().filter((e) => inFamily(e, family));
    return mine.find((e) => e.path === profile()?.spatial_engine_path) ?? mine[0];
  };

  /** What the profile runs now (before the list arrives, its saved path is a
   *  Sony one unless the profile says otherwise) */
  const mode = (): Mode => {
    const p = profile();
    if (!p?.spatial_engine) return "off";
    return list().find((e) => e.path === p.spatial_engine_path)?.family === "upmixer" ? "upmixer" : "sony";
  };

  const choose = async (next: Mode) => {
    if (saving()) return;
    const stage = next === "off" ? null : stageFor(next);
    if (next !== "off" && !stage) return;
    setSaving(true);
    // A failed save is reported by app.run, through the Codecs screen's save
    await props.save((p) =>
      next === "off"
        ? { ...p, spatial_engine: false }
        : { ...p, spatial_engine: true, spatial_engine_path: stage!.path },
    );
    setSaving(false);
  };

  const stages = (): SpatialEngine[] =>
    mode() === "off" ? [] : list().filter((e) => inFamily(e, mode() as "sony" | "upmixer"));
  const chooseStage = async (path: string) => {
    if (saving() || !path) return;
    setSaving(true);
    await props.save((p) => ({ ...p, spatial_engine: true, spatial_engine_path: path }));
    setSaving(false);
  };

  const options = (): Option<Mode>[] => [
    { value: "sony", label: t("profile.spatial_sony"), disabled: busy() || !stageFor("sony") },
    { value: "upmixer", label: t("profile.spatial_upmixer"), disabled: busy() || !stageFor("upmixer") },
    { value: "off", label: t("profile.spatial_off"), disabled: saving() },
  ];

  return (
    <div
      class="effects"
      role="group"
      aria-label={t("profile.spatial_engine")}
      aria-busy={busy() ? "true" : undefined}
    >
      <div class="effect-row">
        <Segmented label={t("profile.spatial_engine")} value={mode()} options={options()} onChange={(m) => void choose(m)} />
        <ProfilePick entry={props.entry} />
      </div>
      {/* The stage that runs inside the family: the Sony chain has several
          (XM6 BA, HPS, reference), and the Upmixer has its own */}
      <Show when={stages().length > 1}>
        <Select
          label={t("profile.spatial_stage")}
          value={profile()?.spatial_engine_path ?? ""}
          disabled={busy()}
          options={stages().map((e) => ({ value: e.path, label: stageLabel(e) }))}
          onChange={(path) => void chooseStage(path)}
        />
      </Show>
      {/* The Upmixer's own settings: only while it runs, and only when the
          stage declares some (spatial.controls) */}
      <Show when={mode() === "upmixer" && name()}>
        <EffectSliders profile={name()} />
      </Show>
    </div>
  );
}

/** The sliders a stage declares, written as they move (one write per drag,
 *  after a moment): the core keeps them in the profile and starts the stage
 *  again with them */
function EffectSliders(props: { profile: string }) {
  const app = useApp();
  const [controls, { refetch }] = createResource(
    () => props.profile,
    (profile) => app.core.request("spatial.controls", { profile }),
  );
  const [draft, setDraft] = createSignal<Record<string, number | string>>({});
  const value = (key: string, fallback: number | string) => draft()[key] ?? fallback;
  const [saving, setSaving] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let mounted = true;
  onCleanup(() => {
    mounted = false;
    clearTimeout(timer);
  });
  // Another profile (or none): the values shown are that one's
  createEffect(on(() => props.profile, () => setDraft({}), { defer: true }));

  const write = async () => {
    const values = draft();
    if (!mounted || Object.keys(values).length === 0) return;
    setSaving(true);
    await app.run(() => app.core.request("spatial.set_controls", { profile: props.profile, values }));
    setSaving(false);
  };
  const putChoice = (key: string, v: string) => {
    setDraft({ ...draft(), [key]: v });
    clearTimeout(timer);
    timer = setTimeout(() => void write(), 400);
  };
  const put = (key: string, v: number) => {
    setDraft({ ...draft(), [key]: v });
    clearTimeout(timer);
    timer = setTimeout(() => void write(), 400);
  };

  return (
    <div class="effect-controls" role="group" aria-label={t("spatial.controls")} aria-busy={saving() ? "true" : undefined}>
      <Show when={controls.error}>
        <Banner
          kind="error"
          actions={
            <Button icon="arrow_sync" onClick={() => refetch()}>
              {t("profile.spatial_engine_retry")}
            </Button>
          }
        >
          {t("spatial.controls_failed", errorText(controls.error))}
        </Banner>
      </Show>
      <Show when={controls()}>
        {(c) => (
          <>
          <For each={c().selects}>
            {(s) => (
              <Select
                label={controlLabel(s.key, s.label)}
                value={String(draft()[s.key] ?? s.value)}
                disabled={saving()}
                options={s.options.map((o) => ({ value: o, label: choiceLabel(o) }))}
                onChange={(v) => putChoice(s.key, v)}
              />
            )}
          </For>
          <Show when={c().supported && c().sliders.length > 0}>
            <For each={c().sliders}>
              {(s) => (
                <Slider
                  label={controlLabel(s.key, s.label)}
                  min={s.min}
                  max={s.max}
                  step={s.step}
                  value={Number(value(s.key, s.value))}
                  disabled={saving()}
                  onChange={(v) => setDraft({ ...draft(), [s.key]: v })}
                  onCommit={(v) => put(s.key, v)}
                />
              )}
            </For>
          </Show>
          </>
        )}
      </Show>
    </div>
  );
}

/** The profile the device runs: its own (usually one); choosing another makes
 *  it the active one (profile.set_active), as the device chooser does */
function ProfilePick(props: { entry: AudioEntry }) {
  const app = useApp();
  const snap = () => app.core.state.snap!;
  const mine = () => profilesOf(snap().profiles, props.entry.address).filter((p) => p.le_audio === props.entry.le_audio);
  const value = () => props.entry.profile?.name ?? "";
  const choose = (name: string) => {
    if (name && name !== value()) void app.run(() => app.core.request("profile.set_active", { name }));
  };
  return (
    <Show when={mine().length > 0}>
      <div class="effect-profile">
        <Select
          label={t("ui.home.profile")}
          value={value()}
          disabled={mine().length < 2}
          options={mine().map((p) => ({ value: p.name, label: p.name }))}
          onChange={choose}
        />
      </div>
    </Show>
  );
}
