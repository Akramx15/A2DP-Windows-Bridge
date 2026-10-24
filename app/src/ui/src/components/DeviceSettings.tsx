// One audio device's own settings on the Audio Devices screen (issue #86):
// what is captured for it (one list, as on Home), the Windows volume that
// follows it and, in
// developer mode, the max media packet size (kept in its profile: the core
// reads them from the profile it streams; a device without one gets it with
// the first change),
// whether its buttons control playback and whether its volume is
// remembered (Settings.device_options; the global value until changed).
// Only what matters is spelled out: the long explanations are behind the
// info buttons.

import { createEffect, createMemo, createResource, createSignal, on, onCleanup, onMount, Show } from "solid-js";
import { createStore, unwrap } from "solid-js/store";
import { useApp } from "../app/context";
import { saveSettings } from "../app/settings";
import { t } from "../i18n";
import { codecSpec, PACKET_DEFAULT, PACKET_MAX, PACKET_MIN } from "../lib/codecs";
import type { DeviceOptions } from "../ipc/bindings/DeviceOptions";
import type { AudioApp, AudioDevice, Device, Profile } from "../ipc/types";
import { LINKS } from "../lib/links";
import {
  appCaptureSupport,
  captureDevice,
  deviceFields,
  deviceFieldsComplete,
  entryBase,
  followedName,
  formatMismatch,
  normalizeProfile,
  profilesOf,
  type DeviceFields,
} from "../lib/profile";
import { captureChoices, captureText, captureValue, withCapture } from "./CapturePicker";
import { Banner, Button, Field, IconButton, Select, Toggle, type Option } from "./controls";
import { Icon } from "./Icon";

export function DeviceSettings(props: { device: Device }) {
  const app = useApp();
  const { core } = app;
  const snap = () => core.state.snap!;
  const profiles = createMemo(() => profilesOf(snap().profiles, props.device.address));
  // An LE Audio entry: its own profile, and no A2DP packet size
  const leEntry = () => props.device.le_audio;
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
  // Both are in the capture source's list; asked again whenever the window
  // comes back (apps start and stop playing)
  const load = () => {
    void loadDevices();
    void loadApps();
  };
  onMount(() => {
    load();
    window.addEventListener("focus", load);
  });
  onCleanup(() => window.removeEventListener("focus", load));
  const virtualDevices = createMemo(() => (devices() ?? []).filter((d) => d.is_virtual));
  const [appSupport] = createResource(appCaptureSupport);
  const leOnly = () => leEntry() || (profiles().length > 0 && profiles().every((p) => p.le_audio));
  /** The shown settings as a profile (what Home's capture source list takes) */
  const shown = (): Profile => ({ ...newestBase(profiles()), ...f, le_audio: leOnly() });

  /** Changes the device's settings: into each of its profiles once complete
   *  (a virtual device or an app chosen); a profile in use starts again with
   *  them (profile.save in the core). A device without a profile gets one
   *  (Auto, as the Codecs screen would make it). */
  const apply = async (patch: Partial<DeviceFields>) => {
    setF(patch);
    const next = { ...unwrap(f) };
    if (!deviceFieldsComplete(next)) return;
    setSaving(true);
    if (profiles().length === 0) {
      const base: Profile = { ...entryBase(props.device, snap().profiles), ...next };
      base.name = followedName(base, props.device.le_audio ? "" : t(codecSpec("auto").label), snap().profiles.map((p) => p.name));
      await app.run(() => core.request("profile.save", { profile: normalizeProfile(base), original_name: null }));
    }
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

  /** System Loopback, the virtual devices and the playing apps in one list */
  const sources = createMemo((): Option<string>[] => {
    const p = shown();
    const c = captureChoices(p, devices() ?? [], apps() ?? [], appSupport() !== "no");
    const virtual: Option<string>[] = c.virtual.map((o) => ({ ...o, group: t("capture.virtual") }));
    if (virtual.length === 0 && devices() !== null) {
      virtual.push({ value: "virtual:", label: t("capture.no_virtual_device"), group: t("capture.virtual"), disabled: true });
    }
    const list: Option<string>[] = c.apps.map((o) => ({ ...o, group: t("capture.app") }));
    if (list.length === 0 && apps() !== null && !p.le_audio && appSupport() !== "no") {
      list.push({ value: "app:", label: t("capture.no_app_playing"), group: t("capture.app"), disabled: true });
    }
    return [{ value: "loopback", label: t("capture.loopback") }, ...virtual, ...list];
  });
  const chooseSource = (value: string) => {
    const p = shown();
    if (value === captureValue(p)) return;
    const next = withCapture(p, value, devices() ?? [], apps() ?? []);
    void apply({
      capture_mode: next.capture_mode,
      audio_device_id: next.audio_device_id,
      audio_device_name: next.audio_device_name,
      app_exe: next.app_exe,
      app_name: next.app_name,
    });
  };

  return (
    <div class="device-settings" aria-busy={saving() ? "true" : undefined}>
      <Field
        label={t("capture.mode")}
        more={() => (
          <>
            <p class="hint">
              {f.capture_mode === "virtual"
                ? t("capture.hint_virtual")
                : f.capture_mode === "app"
                  ? `${t("capture.hint_app")}\n${t("capture.app_hint")}`
                  : t("capture.hint_loopback")}
            </p>
            <Show when={f.capture_mode === "app" && appSupport() !== "yes" && appSupport() !== "no"}>
              <p class="hint">{t("ui.profile.app_capture_requirement")}</p>
            </Show>
            <Show when={devices() !== null && virtualDevices().length === 0}>
              <div class="inline">
                <span class="hint">{t("capture.no_virtual_device")}</span>
                <Button icon="open" onClick={() => core.transport.openUrl(LINKS.vbCable)}>
                  {t("vbcable.download")}
                </Button>
              </div>
            </Show>
          </>
        )}
      >
        {(id, described) => (
          <div class="inline capture-source">
            <Icon name={captureText(shown()).icon} />
            <Select id={id} describedBy={described} value={captureValue(shown())} options={sources()} onChange={chooseSource} />
            <IconButton icon="arrow_sync" label={t("capture.app_refresh")} onClick={load} />
          </div>
        )}
      </Field>
      <CaptureFormat fields={f} profiles={profiles()} devices={devices()} />
      {/* Whether this Windows can capture one app: only the core knows the
          build for sure (it refuses the stream on an older Windows); the UI
          asks the WebView's client hints */}
      <Show when={f.capture_mode === "app" && appSupport() === "no"}>
        <Banner kind="warn">{t("ui.profile.app_capture_unsupported")}</Banner>
      </Show>
      <Show when={f.capture_mode === "virtual"}>
        <Toggle
          label={t("capture.auto_switch")}
          hint={t("tooltip.auto_switch")}
          checked={f.auto_switch_device}
          onChange={(v) => void apply({ auto_switch_device: v })}
        />
      </Show>
      <Field
        label={t("capture.volume_sync")}
        hint={t("capture.volume_sync_hint")}
        more={() => <p class="hint">{t("tooltip.volume_sync")}</p>}
      >
        {(id, described) => (
          <Select
            id={id}
            describedBy={described}
            value={f.volume_sync === "device" ? `device:${f.volume_device_id}` : f.volume_sync}
            options={[
              { value: "auto", label: t("capture.volume_sync_auto") },
              { value: "off", label: t("capture.volume_sync_off") },
              ...volumeDevices(devices(), f).map((d) => ({
                value: `device:${d.id}`,
                label: d.name,
                group: t("capture.volume_sync_other"),
              })),
            ]}
            onChange={(v) => {
              if (v.startsWith("device:")) {
                const did = v.slice("device:".length);
                const d = (devices() ?? []).find((x) => x.id === did);
                void apply({ volume_sync: "device", volume_device_id: did, volume_device_name: d?.name ?? f.volume_device_name });
              } else {
                void apply({ volume_sync: v });
              }
            }}
          />
        )}
      </Field>
      {/* Not one for end users (issue #86): developer mode, or a size
          changed earlier, so it can be set back */}
      <Show when={!leOnly() && (snap().settings.developer_mode || f.max_media_payload !== PACKET_DEFAULT)}>
        <PacketSize value={f.max_media_payload} onChange={(v) => void apply({ max_media_payload: v })} />
      </Show>
      <div>
        <Toggle
          label={t("settings.media_keys")}
          hint={t("tooltip.media_keys")}
          checked={mediaKeys()}
          onChange={(v) => setOption({ media_keys: v })}
        />
        <Toggle
          label={t("settings.remember_volume")}
          checked={rememberVolume()}
          onChange={(v) => setOption({ remember_volume: v })}
        />
      </div>
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
      max_media_payload: PACKET_DEFAULT,
    }
  );
}

