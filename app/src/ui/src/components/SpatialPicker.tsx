import { createMemo, createResource, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { unwrap } from "solid-js/store";
import { useApp } from "../app/context";
import { t } from "../i18n";
import type { SpatialEngine } from "../ipc/bindings/SpatialEngine";
import type { Profile } from "../ipc/types";
import { Button } from "./controls";

export function registryFolder(
  p: Pick<Profile, "spatial_engine_registry" | "spatial_engine_path">,
): string {
  const registry = p.spatial_engine_registry.trim();
  if (registry) return registry;
  const path = p.spatial_engine_path.trim();
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut > 0 ? path.slice(0, cut) : "";
}

export function SpatialPicker(props: { profile: Profile; onPick?: (engine: SpatialEngine) => void }) {
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

  const current = (e: SpatialEngine) =>
    props.profile.spatial_engine && e.path === props.profile.spatial_engine_path;

  const choose = (engine: SpatialEngine) => {
    if (props.onPick) {
      props.onPick(engine);
      return;
    }
    const name = props.profile.name;
    if (!name || current(engine)) return;
    const next = {
      ...structuredClone(unwrap(props.profile)),
      spatial_engine: true,
      spatial_engine_path: engine.path,
    };
    setSaving(true);
    void app
      .run(async () => {
        await core.request("profile.save", { profile: next, original_name: name });
        const stream = core.state.snap?.stream;
        const st = stream?.state;
        if (stream?.profile === name && (st === "connecting" || st === "reconnecting" || st === "streaming")) {
          await core.request("stream.stop");
          await core.request("stream.start", { profile: name });
        }
      })
      .finally(() => setSaving(false));
  };

  return (
    <Show when={folder()} fallback={<p class="hint">{t("profile.spatial_engine_set_folder")}</p>}>
      <Show
        when={(engines() ?? []).length > 0}
        fallback={
          <Show when={!engines.loading}>
            <p class="hint">{t("profile.spatial_engine_none")}</p>
          </Show>
        }
      >
        <div class="inline wrap" role="group" aria-label={t("profile.spatial_engine")}>
          <For each={engines()}>
            {(engine) => (
              <Button
                kind={current(engine) ? "strong" : "subtle"}
                disabled={saving()}
                aria-pressed={current(engine)}
                onClick={() => choose(engine)}
              >
                {engine.label}
              </Button>
            )}
          </For>
        </div>
      </Show>
    </Show>
  );
}
