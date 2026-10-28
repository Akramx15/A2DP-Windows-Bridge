// The spatial effect of a device's profile: the Sony 360 chain, the Upmixer
// chain or the plain (off) path, the stage that runs inside the chosen family,
// the folder the stages are found in, and the settings a stage declares.
//
// It is one part of the profile: every change is saved with the Codecs
// screen's saveEntry (so the profile's name follows the codec, and a device
// without a profile gets one), never with profile.save here. The engines come
// from one request per folder for the whole app (lib/spatial.ts): Home shows
// the same list for its badge, and no focus listener is needed.

import { createEffect, createMemo, createResource, createSignal, createUniqueId, For, on, Show } from "solid-js";
import { errorText, useApp } from "../app/context";
import { t } from "../i18n";
import type { SpatialEngine } from "../ipc/bindings/SpatialEngine";
import type { Profile } from "../ipc/types";
import { normalizeFolder, registryFolder } from "../lib/profile";
import { spatialEngines } from "../lib/spatial";
import { Banner, Button, Dialog, Field, Segmented, Select, Slider, type Option } from "./controls";

/** What the effect runs: a family of stages, or nothing */
type Mode = "sony" | "upmixer" | "off";

/** The stages of a family: an engine without one is a Sony stage (the first
 *  registries had no family) */
function inFamily(e: SpatialEngine, family: "sony" | "upmixer"): boolean {
  return family === "upmixer" ? e.family === "upmixer" : e.family === "sony" || e.family === "";
}

/** The labels of the stage controls the app knows, by their config key */
const CONTROL_KEYS: Record<string, string> = {
  center_focus: "spatial.control_center_focus",
  ambience_spread: "spatial.control_ambience_spread",
  correction: "spatial.control_correction",
};

/** The labels of the choices the app knows, by their value */
const CHOICE_KEYS: Record<string, string> = {
  none: "spatial.correction_none",
  "sony-xm6-ba": "spatial.correction_sony_xm6_ba",
  autoeq: "spatial.correction_autoeq",
};

/** A control's label: the app's own words for the ones it knows, the stage
 *  manifest's otherwise */
function controlLabel(key: string, label: string): string {
  const known = CONTROL_KEYS[key];
  return known ? t(known) : label;
}

function choiceLabel(value: string): string {
  const known = CHOICE_KEYS[value];
  return known ? t(known) : value;
}

