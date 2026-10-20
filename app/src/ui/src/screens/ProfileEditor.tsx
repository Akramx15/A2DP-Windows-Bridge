// The profile editor: device, codec (cards comparing quality, latency and
// stability, and whether the device offered it when it last connected; the
// ones it did not offer hidden unless forced, each with its quality,
// sample rate and bit depth behind "..."), capture source (with the capture
// device's format), volume sync, and the packet size. The defaults give the
// best sound; the codec settings only offer what that codec can do
// (lib/codecs.ts). With LE Audio (experimental) on and an adapter known to
// stream it, "LE Audio device" makes an LE Audio profile: LC3, so no codec,
// quality, sample rate or bit depth, and no application capture.

import { createEffect, createMemo, createResource, createSignal, For, on, Show } from "solid-js";
import { createStore, unwrap } from "solid-js/store";
import { errorText, useApp } from "../app/context";
import { Banner, Button, Check, Dialog, Field, IconButton, Section, Segmented, Select, Toggle } from "../components/controls";
import { DevicePicker } from "../components/DevicePicker";
import { Icon } from "../components/Icon";
import { t } from "../i18n";
import type { AudioApp, AudioDevice, Profile } from "../ipc/types";
import {
  bitDepths,
  CODECS,
  codecSpec,
  codecSupport,
  fitToCodec,
  PACKET_DEFAULT,
  PACKET_MAX,
  PACKET_MIN,
  qualityIgnored,
  sampleRates,
  type CodecSpec,
  type CodecSupport,
} from "../lib/codecs";
import { LINKS } from "../lib/links";
import {
  appCaptureSupport,
  captureDevice,
  defaultProfileName,
  formatMismatch,
  knownCodecs,
  newProfile,
  normalizeProfile,
  validateProfile,
  type Problem,
  type ProfileProblems,
} from "../lib/profile";

const tp = (p: Problem | undefined) => (p ? t(p.key, ...(p.args ?? [])) : null);

