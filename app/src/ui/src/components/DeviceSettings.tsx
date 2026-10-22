// One audio device's own settings on the Audio Devices screen (issue #86):
// what is captured for it and the Windows volume that follows it (kept the
// same in all its profiles: the core reads them from the profile it
// streams), whether its buttons control playback and whether its volume is
// remembered (Settings.device_options; the global value until changed).

import { createEffect, createMemo, createResource, createSignal, on, Show } from "solid-js";
import { createStore, unwrap } from "solid-js/store";
import { useApp } from "../app/context";
import { saveSettings } from "../app/settings";
import { t } from "../i18n";
import type { DeviceOptions } from "../ipc/bindings/DeviceOptions";
import type { AudioApp, AudioDevice, Device, Profile } from "../ipc/types";
import { LINKS } from "../lib/links";
import {
  appCaptureSupport,
  captureDevice,
  deviceFields,
  deviceFieldsComplete,
  formatMismatch,
  normalizeProfile,
  profilesOf,
  type DeviceFields,
} from "../lib/profile";
import { Banner, Button, Field, IconButton, Segmented, Select, Toggle } from "./controls";
import { Icon } from "./Icon";

export function DeviceSettings(props: { device: Device }) {
  const app = useApp();
  const { core, navigate } = app;
  const snap = () => core.state.snap!;
  const profiles = createMemo(() => profilesOf(snap().profiles, props.device.address));
  // What is shown: the first profile's, until changed here
  const [f, setF] = createStore<DeviceFields>(deviceFields(newestBase(profiles())));
  createEffect(on(profiles, (list) => list[0] && setF(deviceFields(list[0])), { defer: true }));
  const [saving, setSaving] = createSignal(false);

  // ---- Audio devices and apps (asked from the core when needed) ----------
  const [devices, setDevices] = createSignal<AudioDevice[] | null>(null);
  const [apps, setApps] = createSignal<AudioApp[] | null>(null);
  const loadDevices = () =>
    core
      .request("audio.devices")
      .then(setDevices)
      .catch(() => setDevices([]));
  const loadApps = () =>
    core
      .request("audio.apps")
      .then(setApps)
      .catch(() => setApps([]));
  createEffect(
    on(
      () => [f.capture_mode, f.volume_sync] as const,
      ([mode, sync]) => {
        // Loopback and virtual device: the capture device's format line
        if ((mode !== "app" || sync === "device") && devices() === null) void loadDevices();
        if (mode === "app" && apps() === null) void loadApps();
      },
    ),
  );
  const virtualDevices = createMemo(() => (devices() ?? []).filter((d) => d.is_virtual));
  const [appSupport] = createResource(appCaptureSupport);
  const leOnly = () => profiles().length > 0 && profiles().every((p) => p.le_audio);

  /** Changes the device's settings: into each of its profiles once complete
   *  (a virtual device or an app chosen); a profile in use starts again with
   *  them (profile.save in the core) */
  const apply = async (patch: Partial<DeviceFields>) => {
    setF(patch);
    const next = { ...unwrap(f) };
    if (!deviceFieldsComplete(next)) return;
    setSaving(true);
    for (const p of profiles()) {
      // An LE Audio profile cannot capture one application: it keeps its own
      if (p.le_audio && next.capture_mode === "app") continue;
      const updated: Profile = normalizeProfile({ ...structuredClone(unwrap(p)), ...next });
      if (JSON.stringify(deviceFields(updated)) === JSON.stringify(deviceFields(p))) continue;
      const ok = await app.run(() => core.request("profile.save", { profile: updated, original_name: p.name }));
      if (!ok) break;
    }
    setSaving(false);
  };

  // ---- Buttons and remembered volume: per device, else the global value ----
  const key = () => props.device.address.toUpperCase();
  const options = () => snap().settings.device_options?.[key()] ?? null;
  const mediaKeys = () => options()?.media_keys ?? snap().settings.media_keys;
  const rememberVolume = () => options()?.remember_volume ?? snap().settings.remember_volume;
  const setOption = (patch: Partial<DeviceOptions>) => {
    const all = { ...(unwrap(snap().settings.device_options) ?? {}) };
    all[key()] = { ...(all[key()] ?? { media_keys: null, remember_volume: null }), ...patch };
    void saveSettings(app, { device_options: all });
  };

  return (
    <div class="device-settings" aria-busy={saving() ? "true" : undefined}>
      <Show
        when={profiles().length > 0}
        fallback={
          <p class="hint">
            {t("ui.devices.no_profile")}{" "}
            <button type="button" class="link" onClick={() => navigate({ name: "profiles", arg: "new" })}>
              {t("profile.new")}
            </button>
          </p>
        }
      >
        <div class="field">
          <span class="field-label">{t("capture.mode")}</span>
          <Segmented
            label={t("capture.mode")}
            value={f.capture_mode}
            options={[
              { value: "loopback", label: t("capture.loopback"), icon: "speaker_2" },
              { value: "virtual", label: t("capture.virtual"), icon: "desktop_speaker" },
              { value: "app", label: t("capture.app"), icon: "window_apps", disabled: leOnly() },
            ]}
            onChange={(v) => void apply({ capture_mode: v })}
          />
          <p class="hint">
            {f.capture_mode === "virtual"
              ? t("capture.hint_virtual")
              : f.capture_mode === "app"
                ? t("capture.hint_app")
                : t("capture.hint_loopback")}
          </p>
        </div>
        <CaptureFormat fields={f} profiles={profiles()} devices={devices()} />
        <Show when={f.capture_mode === "virtual"}>
          <Field
            label={t("capture.audio_device")}
            hint={t("tooltip.audio_device")}
            error={f.audio_device_id === "" && devices() !== null ? t("ui.devices.pick_virtual") : null}
          >
            {(id, described) => (
              <Show
                when={virtualDevices().length > 0 || devices() === null}
                fallback={
                  <div class="inline">
                    <span class="muted">{t("capture.no_virtual_device")}</span>
                    <Button icon="open" onClick={() => core.transport.openUrl(LINKS.vbCable)}>
                      {t("vbcable.download")}
                    </Button>
                  </div>
                }
              >
                <Select
                  id={id}
                  describedBy={described}
                  value={f.audio_device_id}
                  options={[
                    { value: "", label: "—", disabled: true },
                    ...virtualDevices().map((d) => ({ value: d.id, label: d.name })),
                  ]}
                  onChange={(v) => {
                    const d = virtualDevices().find((x) => x.id === v);
                    void apply({ audio_device_id: v, audio_device_name: d?.name ?? "" });
                  }}
                />
              </Show>
            )}
          </Field>
          <Toggle
            label={t("capture.auto_switch")}
            hint={t("tooltip.auto_switch")}
            checked={f.auto_switch_device}
            onChange={(v) => void apply({ auto_switch_device: v })}
          />
        </Show>
        <Show when={f.capture_mode === "app"}>
          {/* Whether this Windows can capture one app: only the core knows
              the build for sure (it refuses the stream on an older Windows); the UI asks
              the WebView's client hints and shows a hint without them */}
          <Show
            when={appSupport() === "no"}
            fallback={
              <Show when={appSupport() !== "yes"}>
                <p class="hint">{t("ui.profile.app_capture_requirement")}</p>
              </Show>
            }
          >
            <Banner kind="warn">{t("ui.profile.app_capture_unsupported")}</Banner>
          </Show>
          <Field
            label={t("capture.app_select")}
            hint={t("capture.app_hint")}
            error={f.app_exe === "" && apps() !== null ? t("ui.devices.pick_app") : null}
          >
            {(id, described) => (
              <div class="inline">
                <Select
                  id={id}
                  describedBy={described}
                  value={f.app_exe}
                  disabled={appSupport() === "no"}
                  options={appOptions(apps(), f)}
                  onChange={(v) => {
                    const a = (apps() ?? []).find((x) => x.exe === v);
                    void apply({ app_exe: v, app_name: a?.name ?? v });
                  }}
                />
                <IconButton
                  icon="arrow_sync"
                  label={t("capture.app_refresh")}
                  disabled={appSupport() === "no"}
                  onClick={() => void loadApps()}
                />
              </div>
            )}
          </Field>
        </Show>
        <Field label={t("capture.volume_sync")} hint={t("tooltip.volume_sync")}>
          {(id, described) => (
            <Select
              id={id}
              describedBy={described}
              value={f.volume_sync === "device" ? `device:${f.volume_device_id}` : f.volume_sync}
              options={[
                { value: "auto", label: t("capture.volume_sync_auto") },
                { value: "off", label: t("capture.volume_sync_off") },
                ...volumeDevices(devices(), f).map((d) => ({ value: `device:${d.id}`, label: d.name })),
              ]}
              onChange={(v) => {
                if (v.startsWith("device:")) {
                  const did = v.slice("device:".length);
                  const d = (devices() ?? []).find((x) => x.id === did);
                  void apply({ volume_sync: "device", volume_device_id: did, volume_device_name: d?.name ?? f.volume_device_name });
                } else {
                  void apply({ volume_sync: v });
                  if (devices() === null) void loadDevices();
                }
              }}
            />
          )}
        </Field>
      </Show>
      <Toggle label={t("settings.media_keys")} checked={mediaKeys()} onChange={(v) => setOption({ media_keys: v })} />
      <Toggle
        label={t("settings.remember_volume")}
        checked={rememberVolume()}
        onChange={(v) => setOption({ remember_volume: v })}
      />
    </div>
  );
}