export function SpatialStage(props: {
  profile: Profile;
  /** Saves the profile as the Codecs screen does (its name follows the codec) */
  save: (edit: (p: Profile) => Profile) => Promise<string | null>;
}) {
  const app = useApp();
  const core = app.core;
  const folder = createMemo(() => registryFolder(props.profile));
  // One list for the folder, shared with Home (lib/spatial.ts)
  const engines = () => spatialEngines(core, folder());
  // A resource that failed throws when read: its error is shown instead
  const list = () => {
    const resource = engines().engines;
    return resource.error ? [] : (resource() ?? []);
  };
  const loading = () => engines().engines.loading;
  const failure = () => engines().engines.error;
  const [saving, setSaving] = createSignal(false);
  const [said, setSaid] = createSignal("");
  const [controlsOpen, setControlsOpen] = createSignal(false);
  const titleId = createUniqueId();
  const busy = () => saving() || loading();

  /** The stage of a family to run: the one the profile has when it belongs
   *  to it, else its first stage */
  const stageFor = (family: "sony" | "upmixer"): SpatialEngine | undefined => {
    const mine = list().filter((e) => inFamily(e, family));
    return mine.find((e) => e.path === props.profile.spatial_engine_path) ?? mine[0];
  };
  const sony = () => stageFor("sony");
  const upmixer = () => stageFor("upmixer");

  /** What the profile runs now (before the list arrives, its saved path is
   *  a Sony one unless the profile says otherwise) */
  const mode = (): Mode => {
    if (!props.profile.spatial_engine) return "off";
    const current = list().find((e) => e.path === props.profile.spatial_engine_path);
    return current?.family === "upmixer" ? "upmixer" : "sony";
  };

  const stages = (): SpatialEngine[] =>
    mode() === "off" ? [] : list().filter((e) => inFamily(e, mode() === "upmixer" ? "upmixer" : "sony"));

  const apply = async (edit: (p: Profile) => Profile, done: string) => {
    if (saving()) return;
    setSaving(true);
    const name = await props.save(edit);
    setSaving(false);
    // saveEntry reports a failed save itself (app.run), and returns null
    if (name === null) return;
    setSaid(done);
    app.notify(done);
  };

  const choose = (next: Mode) => {
    if (next === "off") {
      void apply((p) => ({ ...p, spatial_engine: false }), t("profile.spatial_engine_saved"));
      return;
    }
    const stage = stageFor(next);
    if (!stage) return;
    void apply(
      (p) => ({ ...p, spatial_engine: true, spatial_engine_path: stage.path }),
      t("profile.spatial_engine_saved"),
    );
  };

  const chooseStage = (path: string) => {
    const stage = list().find((e) => e.path === path);
    if (!stage) return;
    void apply(
      (p) => ({ ...p, spatial_engine: true, spatial_engine_path: stage.path }),
      t("profile.spatial_engine_saved"),
    );
  };

  // ---- the engine folder ----
  // null: what the profile has saved (a save, or its change from elsewhere,
  // puts it back)
  const [draft, setDraft] = createSignal<string | null>(null);
  const folderText = () => draft() ?? props.profile.spatial_engine_registry;
  createEffect(on(() => props.profile.spatial_engine_registry, () => setDraft(null), { defer: true }));
  const folderChanged = () =>
    normalizeFolder(folderText()) !== normalizeFolder(props.profile.spatial_engine_registry);
  const saveFolder = (value: string) => {
    const next = normalizeFolder(value);
    setDraft(null);
    // The folder is the one it has: nothing to save
    if (next === normalizeFolder(props.profile.spatial_engine_registry)) return;
    void apply((p) => ({ ...p, spatial_engine_registry: next }), t("profile.spatial_engine_saved"));
  };

  const modes = (): Option<Mode>[] => [
    { value: "sony", label: t("profile.spatial_sony"), disabled: busy() || !sony() },
    { value: "upmixer", label: t("profile.spatial_upmixer"), disabled: busy() || !upmixer() },
    { value: "off", label: t("profile.spatial_off"), disabled: saving() },
  ];

  const empty = () => !loading() && !failure() && list().length === 0;

  return (
    <div
      class="spatial-stage"
      role="group"
      aria-labelledby={titleId}
      aria-busy={busy() ? "true" : undefined}
    >
      <h3 id={titleId} class="spatial-title">
        {t("profile.spatial_engine")}
      </h3>
      <p class="hint">{t("profile.spatial_engine_hint")}</p>

      <Show when={failure()}>
        <Banner
          kind="error"
          actions={
            <Button icon="arrow_sync" onClick={() => engines().refetch()}>
              {t("profile.spatial_engine_retry")}
            </Button>
          }
        >
          {t("profile.spatial_engine_failed", errorText(failure()))}
        </Banner>
      </Show>
      <Show when={loading()}>
        <p class="hint" role="status">
          {t("profile.spatial_engine_loading")}
        </p>
      </Show>

      <Segmented
        label={t("profile.spatial_engine")}
        value={mode()}
        options={modes()}
        onChange={choose}
      />

      <Show when={empty()}>
        <p class="hint" role="status">
          {t("profile.spatial_engine_none")}
        </p>
      </Show>
      <Show when={!loading() && !failure() && list().length > 0 && !upmixer()}>
        <Banner kind="warn">{t("profile.spatial_upmixer_missing")}</Banner>
      </Show>

      <Show when={mode() !== "off"}>
        <Field
          label={t("profile.spatial_engine_stage")}
          hint={stages().length === 0 ? t("profile.spatial_engine_none") : undefined}
        >
          {(id, described) => (
            <Select
              id={id}
              describedBy={described}
              value={stageFor(mode() === "upmixer" ? "upmixer" : "sony")?.path ?? ""}
              disabled={busy() || stages().length === 0}
              options={stages().map((e) => ({ value: e.path, label: e.label }))}
              onChange={(v: string) => chooseStage(v)}
            />
          )}
        </Field>
      </Show>

      <Field
        label={t("profile.spatial_engine_registry")}
        hint={
          folderText().trim()
            ? t("profile.spatial_engine_registry_hint")
            : t("profile.spatial_engine_registry_empty")
        }
      >
        {(id, described) => (
          <div class="inline wrap">
            <input
              id={id}
              class="input mono"
              type="text"
              value={folderText()}
              aria-describedby={described}
              disabled={busy()}
              onInput={(e) => setDraft(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key !== "Enter") return;
                e.preventDefault();
                if (folderChanged()) saveFolder(folderText());
              }}
            />
            <Button
              kind="subtle"
              disabled={busy() || !folderChanged()}
              onClick={() => saveFolder(folderText())}
            >
              {t("profile.spatial_engine_folder_save")}
            </Button>
            <Button kind="subtle" disabled={busy()} onClick={() => saveFolder("")}>
              {t("profile.spatial_engine_folder_default")}
            </Button>
          </div>
        )}
      </Field>

      <Show when={props.profile.spatial_engine && props.profile.name !== ""}>
        <div class="inline">
          <Button kind="subtle" onClick={() => setControlsOpen(true)}>
            {t("spatial.controls_open")}
          </Button>
        </div>
      </Show>

      <SpatialControls
        open={controlsOpen()}
        profile={props.profile}
        onClose={() => setControlsOpen(false)}
      />

      <span class="sr-only" role="status" aria-live="polite">
        {said()}
      </span>
    </div>
  );
}

