// Profiles: the saved device + sound combinations as cards; the editor at
// #/profiles/new and #/profiles/edit/<name>. Each card says what its
// profile is for: the device, the codec and the capture source in one
// line, the kind of use and the user's own note.

import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { useApp } from "../app/context";
import { Button, Dialog, IconButton } from "../components/controls";
import { Icon } from "../components/Icon";
import { t } from "../i18n";
import { codecSpec, codecSupport, qualityTag } from "../lib/codecs";
import { engineLabelOf, knownCodecs, profileSummary, purposeLabel } from "../lib/profile";
import { captureText } from "../components/CapturePicker";
import { registryFolder } from "../components/SpatialPicker";
import type { SpatialEngine } from "../ipc/types";
import { ProfileEditor } from "./ProfileEditor";

export function Profiles() {
  const app = useApp();
  return (
    <Show when={app.route().arg} keyed fallback={<ProfileList />}>
      {(arg) => <ProfileEditor original={arg === "new" ? null : arg} />}
    </Show>
  );
}

function ProfileList() {
  const app = useApp();
  const { core, navigate } = app;
  const snap = () => core.state.snap!;
  const [deleting, setDeleting] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  // The profile of the last start: its card shows the state (also an error)
  const streamingName = () => (snap().stream.state !== "idle" ? snap().stream.profile : null);
  // Still running (Disconnect / Cancel); after an error it connects again
  const running = () => ["streaming", "connecting", "reconnecting"].includes(snap().stream.state);

  const remove = async () => {
    const name = deleting();
    if (!name) return;
    setBusy(true);
    await app.run(() => core.request("profile.delete", { name }));
    setBusy(false);
    setDeleting(null);
  };

  // The engines of the folders the profiles use: one request for the whole
  // list, so a card can name the engine it runs
  const engineFolders = createMemo(() => {
    const folders = new Set<string>();
    for (const p of snap().profiles) {
      if (!p.spatial_engine) continue;
      const folder = registryFolder(p);
      if (folder) folders.add(folder);
    }
    return [...folders];
  });
  const [engines] = createResource(engineFolders, async (folders): Promise<SpatialEngine[]> => {
    const all: SpatialEngine[] = [];
    for (const folder of folders) {
      try {
        all.push(...((await core.request("spatial.engines", { path: folder })) ?? []));
      } catch {
        // No engine there (the core does not know the folder)
      }
    }
    return all;
  });

  return (
    <div class="page">
      <header class="page-head">
        <h1>{t("profile.header")}</h1>
        <Button kind="strong" icon="add" onClick={() => navigate({ name: "profiles", arg: "new" })}>
          {t("profile.new")}
        </Button>
      </header>
      <Show when={snap().profiles.length > 0} fallback={<p class="card empty">{t("profile.empty")}</p>}>
        <ul class="profile-grid">
          <For each={snap().profiles}>
            {(p) => {
              const active = () => p.name === snap().active_profile;
              const shown = () => p.name === streamingName();
              const live = () => shown() && running();
              // The engine the profile runs, by the label the core gives it
              const engine = () => (p.spatial_engine ? engineLabelOf(p, engines() ?? []) : "");
              return (
                <li class="card profile-card" classList={{ active: active() }}>
                  <div class="profile-card-head">
                    <Icon name="headphones" />
                    <div class="profile-card-title">
                      <h2 class="ellipsis">{p.name}</h2>
                      <span class="muted ellipsis">{profileSummary(p, t)}</span>
                    </div>
                    <Show when={shown()}>
                      <span class={`tag st-${snap().stream.state}`}>{t(`status.${snap().stream.state}`)}</span>
                    </Show>
                  </div>
                  <div class="profile-card-meta">
                    {/* What the profile is for: the user's own kind of use */}
                    <Show when={purposeLabel(p.purpose_kind, t)}>
                      {(kind) => <span class="tag tag-purpose">{kind()}</span>}
                    </Show>
                    {/* LE Audio (experimental): LC3, no codec choice */}
                    <Show when={p.le_audio} fallback={<span class="tag">{t(codecSpec(p.codec).label)}</span>}>
                      <span class="tag">{t("ui.codec.lc3")}</span>
                    </Show>
                    {/* The quality and its bit rate (v1's list) */}
                    <Show when={qualityTag(p, t)}>{(q) => <span class="tag num">{q()}</span>}</Show>
                    {/* The device did not offer this codec when it last connected */}
                    <Show
                      when={
                        !p.le_audio && codecSupport(p.codec, knownCodecs(snap().devices, p.device_address)) === "not_offered"
                      }
                    >
                      <span class="tag">
                        <Icon name="warning" />
                        {t("ui.profile.codec_not_offered")}
                      </span>
                    </Show>
                    <span class="tag">
                      <Icon name={captureText(p).icon} />
                      <span class="ellipsis">{captureText(p).text}</span>
                    </span>
                    {/* The captured audio runs through the spatial engine:
                        the stage that runs it, not the engine in general */}
                    <Show when={p.spatial_engine}>
                      <span class="tag" title={engine() || t("profile.spatial_engine")}>
                        <span class="ellipsis">{engine() || t("profile.spatial_engine")}</span>
                      </span>
                    </Show>
                  </div>
                  {/* The user's own words about this profile */}
                  <Show when={p.purpose_note.trim()}>
                    {(note) => (
                      <span class="muted ellipsis" title={note()}>
                        {note()}
                      </span>
                    )}
                  </Show>
                  <div class="profile-card-actions">
                    <Button
                      kind={live() ? "default" : "strong"}
                      icon={live() ? "plug_disconnected" : "power"}
                      onClick={() =>
                        app.run(() =>
                          live() ? core.request("stream.stop") : core.request("stream.start", { profile: p.name }),
                        )
                      }
                    >
                      {live() ? t("connection.disconnect") : t("profile.connect")}
                    </Button>
                    <IconButton
                      icon="edit"
                      label={`${t("profile.edit")}: ${p.name}`}
                      onClick={() => navigate({ name: "profiles", arg: p.name })}
                    />
                    <IconButton
                      icon="delete"
                      label={`${t("profile.delete")}: ${p.name}`}
                      disabled={live()}
                      onClick={() => setDeleting(p.name)}
                    />
                  </div>
                </li>
              );
            }}
          </For>
        </ul>
      </Show>
      <Dialog
        open={deleting() !== null}
        title={t("modal.confirm_title")}
        confirm={t("profile.delete")}
        cancel={t("modal.cancel")}
        danger
        busy={busy()}
        onConfirm={remove}
        onCancel={() => setDeleting(null)}
      >
        <p>{t("modal.confirm_delete_profile")}</p>
        <p>
          <strong>{deleting()}</strong>
        </p>
      </Dialog>
    </div>
  );
}