/** The profile the shown settings start from (a device without one: the defaults) */
function newestBase(list: Profile[]): Profile {
  return (
    list[0] ?? {
      ...({} as Profile),
      capture_mode: "loopback",
      audio_device_id: "",
      audio_device_name: "",
      auto_switch_device: true,
      app_exe: "",
      app_name: "",
      volume_sync: "auto",
      volume_device_id: "",
      volume_device_name: "",
    }
  );
}

function appOptions(apps: AudioApp[] | null, f: DeviceFields) {
  const list = (apps ?? []).map((a) => ({ value: a.exe, label: `${a.name} (${a.exe})` }));
  // The saved app, also when it is not playing now
  if (f.app_exe && !list.some((o) => o.value === f.app_exe)) {
    list.unshift({ value: f.app_exe, label: t("capture.app_not_playing", f.app_name || f.app_exe) });
  }
  return [{ value: "", label: "—", disabled: true }, ...list];
}

function volumeDevices(devices: AudioDevice[] | null, f: DeviceFields): { id: string; name: string }[] {
  const list = (devices ?? []).map((d) => ({ id: d.id, name: d.name }));
  if (f.volume_sync === "device" && f.volume_device_id && !list.some((d) => d.id === f.volume_device_id)) {
    list.unshift({ id: f.volume_device_id, name: t("capture.volume_sync_missing", f.volume_device_name || f.volume_device_id) });
  }
  return list;
}