/**
 * The max media packet size (it was the profile's "Advanced" setting: the
 * same for every codec, so the device's, issue #86). Saved when a valid
 * value is left or the recommended one chosen.
 */
function PacketSize(props: { value: number; onChange: (v: number) => void }) {
  const [text, setText] = createSignal<string | null>(null);
  createEffect(on(() => props.value, () => setText(null), { defer: true }));
  const shown = () => text() ?? String(props.value);
  const valid = (s: string) => {
    const n = Number(s);
    return Number.isInteger(n) && n >= PACKET_MIN && n <= PACKET_MAX;
  };
  const commit = () => {
    const s = text();
    if (s === null || !valid(s)) return;
    if (Number(s) !== props.value) props.onChange(Number(s));
  };
  return (
    <Field
      label={t("advanced.max_packet")}
      hint={t("advanced.max_packet_help", PACKET_MIN, PACKET_MAX, PACKET_DEFAULT)}
      error={text() !== null && !valid(text()!) ? t("ui.profile.err_packet", PACKET_MIN, PACKET_MAX) : null}
    >
      {(id, described) => (
        <div class="inline">
          <input
            id={id}
            class="input num narrow"
            type="number"
            inputmode="numeric"
            min={PACKET_MIN}
            max={PACKET_MAX}
            step="1"
            value={shown()}
            aria-describedby={described}
            aria-invalid={text() !== null && !valid(text()!) ? "true" : undefined}
            onInput={(e) => setText(e.currentTarget.value)}
            onChange={commit}
            onKeyDown={(e) => e.key === "Enter" && commit()}
          />
          <Button kind="subtle" disabled={props.value === PACKET_DEFAULT && text() === null} onClick={() => props.onChange(PACKET_DEFAULT)}>
            {t("advanced.max_packet_recommended", PACKET_DEFAULT)}
          </Button>
        </div>
      )}
    </Field>
  );
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
