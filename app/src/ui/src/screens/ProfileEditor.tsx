// The profile editor: device, codec (cards comparing quality, latency and
// stability, and whether the device supported it when it last connected;
// the ones it did not support hidden unless asked for, each with its
// quality, sample rate and bit depth behind "..."), and the packet size.
// The capture source and volume sync are the device's, on the Audio
// Devices screen (issue #86); a new profile takes them from the device's
// other profiles. The defaults give the best sound; the codec settings only
// offer what that codec can do (lib/codecs.ts). With LE Audio (experimental)
// on and an adapter known to stream it, "LE Audio device" makes an LE Audio
// profile: LC3, so no codec, quality, sample rate or bit depth; that choice
// sits with the codecs. Codec and what follows wait for a device: the codecs
// offered are the device's.

import { createEffect, createMemo, createSignal, For, on, Show } from "solid-js";
import { createStore, unwrap } from "solid-js/store";
import { errorText, useApp } from "../app/context";
import { Banner, Button, Check, Dialog, Field, IconButton, Section, Select } from "../components/controls";
import { DevicePicker } from "../components/DevicePicker";
import { Icon } from "../components/Icon";
import { t } from "../i18n";
import type { Profile } from "../ipc/types";
import {
  abrCarries,
  bitDepths,
  CODECS,
  codecSpec,
  codecSupport,
  fitToCodec,
  PACKET_DEFAULT,
  PACKET_MAX,
  PACKET_MIN,
  BITPOOL_MAX,
  BITPOOL_MAX_XQ,
  BITPOOL_MIN,
  qualityIgnored,
  sampleRates,
  type CodecSpec,
  type CodecSupport,
} from "../lib/codecs";
import { knownSbcCaps, SBC_ALLOCATIONS, SBC_BLOCKS, SBC_CHANNEL_MODES, SBC_SUBBANDS, sbcOffered, sbcRate, sbcSetup, type SbcCaps } from "../lib/sbc";
import {
  defaultProfileName,
  deviceDefaults,
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

  const leOn = () => snap().settings.le_audio_experimental;
  // Whether the adapter can stream LE Audio: checked when the stack first
  // runs on it, and remembered; not known yet counts as no
  const leSupport = () => snap().adapter.le_audio;
  const leUnsupported = () => leSupport() !== true;
  const followName = () => {
    if (!nameTouched()) {
      setP("name", defaultProfileName(p.device_name, p.device_address, p.le_audio, t(codecSpec(p.codec).label)));
    }
  };
  const pickDevice = (address: string, name: string, leAudio: boolean) => {
    setP({ device_address: address, device_name: name });
    // The capture source and volume sync are the device's (Audio Devices):
    // a new profile takes them from its other profiles
    const own = deviceDefaults(snap().profiles.filter((x) => x.name !== props.original), address);
    if (own) setP(own);
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

  // What each codec was set to while the editor is open: switching to
  // another codec and back brings its settings back (issue #86, AAC's VBR
  // was lost)
  const kept = new Map<string, CodecValues>();
  const pickCodec = (c: CodecSpec) => {
    const cur = unwrap(p);
    kept.set(cur.codec, codecValues(cur));
    const before = kept.get(c.id);
    if (before) {
      setP(fitToCodec({ ...cur, ...before }));
    } else {
      // The flag is VBR under AAC, the adaptive bitpool under SBC and ABR
      // elsewhere: not carried across
      const abr = abrCarries(cur.codec, c.id) ? cur.abr : false;
      setP(fitToCodec({ ...cur, abr, codec: c.id }));
    }
    followName();
  };
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
  // What the device offered when it last connected (empty: not known yet)
  const known = createMemo(() => knownCodecs(snap().devices, p.device_address));
  const support = (id: string) => codecSupport(id, known());
  // Codecs the device did not offer are hidden (the profile's own stays)
  // unless forced: it may offer them after a change in its app
  const [showAll, setShowAll] = createSignal(false);
  const notOffered = () => CODECS.filter((c) => c.id !== p.codec && support(c.id) === "not_offered").length;
  const shownCodecs = () => CODECS.filter((c) => showAll() || c.id === p.codec || support(c.id) !== "not_offered");

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
          <DevicePicker
            address={p.device_address}
            leAudio={p.le_audio}
            onPick={pickDevice}
            savedOnly
            error={tp(problems().device)}
          />
        </Section>

        {/* The codecs offered are the device's: chosen once there is one */}
        <Show when={p.device_address}>
          <Section title={t("connection.codec")}>
            <Show
              when={!p.le_audio}
              fallback={
                // LE Audio: LC3, the configuration the device offers best
                <div class="codec-grid" role="radiogroup" aria-label={t("connection.codec")}>
                  <div class="codec-cell">
                    <label class="codec-card on">
                      <input type="radio" name="codec" value="lc3" checked />
                      <span class="codec-card-name">{t("ui.codec.lc3")}</span>
                      <span class="muted codec-card-desc">{t("ui.codec.lc3_desc")}</span>
                    </label>
                  </div>
                </div>
              }
            >
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
                    label={t("ui.profile.codecs_hide", notOffered())}
                    title={t("ui.profile.codecs_force_hint")}
                    checked={!showAll()}
                    onChange={(hide) => setShowAll(!hide)}
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
                  kept={(codec) => kept.get(codec)}
                  sbcCaps={knownSbcCaps(snap().devices, p.device_address)}
                  onApply={(values) => {
                    const cur = unwrap(p);
                    if (values.codec !== cur.codec) kept.set(cur.codec, codecValues(cur));
                    setP(fitToCodec({ ...cur, ...values }));
                    setSettingsOf(null);
                  }}
                  onCancel={() => setSettingsOf(null)}
                />
            </Show>
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
          </Section>

          {/* The capture source and volume sync are the device's (issue #86) */}
          <p class="hint">
            {t("ui.profile.capture_moved")}{" "}
            <button type="button" class="link" onClick={() => navigate({ name: "devices" })}>
              {t("ui.nav.devices")}
            </button>
          </p>

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
        </Show>

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
                {s() === "offered" ? t("ui.profile.codec_supported") : t("ui.profile.codec_not_supported")}
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

type CodecValues = Pick<
  Profile,
  | "codec"
  | "quality"
  | "sample_rate"
  | "bit_depth"
  | "abr"
  | "sbc_bitpool"
  | "sbcxq_bitpool"
  | "sbc_blocks"
  | "sbc_subbands"
  | "sbc_allocation"
  | "sbc_channel_mode"
>;

function codecValues(p: CodecValues): CodecValues {
  return {
    codec: p.codec,
    quality: p.quality,
    sample_rate: p.sample_rate,
    bit_depth: p.bit_depth,
    abr: p.abr,
    sbc_bitpool: p.sbc_bitpool,
    sbcxq_bitpool: p.sbcxq_bitpool,
    sbc_blocks: p.sbc_blocks,
    sbc_subbands: p.sbc_subbands,
    sbc_allocation: p.sbc_allocation,
    sbc_channel_mode: p.sbc_channel_mode,
  };
}

/**
 * One codec's quality, sample rate, bit depth and ABR (what "Advanced" had
 * for the chosen codec); SBC and SBC XQ also their bitpool and frame setup,
 * offering only what the device listed when it last connected (`sbcCaps`),
 * with the bit rate that gives. It starts from the profile's values fitted
 * to the codec, or from what that codec was set to earlier in the editor
 * (`kept`); OK chooses the codec with them, Cancel leaves the profile alone.
 */
function CodecSettings(props: {
  spec: CodecSpec | null;
  profile: Profile;
  kept: (codec: string) => CodecValues | undefined;
  sbcCaps: SbcCaps | null;
  onApply: (values: CodecValues) => void;
  onCancel: () => void;
}) {
  const [v, setV] = createStore<CodecValues>({
    codec: "auto",
    quality: "hq",
    sample_rate: 0,
    bit_depth: 0,
    abr: false,
    sbc_bitpool: 0,
    sbcxq_bitpool: 0,
    sbc_blocks: 0,
    sbc_subbands: 0,
    sbc_allocation: "",
    sbc_channel_mode: "",
  });
  createEffect(
    on(
      () => props.spec,
      (s) => {
        if (!s) return;
        const p = s.id === props.profile.codec ? props.profile : (props.kept(s.id) ?? props.profile);
        setV(
          fitToCodec({
            codec: s.id,
            // SBC / SBC XQ: no quality to choose, Auto is the highest bitpool
            quality: s.bitpool ? "hq" : p.quality,
            sample_rate: p.sample_rate,
            bit_depth: p.bit_depth,
            abr: abrCarries(p.codec, s.id) ? p.abr : false,
            sbc_bitpool: p.sbc_bitpool,
            sbcxq_bitpool: p.sbcxq_bitpool,
            sbc_blocks: p.sbc_blocks,
            sbc_subbands: p.sbc_subbands,
            sbc_allocation: p.sbc_allocation,
            sbc_channel_mode: p.sbc_channel_mode,
          }),
        );
      },
    ),
  );
  const spec = () => codecSpec(v.codec);
  const name = () => t(spec().label);
  // SBC and SBC XQ keep their own bitpool
  const bpKey = () => (v.codec === "sbcxq" ? "sbcxq_bitpool" : "sbc_bitpool");
  const bp = () => v[bpKey()];
  const bpMax = () => (v.codec === "sbcxq" ? BITPOOL_MAX_XQ : BITPOOL_MAX);
  const bitpoolError = () =>
    bp() !== 0 && !(Number.isInteger(bp()) && bp() >= BITPOOL_MIN && bp() <= bpMax())
      ? t("error.sbc_bitpool_range", BITPOOL_MIN, bpMax())
      : undefined;
  // The frame setup: Auto, then what the device listed (the value chosen
  // stays, marked, when it did not list it: it falls back when connecting)
  const sbcChoice = <T extends string | number>(
    field: "blocks" | "subbands" | "allocations" | "channel_modes",
    values: T[],
    current: T,
    label: (v: T) => string,
  ) => [
    { value: (typeof current === "number" ? 0 : "") as T, label: t("codec.auto") },
    ...values
      .filter((x) => x === current || sbcOffered(props.sbcCaps, field, x))
      .map((x) => ({
        value: x,
        label: sbcOffered(props.sbcCaps, field, x) ? label(x) : t("ui.profile.sbc_not_offered", label(x)),
      })),
  ];
  // The bit rate it gives, as the core works it out, at the profile's rate
  // (both when Auto)
  // (on Auto the highest rate the device takes: one figure, issue #86)
  const sbcBitrate = () => {
    if (bitpoolError()) return "";
    const r = sbcRate(v, props.sbcCaps);
    return t("ui.profile.sbc_bitrate", sbcSetup(v, r, props.sbcCaps).kbps, r === 44100 ? "44.1" : "48");
  };
  return (
    <Dialog
      open={props.spec !== null}
      title={t("ui.profile.codec_settings", name())}
      confirm={t("modal.ok")}
      cancel={t("modal.cancel")}
      onConfirm={() => {
        if (!bitpoolError()) props.onApply({ ...v });
      }}
      onCancel={() => props.onCancel()}
    >
      <div class="codec-settings">
        {/* SBC and SBC XQ: the bitpool and frame setup below decide the bit
            rate, shown here as worked out (issue #86) */}
        <Show
          when={!spec().bitpool}
          fallback={
            <Field label={t("connection.quality")} hint={t("ui.profile.sbc_quality_hint")}>
              {(id, described) => (
                <output id={id} class="input readonly num" aria-describedby={described} aria-live="polite">
                  {sbcBitrate() || "–"}
                </output>
              )}
            </Field>
          }
        >
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
        </Show>
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
          <Check
            label={t("connection.abr")}
            title={t(spec().bitpool ? "tooltip.sbc_abr" : "tooltip.abr")}
            checked={v.abr}
            onChange={(x) => setV("abr", x)}
          />
        </Show>
        <Show when={spec().vbr}>
          <Check label={t("connection.vbr")} title={t("tooltip.vbr")} checked={v.abr} onChange={(x) => setV("abr", x)} />
        </Show>
        <Show when={spec().bitpool}>
          <Field
            label={t("connection.sbc_bitpool")}
            hint={t("tooltip.sbc_bitpool", BITPOOL_MIN, bpMax())}
            error={bitpoolError()}
          >
            {(id, described) => (
              <div class="inline">
                <input
                  id={id}
                  class="input num narrow"
                  type="number"
                  inputmode="numeric"
                  min={BITPOOL_MIN}
                  max={bpMax()}
                  step="1"
                  placeholder={t("codec.auto")}
                  value={bp() === 0 ? "" : bp()}
                  aria-describedby={described}
                  onInput={(e) => setV(bpKey(), e.currentTarget.value === "" ? 0 : e.currentTarget.valueAsNumber)}
                />
                <Button kind="subtle" disabled={bp() === 0} onClick={() => setV(bpKey(), 0)}>
                  {t("codec.auto")}
                </Button>
              </div>
            )}
          </Field>
          <fieldset class="sbc-frame">
            <legend class="field-label">{t("ui.profile.sbc_frame")}</legend>
            <Field label={t("ui.diag.cd_blocks")}>
              {(id) => (
                <Select
                  id={id}
                  value={v.sbc_blocks}
                  options={sbcChoice("blocks", SBC_BLOCKS, v.sbc_blocks, String)}
                  onChange={(x) => setV("sbc_blocks", x)}
                />
              )}
            </Field>
            <Field label={t("ui.diag.cd_subbands")}>
              {(id) => (
                <Select
                  id={id}
                  value={v.sbc_subbands}
                  options={sbcChoice("subbands", SBC_SUBBANDS, v.sbc_subbands, String)}
                  onChange={(x) => setV("sbc_subbands", x)}
                />
              )}
            </Field>
            <Field label={t("ui.diag.cd_allocation")}>
              {(id) => (
                <Select
                  id={id}
                  value={v.sbc_allocation}
                  options={sbcChoice("allocations", SBC_ALLOCATIONS, v.sbc_allocation, (a) => t(`ui.diag.cd_alloc.${a}`))}
                  onChange={(x) => setV("sbc_allocation", x)}
                />
              )}
            </Field>
            {/* SBC XQ is dual channel by definition */}
            <Show when={v.codec === "sbc"}>
              <Field label={t("ui.diag.cd_channel_mode")}>
                {(id) => (
                  <Select
                    id={id}
                    value={v.sbc_channel_mode}
                    options={sbcChoice("channel_modes", SBC_CHANNEL_MODES, v.sbc_channel_mode, (m) => t(`ui.diag.cd_mode.${m}`))}
                    onChange={(x) => setV("sbc_channel_mode", x)}
                  />
                )}
              </Field>
            </Show>
            <p class="hint">{t("ui.profile.sbc_frame_hint")}</p>
          </fieldset>
        </Show>
        <Show when={v.codec !== props.profile.codec}>
          <p class="hint">{t("ui.profile.codec_settings_apply", name())}</p>
        </Show>
      </div>
    </Dialog>
  );
}