/**
 * The format Windows mixes the captured device in, red when a profile of the
 * device asks for another rate or bit depth (v1 capture.device_format)
 */
function CaptureFormat(props: { fields: DeviceFields; profiles: Profile[]; devices: AudioDevice[] | null }) {
  const device = () => captureDevice(props.fields, props.devices ?? []);
  return (
    <Show when={device()}>
      {(d) => {
        // The first profile that asks for another format
        const off = () => {
          for (const p of props.profiles) {
            const m = formatMismatch(p, d());
            if (m.rate || m.bits) return { ...m, profile: p.name };
          }
          return null;
        };
        const text = () => t("capture.device_format", d().mix_rate ?? 0, d().mix_bits ?? 0);
        const which = (o: { rate: boolean; bits: boolean }) =>
          [o.rate ? t("connection.sample_rate") : "", o.bits ? t("connection.bit_depth") : ""].filter((x) => x).join(", ");
        return (
          <Show when={d().mix_rate !== null && d().mix_bits !== null}>
            <Show when={off()} fallback={<p class="hint">{text()}</p>}>
              {(o) => (
                <p class="field-error">
                  <Icon name="warning" />
                  <span>
                    {text()}. {o().profile}: {t("ui.profile.format_differs", which(o()))}
                  </span>
                </p>
              )}
            </Show>
          </Show>
        );
      }}
    </Show>
  );
}