export function ProfileEditor(props: { original: string | null }) {
  const app = useApp();
  const { core, navigate } = app;
  const snap = () => core.state.snap!;
  const existing = snap().profiles.find((p) => p.name === props.original);
  const [p, setP] = createStore<Profile>(structuredClone(unwrap(existing) ?? newProfile()));
  // The name follows the device until the user types one
  const [nameTouched, setNameTouched] = createSignal(existing !== undefined);
  const [problems, setProblems] = createSignal<ProfileProblems>({});
  const [tried, setTried] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [saveError, setSaveError] = createSignal<string | null>(null);

  const taken = () => snap().profiles.map((x) => x.name).filter((n) => n !== props.original);
  // Re-check while the user fixes things, once a save was tried
  createEffect(() => {
    const now = validateProfile(p, taken());
    if (tried()) setProblems(now);
  });

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
      () => [p.capture_mode, p.volume_sync] as const,
      ([mode, sync]) => {
        // Loopback and virtual device: the capture device's format line
        if ((mode !== "app" || sync === "device") && devices() === null) void loadDevices();
        if (mode === "app" && apps() === null) void loadApps();
      },
    ),
  );

  const leOn = () => snap().settings.le_audio_experimental;
  // Whether the adapter can stream LE Audio: checked when the stack first
  // runs on it, and remembered; not known yet counts as no
  const leSupport = () => snap().adapter.le_audio;
  const leUnsupported = () => leSupport() !== true;
  const followName = () => {
    if (!nameTouched()) setP("name", defaultProfileName(p.device_name, p.device_address, p.le_audio));
  };
  const pickDevice = (address: string, name: string, leAudio: boolean) => {
    setP({ device_address: address, device_name: name });
    // A new profile follows the entry picked (the classic or the LE Audio
    // entry of a device); an existing one keeps its kind, only the check
    // box changes that (v1)
    if (props.original === null && leOn()) setP("le_audio", leAudio);
    followName();
  };
  const setLeAudio = (v: boolean) => {
    setP("le_audio", v);
    followName();
  };

  const pickCodec = (c: CodecSpec) => setP(fitToCodec({ ...unwrap(p), codec: c.id }));
  // The codec whose settings dialog is open
  const [settingsOf, setSettingsOf] = createSignal<CodecSpec | null>(null);

  const save = async (e?: SubmitEvent) => {
    e?.preventDefault();
    setTried(true);
    const found = validateProfile(p, taken());
    setProblems(found);
    if (Object.keys(found).length > 0) {
      queueMicrotask(() => document.querySelector<HTMLElement>(".field.invalid input, .field.invalid select")?.focus());
      return;
    }
    setSaving(true);
    setSaveError(null);
    try {
      await core.request("profile.save", { profile: normalizeProfile(unwrap(p)), original_name: props.original });
      app.notify(t("ui.profile.saved", p.name.trim()));
      navigate({ name: "profiles" });
    } catch (err) {
      setSaveError(errorText(err));
    } finally {
      setSaving(false);
    }
  };

  const spec = () => codecSpec(p.codec);
  const virtualDevices = createMemo(() => (devices() ?? []).filter((d) => d.is_virtual));
  // What the device offered when it last connected (empty: not known yet)
  const known = createMemo(() => knownCodecs(snap().devices, p.device_address));
  const support = (id: string) => codecSupport(id, known());
  // Codecs the device did not offer are hidden (the profile's own stays)
  // unless forced: it may offer them after a change in its app
  const [showAll, setShowAll] = createSignal(false);
  const notOffered = () => CODECS.filter((c) => c.id !== p.codec && support(c.id) === "not_offered").length;
  const shownCodecs = () => CODECS.filter((c) => showAll() || c.id === p.codec || support(c.id) !== "not_offered");
  const [appSupport] = createResource(appCaptureSupport);

  return (
    <div class="page editor">
      <header class="page-head">
        <IconButton icon="arrow_left" label={t("modal.cancel")} onClick={() => navigate({ name: "profiles" })} />
        <h1>{props.original ? t("profile.edit_title") : t("profile.new_title")}</h1>
      </header>
      <form class="form" onSubmit={save} novalidate>
        <Section title={t("device.tag")}>
          <Field label={t("profile.name")} hint={t("profile.name_hint")} error={tp(problems().name)}>
            {(id, described) => (
              <input
                id={id}
                class="input"
                value={p.name}
                aria-describedby={described}
                aria-invalid={problems().name ? "true" : undefined}
                onInput={(e) => {
                  setNameTouched(true);
                  setP("name", e.currentTarget.value);
                }}
              />
            )}
          </Field>
          <div class="selected-device" classList={{ invalid: !!problems().device }}>
            <Icon name="headphones" />
            <Show when={p.device_address} fallback={<span class="muted">{t("ui.profile.no_device")}</span>}>
              <span>
                <strong>{p.device_name || t("device.name_unknown")}</strong> <span class="mono muted">{p.device_address}</span>
              </span>
            </Show>
          </div>
          {/* LE Audio device (experimental): only offered with the setting on
              and an adapter that can stream it */}
          <Show when={leOn() && (!leUnsupported() || p.le_audio)}>
            <Check label={t("profile.le_audio")} title={t("tooltip.le_audio")} checked={p.le_audio} onChange={setLeAudio} />
            <Show when={p.le_audio}>
              <p class="hint">{t("tooltip.le_audio")}</p>
              <Check
                label={t("settings.le_low_latency")}
                title={t("ui.settings.le_low_latency_hint")}
                checked={p.le_low_latency}
                onChange={(v) => setP("le_low_latency", v)}
              />
            </Show>
          </Show>
          <Show when={p.le_audio && !leOn()}>
            <Banner kind="warn">{t("error.le_audio_disabled")}</Banner>
          </Show>
          <Show when={leOn() && leUnsupported()}>
            <Show
              when={p.le_audio}
              fallback={
                <p class="hint">
                  {leSupport() === false ? t("ui.profile.le_audio_unsupported") : t("ui.profile.le_audio_unchecked")}
                </p>
              }
            >
              <Banner kind="warn">
                {leSupport() === false ? t("ui.profile.le_audio_unsupported") : t("ui.profile.le_audio_unchecked")}
              </Banner>
            </Show>
          </Show>
          <DevicePicker
            address={p.device_address}
            leAudio={p.le_audio}
            onPick={pickDevice}
            error={tp(problems().device)}
          />
        </Section>

        <Show when={!p.le_audio}>
          <Section title={t("connection.codec")}>
            <div class="codec-grid" role="radiogroup" aria-label={t("connection.codec")}>
              <For each={shownCodecs()}>
                {(c) => (
                  <CodecCard
                    spec={c}
                    selected={p.codec === c.id}
                    support={c.id === "auto" ? null : support(c.id)}
                    onPick={() => pickCodec(c)}
                    onSettings={() => setSettingsOf(c)}
                  />
                )}
              </For>
            </div>
            <Show when={notOffered() > 0 || showAll()}>
              <Check
                label={t("ui.profile.codecs_force", notOffered())}
                title={t("ui.profile.codecs_force_hint")}
                checked={showAll()}
                onChange={setShowAll}
              />
            </Show>
            <Show when={p.device_address && known().length === 0}>
              <p class="hint">{t("ui.profile.codecs_unknown")}</p>
            </Show>
            <Show when={support(p.codec) === "not_offered"}>
              <Banner kind="warn" title={t("error.codec_not_supported", t(spec().label))}>
                {p.codec_fallback ? t("ui.profile.codec_fallback_on") : t("ui.profile.codec_fallback_off")}
              </Banner>
            </Show>
            <Check
              label={t("connection.codec_fallback")}
              title={t("tooltip.codec_fallback")}
              checked={p.codec_fallback}
              // Auto already takes the best codec the device offers
              disabled={p.codec === "auto"}
              onChange={(v) => setP("codec_fallback", v)}
            />
            <CodecSettings
              spec={settingsOf()}
              profile={p}
              onApply={(values) => {
                setP(fitToCodec({ ...unwrap(p), ...values }));
                setSettingsOf(null);
              }}
              onCancel={() => setSettingsOf(null)}
            />
          </Section>
        </Show>

        <Section title={t("capture.mode")}>
          <Segmented
            label={t("capture.mode")}
            value={p.capture_mode}
            options={[
              { value: "loopback", label: t("capture.loopback"), icon: "speaker_2" },
              { value: "virtual", label: t("capture.virtual"), icon: "desktop_speaker" },
              { value: "app", label: t("capture.app"), icon: "window_apps" },
            ]}
            onChange={(v) => setP("capture_mode", v)}
          />
          <p class="hint">
            {p.capture_mode === "virtual"
              ? t("capture.hint_virtual")
              : p.capture_mode === "app"
                ? t("capture.hint_app")
                : t("capture.hint_loopback")}
          </p>
          <Show when={p.le_audio && p.capture_mode === "app"}>
            <Banner kind="warn">{t("error.le_audio_capture")}</Banner>
          </Show>
          <CaptureFormat profile={p} devices={devices()} />
          <Show when={p.capture_mode === "virtual"}>
            <Field label={t("capture.audio_device")} hint={t("tooltip.audio_device")} error={tp(problems().audio_device)}>
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
                    value={p.audio_device_id}
                    options={[
                      { value: "", label: "—", disabled: true },
                      ...virtualDevices().map((d) => ({ value: d.id, label: d.name })),
                    ]}
                    onChange={(v) => {
                      const d = virtualDevices().find((x) => x.id === v);
                      setP({ audio_device_id: v, audio_device_name: d?.name ?? "" });
                    }}
                  />
                </Show>
              )}
            </Field>
            <Toggle
              label={t("capture.auto_switch")}
              hint={t("tooltip.auto_switch")}
              checked={p.auto_switch_device}
              onChange={(v) => setP("auto_switch_device", v)}
            />
          </Show>
          <Show when={p.capture_mode === "app"}>
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
            <Field label={t("capture.app_select")} hint={t("capture.app_hint")} error={tp(problems().app)}>
              {(id, described) => (
                <div class="inline">
                  <Select
                    id={id}
                    describedBy={described}
                    value={p.app_exe}
                    disabled={appSupport() === "no"}
                    options={appOptions(apps(), p)}
                    onChange={(v) => {
                      const a = (apps() ?? []).find((x) => x.exe === v);
                      setP({ app_exe: v, app_name: a?.name ?? v });
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
                value={p.volume_sync === "device" ? `device:${p.volume_device_id}` : p.volume_sync}
                options={[
                  { value: "auto", label: t("capture.volume_sync_auto") },
                  { value: "off", label: t("capture.volume_sync_off") },
                  ...volumeDevices(devices(), p).map((d) => ({ value: `device:${d.id}`, label: d.name })),
                ]}
                onChange={(v) => {
                  if (v.startsWith("device:")) {
                    const did = v.slice("device:".length);
                    const d = (devices() ?? []).find((x) => x.id === did);
                    setP({ volume_sync: "device", volume_device_id: did, volume_device_name: d?.name ?? p.volume_device_name });
                  } else {
                    setP({ volume_sync: v });
                    if (devices() === null) void loadDevices();
                  }
                }}
              />
            )}
          </Field>
        </Section>

        {/* The one setting left of "Advanced": shown as it is (issue #86) */}
        <Section title={t("ui.profile.advanced")}>
          <Field
            label={t("advanced.max_packet")}
            hint={t("advanced.max_packet_help", PACKET_MIN, PACKET_MAX, PACKET_DEFAULT)}
            error={tp(problems().packet)}
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
                  value={p.max_media_payload}
                  aria-describedby={described}
                  aria-invalid={problems().packet ? "true" : undefined}
                  onInput={(e) => setP("max_media_payload", e.currentTarget.valueAsNumber)}
                />
                <Button
                  kind="subtle"
                  disabled={p.max_media_payload === PACKET_DEFAULT}
                  onClick={() => setP("max_media_payload", PACKET_DEFAULT)}
                >
                  {t("advanced.max_packet_recommended", PACKET_DEFAULT)}
                </Button>
              </div>
            )}
          </Field>
        </Section>

        <Show when={saveError()}>
          <Banner kind="error" title={t("status.error")}>
            {saveError()}
          </Banner>
        </Show>
        <Show when={tried() && Object.keys(problems()).length > 0}>
          <p class="field-error" role="alert">
            <Icon name="error_circle" />
            {t("ui.profile.fix_problems")}
          </p>
        </Show>
        <div class="form-actions">
          <Button kind="subtle" onClick={() => navigate({ name: "profiles" })}>
            {t("modal.cancel")}
          </Button>
          <Button kind="strong" type="submit" icon="checkmark" busy={saving()}>
            {t("modal.save")}
          </Button>
        </div>
      </form>
    </div>
  );
}

