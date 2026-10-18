// Choosing the capture source right where it shows (Home, the tray flyout):
// System Loopback, a virtual device or a playing app, without opening the
// profile editor. The choice is saved into the profile; a profile in use
// starts again with it (profile.save in the core).

import { createMemo, createResource, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { unwrap } from "solid-js/store";
import { useApp } from "../app/context";
import { t } from "../i18n";
import type { AudioApp, AudioDevice, Profile } from "../ipc/types";
import { appCaptureSupport, normalizeProfile } from "../lib/profile";
import { Icon } from "./Icon";
import type { IconName } from "./icons";
import { CaptureSource, sourceText } from "./stream";

/** The profile's capture source, as the profile list and an idle Home show it */
export function captureText(p: Profile): { icon: IconName; text: string } {
  switch (p.capture_mode) {
    case "virtual":
      return { icon: "desktop_speaker", text: p.audio_device_name || t("capture.virtual") };
    case "app":
      return { icon: "window_apps", text: p.app_name || p.app_exe || t("capture.app") };
    default:
      return { icon: "speaker_2", text: t("capture.loopback") };
  }
}

interface Choice {
  value: string;
  label: string;
}

/** The value of the profile's capture source among the choices */
export function captureValue(p: Profile): string {
  if (p.capture_mode === "virtual") return `virtual:${p.audio_device_id}`;
  if (p.capture_mode === "app") return `app:${p.app_exe.toLowerCase()}`;
  return "loopback";
}

/** The choices: virtual devices and playing apps, the profile's own also when
 *  it is not there now. No apps for LE Audio (error.le_audio_capture) or on a
 *  Windows that cannot capture one app. */
export function captureChoices(
  p: Profile,
  devices: readonly AudioDevice[],
  apps: readonly AudioApp[],
  appsOk: boolean,
): { virtual: Choice[]; apps: Choice[] } {
  const virtual = devices.filter((d) => d.is_virtual).map((d) => ({ value: `virtual:${d.id}`, label: d.name }));
  if (p.capture_mode === "virtual" && p.audio_device_id && !virtual.some((c) => c.value === captureValue(p))) {
    virtual.unshift({
      value: captureValue(p),
      label: t("capture.volume_sync_missing", p.audio_device_name || p.audio_device_id),
    });
  }
  const list: Choice[] = [];
  if (!p.le_audio && appsOk) {
    for (const a of apps) {
      const value = `app:${a.exe.toLowerCase()}`;
      if (!list.some((c) => c.value === value)) list.push({ value, label: a.name });
    }
  }
  if (p.capture_mode === "app" && p.app_exe && !list.some((c) => c.value === captureValue(p))) {
    list.unshift({ value: captureValue(p), label: t("capture.app_not_playing", p.app_name || p.app_exe) });
  }
  return { virtual, apps: list };
}

/** The profile with the chosen capture source */
export function withCapture(
  p: Profile,
  value: string,
  devices: readonly AudioDevice[],
  apps: readonly AudioApp[],
): Profile {
  if (value.startsWith("virtual:")) {
    const id = value.slice("virtual:".length);
    const d = devices.find((x) => x.id === id);
    return { ...p, capture_mode: "virtual", audio_device_id: id, audio_device_name: d?.name ?? p.audio_device_name };
  }
  if (value.startsWith("app:")) {
    const exe = value.slice("app:".length);
    if (p.app_exe.toLowerCase() === exe) return { ...p, capture_mode: "app" };
    const a = apps.find((x) => x.exe.toLowerCase() === exe);
    return { ...p, capture_mode: "app", app_exe: a?.exe ?? exe, app_name: a?.name ?? exe };
  }
  return { ...p, capture_mode: "loopback" };
}

export function CapturePicker(props: { profile: Profile }) {
  const app = useApp();
  const { core } = app;
  const [devices, setDevices] = createSignal<AudioDevice[]>([]);
  const [apps, setApps] = createSignal<AudioApp[]>([]);
  const [saving, setSaving] = createSignal(false);
  const [appSupport] = createResource(appCaptureSupport);

  // Asked again whenever the window comes back: apps start and stop playing
  const load = () => {
    void core
      .request("audio.devices")
      .then(setDevices)
      .catch(() => {});
    void core
      .request("audio.apps")
      .then(setApps)
      .catch(() => {});
  };
  onMount(() => {
    load();
    window.addEventListener("focus", load);
  });
  onCleanup(() => window.removeEventListener("focus", load));

  const choices = createMemo(() => captureChoices(props.profile, devices(), apps(), appSupport() !== "no"));
  const stream = () => core.state.snap?.stream;
  // A stream of something else (the test tone, another profile): what it
  // captures, not a choice for this profile
  const other = () => {
    const s = stream();
    return s?.info?.source && s.profile !== props.profile.name ? s.info.source : null;
  };
  // While this profile streams: what the core captures ("waiting for <app>" included)
  const shown = () => {
    const src = stream()?.info?.source;
    if (src && !other()) return sourceText(src, props.profile);
    return { ...captureText(props.profile), waiting: false };
  };

  let select!: HTMLSelectElement;
  const choose = (value: string) => {
    if (value === captureValue(props.profile)) return;
    const next = normalizeProfile(withCapture(structuredClone(unwrap(props.profile)), value, devices(), apps()));
    setSaving(true);
    void app
      .run(() => core.request("profile.save", { profile: next, original_name: props.profile.name }))
      .then((ok) => {
        // Not saved: back to what the profile has
        if (!ok) select.value = captureValue(props.profile);
      })
      .finally(() => setSaving(false));
  };

  return (
    <Show when={!other()} fallback={<CaptureSource profile={props.profile} />}>
      <div class="capture-picker" classList={{ "st-warn": shown().waiting }} title={`${t("capture.mode")}: ${shown().text}`}>
        <div class="capture-picker-row">
          <Icon name={shown().waiting ? "warning" : shown().icon} />
          <div class="select">
            <select ref={select} aria-label={t("capture.mode")} disabled={saving()} onChange={(e) => choose(e.currentTarget.value)}>
              <option value="loopback" selected={captureValue(props.profile) === "loopback"}>
                {t("capture.loopback")}
              </option>
              <Show when={choices().virtual.length > 0}>
                <optgroup label={t("capture.virtual")}>
                  <For each={choices().virtual}>
                    {(c) => (
                      <option value={c.value} selected={c.value === captureValue(props.profile)}>
                        {c.label}
                      </option>
                    )}
                  </For>
                </optgroup>
              </Show>
              <Show when={choices().apps.length > 0}>
                <optgroup label={t("capture.app")}>
                  <For each={choices().apps}>
                    {(c) => (
                      <option value={c.value} selected={c.value === captureValue(props.profile)}>
                        {c.label}
                      </option>
                    )}
                  </For>
                </optgroup>
              </Show>
            </select>
            <Icon name="chevron_down" class="select-chevron" />
          </div>
        </div>
        <Show when={shown().waiting}>
          <span class="capture-waiting ellipsis">{shown().text}</span>
        </Show>
      </div>
    </Show>
  );
}
