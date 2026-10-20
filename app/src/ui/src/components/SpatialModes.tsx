import { createMemo, createResource, createSignal, onCleanup, onMount, Show } from "solid-js";
import { unwrap } from "solid-js/store";
import { useApp } from "../app/context";
import { t } from "../i18n";
import type { SpatialEngine } from "../ipc/bindings/SpatialEngine";
import type { Profile } from "../ipc/types";
import { Button } from "./controls";
import { registryFolder } from "./SpatialPicker";

/** The stage of a family the user picks: the current one when it belongs to
 * the family, otherwise the first stage of it. */
export function stageOf(engines: readonly SpatialEngine[], family: string): SpatialEngine | undefined {
  return engines.find((e) => e.family === family);
}

/** The three choices of the effect, as the Linux app has them: the Sony
 * chain, the Upmixer chain and the original path. */
export function SpatialModes(props: { profile: Profile }) {
  const app = useApp();
  const { core } = app;
  const [saving, setSaving] = createSignal(false);
  const folder = createMemo(() => registryFolder(props.profile));
  const [engines, { refetch }] = createResource(folder, async (path): Promise<SpatialEngine[]> => {
    if (!path) return [];
    try {
      return ((await core.request("spatial.engines", { path })) ?? []) as SpatialEngine[];
    } catch {
      return [];
    }
  });
  const load = () => void refetch();
  onMount(() => window.addEventListener("focus", load));
  onCleanup(() => window.removeEventListener("focus", load));

  const all = createMemo(() => engines() ?? []);
  const sony = createMemo(() => {
    const list = all().filter((e) => e.family === "sony" || !e.family);
    const current = list.find((e) => props.profile.spatial_engine && e.path === props.profile.spatial_engine_path);
    return current ?? list[0];
  });
  const upmixer = createMemo(() => all().find((e) => e.family === "upmixer"));
  const isCurrent = (e?: SpatialEngine) =>
    !!e && props.profile.spatial_engine && e.path === props.profile.spatial_engine_path;
  const isOff = () => !props.profile.spatial_engine;

  const apply = (next: Partial<Profile>) => {
    const name = props.profile.name;
    if (!name || saving()) return;
    const profile = { ...structuredClone(unwrap(props.profile)), ...next };
    setSaving(true);
    void app
      .run(async () => {
        await core.request("profile.save", { profile, original_name: name });
        const stream = core.state.snap?.stream;
        const st = stream?.state;
        if (stream?.profile === name && (st === "connecting" || st === "reconnecting" || st === "streaming")) {
          await core.request("stream.stop");
          await core.request("stream.start", { profile: name });
        }
      })
      .finally(() => setSaving(false));
  };

  const choose = (e?: SpatialEngine) => {
    if (!e) return;
    apply({ spatial_engine: true, spatial_engine_path: e.path });
  };

  return (
    <div class="inline wrap" role="group" aria-label={t("profile.spatial_engine")}>
      <Button
        kind={isCurrent(sony()) ? "strong" : "subtle"}
        disabled={saving() || !sony()}
        aria-pressed={isCurrent(sony())}
        onClick={() => choose(sony())}
        title={sony()?.label}
      >
        {t("profile.spatial_sony")}
      </Button>
      <Button
        kind={isCurrent(upmixer()) ? "strong" : "subtle"}
        disabled={saving() || !upmixer()}
        aria-pressed={isCurrent(upmixer())}
        onClick={() => choose(upmixer())}
        title={upmixer()?.label ?? t("profile.spatial_upmixer_missing")}
      >
        {t("profile.spatial_upmixer")}
      </Button>
      <Button
        kind={isOff() ? "strong" : "subtle"}
        disabled={saving()}
        aria-pressed={isOff()}
        onClick={() => apply({ spatial_engine: false })}
      >
        {t("profile.spatial_off")}
      </Button>
      <Show when={!upmixer()}>
        <span class="hint">{t("profile.spatial_upmixer_missing")}</span>
      </Show>
    </div>
  );
}