function appOptions(apps: AudioApp[] | null, p: Profile) {
  const list = (apps ?? []).map((a) => ({ value: a.exe, label: `${a.name} (${a.exe})` }));
  // The saved app, also when it is not playing now
  if (p.app_exe && !list.some((o) => o.value === p.app_exe)) {
    list.unshift({ value: p.app_exe, label: t("capture.app_not_playing", p.app_name || p.app_exe) });
  }
  return [{ value: "", label: "—", disabled: true }, ...list];
}

function volumeDevices(devices: AudioDevice[] | null, p: Profile): { id: string; name: string }[] {
  const list = (devices ?? []).map((d) => ({ id: d.id, name: d.name }));
  if (p.volume_sync === "device" && p.volume_device_id && !list.some((d) => d.id === p.volume_device_id)) {
    list.unshift({ id: p.volume_device_id, name: t("capture.volume_sync_missing", p.volume_device_name || p.volume_device_id) });
  }
  return list;
}

/** One codec choice with its three comparison bars, and "..." for its settings */
function CodecCard(props: {
  spec: CodecSpec;
  selected: boolean;
  support: CodecSupport | null;
  onPick: () => void;
  onSettings: () => void;
}) {
  const bar = (label: string, v: number) => (
    <div class="cbar">
      <span class="cbar-label">{label}</span>
      <span class="cbar-track" role="meter" aria-valuemin={0} aria-valuemax={5} aria-valuenow={v} aria-label={label}>
        <span class="cbar-fill" style={{ width: `${v * 20}%` }} />
      </span>
    </div>
  );
  const name = () => t(props.spec.label);
  return (
    // The button is beside the label, not in it: the radio's name stays the codec's
    <div class="codec-cell">
      <label class="codec-card" classList={{ on: props.selected }}>
        <input type="radio" name="codec" value={props.spec.id} checked={props.selected} onChange={() => props.onPick()} />
        <span class="codec-card-name">{name()}</span>
        {/* The wrapper keeps the tag its own size in the card's column */}
        <Show when={props.support !== "unknown" && props.support}>
          {(s) => (
            <span>
              <span class="tag">
                <Icon name={s() === "offered" ? "checkmark" : "dismiss"} />
                {s() === "offered" ? t("ui.profile.codec_offered") : t("ui.profile.codec_not_offered")}
              </span>
            </span>
          )}
        </Show>
        <Show when={props.spec.bars} fallback={<span class="muted codec-card-desc">{t("ui.codec.auto_desc")}</span>}>
          {(b) => (
            <>
              {bar(t("ui.codec.bar_quality"), b().quality)}
              {bar(t("ui.codec.bar_latency"), b().latency)}
              {bar(t("ui.codec.bar_stability"), b().stability)}
            </>
          )}
        </Show>
      </label>
      <IconButton
        icon="more_horizontal"
        class="codec-more"
        label={t("ui.profile.codec_settings", name())}
        aria-haspopup="dialog"
        onClick={() => props.onSettings()}
      />
    </div>
  );
}