/**
 * The settings a stage declares (spatial.controls), changed in one dialog and
 * written with spatial.set_controls: the core keeps them in the profile and
 * starts the stage again with them.
 */
function SpatialControls(props: { open: boolean; profile: Profile; onClose: () => void }) {
  const app = useApp();
  const name = () => (props.open && props.profile.spatial_engine ? props.profile.name : null);
  const [controls] = createResource(name, (profile) =>
    app.core.request("spatial.controls", { profile: profile ?? "" }),
  );
  const [draft, setDraft] = createSignal<Record<string, number | string>>({});
  const [saving, setSaving] = createSignal(false);
  const value = (key: string, fallback: number | string) => draft()[key] ?? fallback;
  const set = (key: string, v: number | string) => setDraft({ ...draft(), [key]: v });

  const save = async () => {
    const profile = name();
    const c = controls();
    if (!profile || saving()) return;
    const values: Record<string, number | string> = {};
    for (const s of c?.sliders ?? []) values[s.key] = Number(value(s.key, s.value));
    for (const s of c?.selects ?? []) values[s.key] = String(value(s.key, s.value));
    if (Object.keys(values).length === 0) {
      props.onClose();
      return;
    }
    setSaving(true);
    const ok = await app.run(() => app.core.request("spatial.set_controls", { profile, values }));
    setSaving(false);
    if (!ok) return;
    app.notify(t("spatial.controls_saved"));
    props.onClose();
  };

  return (
    <Dialog
      open={props.open}
      title={t("spatial.controls")}
      confirm={t("modal.save")}
      cancel={t("modal.cancel")}
      busy={saving()}
      onConfirm={() => void save()}
      onCancel={props.onClose}
    >
      <Show when={controls.error}>
        <Banner kind="error">{t("spatial.controls_failed", errorText(controls.error))}</Banner>
      </Show>
      <Show when={controls.loading}>
        <p class="hint" role="status">
          {t("spatial.controls_loading")}
        </p>
      </Show>
      <Show when={controls()}>
        {(c) => (
          <>
            <Show when={!c().supported}>
              <p class="hint">{t("spatial.controls_none")}</p>
            </Show>
            <For each={c().sliders}>
              {(s) => (
                <Slider
                  label={controlLabel(s.key, s.label)}
                  min={s.min}
                  max={s.max}
                  step={s.step}
                  value={Number(value(s.key, s.value))}
                  disabled={saving()}
                  onChange={(v) => set(s.key, v)}
                />
              )}
            </For>
            <For each={c().selects}>
              {(s) => (
                <Field label={controlLabel(s.key, s.label)}>
                  {(id, described) => (
                    <Select
                      id={id}
                      describedBy={described}
                      value={String(value(s.key, s.value))}
                      disabled={saving()}
                      options={s.options.map((o) => ({ value: o, label: choiceLabel(o) }))}
                      onChange={(v) => set(s.key, v)}
                    />
                  )}
                </Field>
              )}
            </For>
          </>
        )}
      </Show>
    </Dialog>
  );
}
