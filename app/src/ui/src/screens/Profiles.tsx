// Profiles: the saved device + sound combinations as cards; the editor at
// #/profiles/new and #/profiles/edit/<name>.

import { createSignal, For, Show } from "solid-js";
import { useApp } from "../app/context";
import { Button, Dialog, IconButton } from "../components/controls";
import { Icon } from "../components/Icon";
import { t } from "../i18n";
import { codecSpec, codecSupport, qualityTag } from "../lib/codecs";
import { khz } from "../lib/format";
import { knownCodecs } from "../lib/profile";
import { knownSbcCaps, sbcKbpsText } from "../lib/sbc";
import type { Device, Profile } from "../ipc/types";
import { ProfileEditor } from "./ProfileEditor";

/** The quality and its bit rate; SBC and SBC XQ the bit rate their
 *  bitpool and frame setup give ("≈ 552 kbps") */
export function qualityText(p: Profile, devices: readonly Device[]): string | null {
  if (!p.le_audio && codecSpec(p.codec).bitpool) return sbcKbpsText(p, knownSbcCaps(devices, p.device_address));
  return qualityTag(p, t);
}

/** The sample rate, bit depth and ABR / VBR a profile asks for ("48 kHz",
 *  "24-bit", "ABR"); what is left on Auto is not listed */
export function formatTags(p: Profile): string[] {
  if (p.le_audio) return [];
  const spec = codecSpec(p.codec);
  const out: string[] = [];
  if (p.sample_rate > 0) out.push(`${khz(p.sample_rate)} kHz`);
  if (p.bit_depth > 0) out.push(t(`bitdepth.${p.bit_depth}`));
  if (p.abr && (spec.abr || spec.vbr)) out.push(spec.vbr ? "VBR" : "ABR");
  return out;
}

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
              return (
                <li class="card profile-card" classList={{ active: active() }}>
                  <div class="profile-card-head">
                    <Icon name="headphones" />
                    <div class="profile-card-title">
                      <h2 class="ellipsis">{p.name}</h2>
                      <span class="profile-card-device">
                        <span class="muted ellipsis">{p.device_name || p.device_address}</span>
                        <Show when={shown()}>
                          <span class={`tag st-${snap().stream.state}`}>{t(`status.${snap().stream.state}`)}</span>
                        </Show>
                      </span>
                    </div>
                    {/* Edit and delete with the name, out of the way of Connect (issue #86) */}
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
                  <div class="profile-card-meta">
                    {/* LE Audio (experimental): LC3, no codec choice */}
                    <Show when={p.le_audio} fallback={<span class="tag">{t(codecSpec(p.codec).label)}</span>}>
                      <span class="tag">{t("ui.codec.lc3")}</span>
                    </Show>
                    {/* The quality and its bit rate (v1's list) */}
                    <Show when={qualityText(p, snap().devices)}>{(q) => <span class="tag num">{q()}</span>}</Show>
                    {/* The device did not offer this codec when it last connected */}
                    <Show
                      when={
                        !p.le_audio && codecSupport(p.codec, knownCodecs(snap().devices, p.device_address)) === "not_offered"
                      }
                    >
                      <span class="tag">
                        <Icon name="warning" />
                        {t("ui.profile.codec_not_supported")}
                      </span>
                    </Show>
                    {/* The format it asks for; the capture source is the device's (Audio Devices) */}
                    <For each={formatTags(p)}>{(tag) => <span class="tag num">{tag}</span>}</For>
                  </div>
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