type CodecValues = Pick<Profile, "codec" | "quality" | "sample_rate" | "bit_depth" | "abr">;

/**
 * One codec's quality, sample rate, bit depth and ABR (what "Advanced" had
 * for the chosen codec). It starts from the profile's values fitted to the
 * codec; OK chooses the codec with them, Cancel leaves the profile alone.
 */
function CodecSettings(props: {
  spec: CodecSpec | null;
  profile: Profile;
  onApply: (values: CodecValues) => void;
  onCancel: () => void;
}) {
  const [v, setV] = createStore<CodecValues>({ codec: "auto", quality: "hq", sample_rate: 0, bit_depth: 0, abr: false });
  createEffect(
    on(
      () => props.spec,
      (s) => {
        if (!s) return;
        const p = props.profile;
        setV(fitToCodec({ codec: s.id, quality: p.quality, sample_rate: p.sample_rate, bit_depth: p.bit_depth, abr: p.abr }));
      },
    ),
  );
  const spec = () => codecSpec(v.codec);
  const name = () => t(spec().label);
  return (
    <Dialog
      open={props.spec !== null}
      title={t("ui.profile.codec_settings", name())}
      confirm={t("modal.ok")}
      cancel={t("modal.cancel")}
      onConfirm={() => props.onApply({ ...v })}
      onCancel={() => props.onCancel()}
    >
      <div class="codec-settings">
        <Field label={t("connection.quality")} hint={t("tooltip.quality")}>
          {(id, described) => (
            <Select
              id={id}
              describedBy={described}
              value={v.quality}
              disabled={spec().qualities.length === 0 || qualityIgnored(v.codec, v.abr)}
              options={
                spec().qualities.length
                  ? spec().qualities.map((q) => ({ value: q.id, label: q.kbps ? `${t(q.label)} (${q.kbps} kbps)` : t(q.label) }))
                  : [{ value: v.quality, label: t("ui.profile.fixed_bitrate") }]
              }
              onChange={(x) => setV("quality", x)}
            />
          )}
        </Field>
        <Field label={t("connection.sample_rate")} hint={t("tooltip.sample_rate")}>
          {(id, described) => (
            <Select
              id={id}
              describedBy={described}
              value={v.sample_rate}
              options={sampleRates(v.codec).map((r) => ({ value: r, label: r === 0 ? t("codec.auto") : `${r} Hz` }))}
              onChange={(x) => setV("sample_rate", x)}
            />
          )}
        </Field>
        <Field label={t("connection.bit_depth")} hint={t("tooltip.bit_depth")}>
          {(id, described) => (
            <Select
              id={id}
              describedBy={described}
              value={v.bit_depth}
              disabled={bitDepths(v.codec).length === 1}
              options={bitDepths(v.codec).map((b) => ({
                value: b,
                label: b === 0 ? (bitDepths(v.codec).length > 1 ? t("codec.auto") : t("bitdepth.16")) : t(`bitdepth.${b}`),
              }))}
              onChange={(x) => setV("bit_depth", x)}
            />
          )}
        </Field>
        <Show when={spec().abr}>
          <Check label={t("connection.abr")} title={t("tooltip.abr")} checked={v.abr} onChange={(x) => setV("abr", x)} />
        </Show>
        <Show when={v.codec !== props.profile.codec}>
          <p class="hint">{t("ui.profile.codec_settings_apply", name())}</p>
        </Show>
      </div>
    </Dialog>
  );
}

/**
 * The format Windows mixes the captured device in, red when the profile
 * asks for another rate or bit depth (v1 capture.device_format)
 */
function CaptureFormat(props: { profile: Profile; devices: AudioDevice[] | null }) {
  const device = () => captureDevice(props.profile, props.devices ?? []);
  return (
    <Show when={device()}>
      {(d) => {
        const off = () => formatMismatch(props.profile, d());
        const text = () => t("capture.device_format", d().mix_rate ?? 0, d().mix_bits ?? 0);
        const which = () =>
          [off().rate ? t("connection.sample_rate") : "", off().bits ? t("connection.bit_depth") : ""]
            .filter((x) => x)
            .join(", ");
        return (
          <Show when={d().mix_rate !== null && d().mix_bits !== null}>
            <Show when={off().rate || off().bits} fallback={<p class="hint">{text()}</p>}>
              <p class="field-error">
                <Icon name="warning" />
                <span>
                  {text()}. {t("ui.profile.format_differs", which())}
                </span>
              </p>
            </Show>
          </Show>
        );
      }}
    </Show>
  );
}
