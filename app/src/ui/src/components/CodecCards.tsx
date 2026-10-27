// The codec cards of the Codecs screen: one codec with its three
// comparison bars and whether the device supported it when it last
// connected (green / red, issue #86), and behind "..." that codec's
// quality, sample rate, bit depth
// and ABR (SBC and SBC XQ: bitpool and frame setup). The codecs offered
// and what they can be set to come from lib/codecs.ts, as far as the
// device listed them when it last connected (issue #86: its rates, bit
// depths, largest bit rate and bitpool range).

import { createEffect, on, Show } from "solid-js";
import { createStore } from "solid-js/store";
import { t } from "../i18n";
import type { Profile } from "../ipc/types";
import {
  capsOf,
  carryToCodec,
  fixedBitDepth,
  codecSpec,
  BITPOOL_MAX,
  BITPOOL_MAX_XQ,
  BITPOOL_MIN,
  offeredBitDepths,
  offeredQualities,
  offeredSampleRates,
  qualityIgnored,
  type CodecCaps,
  type CodecSpec,
  type CodecSupport,
} from "../lib/codecs";
import { SBC_ALLOCATIONS, SBC_BLOCKS, SBC_CHANNEL_MODES, SBC_SUBBANDS, sbcOffered, sbcRate, sbcSetup, type SbcCaps } from "../lib/sbc";
import { Button, Check, Dialog, Field, IconButton, Select } from "./controls";
import { Icon } from "./Icon";

/** One codec choice with its three comparison bars, and "..." for its settings */
export function CodecCard(props: {
  spec: CodecSpec;
  /** The radio group: one per device */
  group: string;
  selected: boolean;
  disabled?: boolean;
  /** Auto chooses (issue #86): the card only shows whether Auto picked it */
  readOnly?: boolean;
  support: CodecSupport | null;
  onPick: () => void;
  /** The card or its "..." clicked while read-only: offers to turn
   *  Auto-select off (issue #86 round 8) */
  onReadOnly?: () => void;
  onSettings: () => void;
  /** The card can be dragged (and moved with Alt+arrow keys) into the
   *  device's order (issue #86 round 8) */
  orderable?: boolean;
  /** Being dragged */
  dragging?: boolean;
  /** Alt+arrow key: one place earlier (-1) or later (1) */
  onMove?: (by: -1 | 1) => void;
  /** The id of what tells how to move it (screen readers) */
  describedBy?: string;
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
    <div class="codec-cell" classList={{ dragging: props.dragging }} data-codec={props.spec.id}>
      <label
        class="codec-card"
        classList={{ on: props.selected, ro: props.readOnly, orderable: props.orderable }}
        onClick={() => {
          if (props.readOnly) props.onReadOnly?.();
        }}
      >
        <input
          type="radio"
          name={props.group}
          value={props.spec.id}
          checked={props.selected}
          disabled={props.disabled || props.readOnly}
          aria-describedby={props.orderable ? props.describedBy : undefined}
          onChange={() => props.onPick()}
          onKeyDown={(e) => {
            if (!props.orderable || !e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
            const by = { ArrowLeft: -1, ArrowUp: -1, ArrowRight: 1, ArrowDown: 1 }[e.key] as -1 | 1 | undefined;
            if (!by) return;
            // Not the radio group's own arrow keys (they choose the next card)
            e.preventDefault();
            props.onMove?.(by);
          }}
        />
        <span class="codec-card-name">{name()}</span>
        {/* The wrapper keeps the tag its own size in the card's column */}
        <Show when={props.support}>
          {(s) => (
            <span class="codec-card-tags">
              {/* Unknown until the device's first connection */}
              <Show
                when={s() !== "unknown"}
                fallback={
                  <span class="tag">
                    <Icon name="question_circle" />
                    {t("ui.profile.codec_support_unknown")}
                  </span>
                }
              >
                {/* Only the mark in colour (issue #86) */}
                <span class="tag">
                  <Icon
                    name={s() === "offered" ? "checkmark" : "dismiss"}
                    class={s() === "offered" ? "st-good" : "st-bad"}
                  />
                  {s() === "offered" ? t("ui.profile.codec_supported") : t("ui.profile.codec_not_supported")}
                </span>
              </Show>
            </span>
          )}
        </Show>
        <Show when={props.spec.bars}>
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
        disabled={props.disabled && !props.readOnly}
        onClick={() => (props.readOnly ? props.onReadOnly?.() : props.onSettings())}
      />
    </div>
  );
}

export type CodecChoice = Pick<
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

export function codecChoice(p: CodecChoice): CodecChoice {
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

/** The SBC / SBC XQ bitpool range the dialog offers: the codec's, within
 *  the device's when known (issue #86: a device of max bitpool 39 is not
 *  offered 53; the core keeps the encoder within it the same way) */
export function bitpoolRange(codec: string, caps: SbcCaps | null): { min: number; max: number } {
  const local = codec === "sbcxq" ? BITPOOL_MAX_XQ : BITPOOL_MAX;
  if (!caps || caps.max_bitpool < BITPOOL_MIN) return { min: BITPOOL_MIN, max: local };
  const max = Math.max(Math.min(local, caps.max_bitpool), BITPOOL_MIN);
  return { min: Math.min(Math.max(caps.min_bitpool, BITPOOL_MIN), max), max };
}

/**
 * One codec's quality, sample rate, bit depth and ABR (what "Advanced" had
 * for the chosen codec); SBC and SBC XQ also their bitpool and frame setup,
 * offering only what the device listed when it last connected (`sbcCaps`,
 * and for every codec its rates, bit depths and largest bit rate: `caps`,
 * issue #86), with the bit rate that gives. A codec with one quality shows
 * it rather than a choice. It starts from the profile's values fitted
 * to the codec, or from what that codec was last set to (`kept`: the
 * profile's codec_settings); OK chooses the codec with them, Cancel leaves
 * the profile alone.
 */
export function CodecSettings(props: {
  spec: CodecSpec | null;
  profile: Profile;
  kept: (codec: string) => Omit<CodecChoice, "codec"> | undefined;
  sbcCaps: SbcCaps | null;
  /** Device.codec_caps (empty: not known) */
  caps?: readonly CodecCaps[];
  /** Developer > "aptX Adaptive at 96 kHz" (in force in developer mode
   *  only): off, aptX Adaptive is not offered 96 kHz */
  aptxad96k?: boolean;
  onApply: (values: CodecChoice) => void;
  onCancel: () => void;
}) {
  const [v, setV] = createStore<CodecChoice>({
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
        const kept = props.kept(s.id);
        const p = s.id === props.profile.codec || !kept ? props.profile : { ...kept, codec: s.id };
        // From another codec's values: without its ABR or SBC frame setup
        // (SBC and SBC XQ apart, issue #86)
        const from = carryToCodec(codecChoice(p), s.id);
        // SBC / SBC XQ: no quality to choose, Auto is the highest bitpool
        const fitted = s.bitpool ? { ...from, quality: "hq" } : from;
        // What the device does not list (or 96 kHz switched off) goes back
        // to Auto, a bitpool above the device's largest down to it, a
        // quality capped like a higher one to High (issue #86)
        const caps = capsOf(props.caps ?? [], s.id);
        if (!offeredSampleRates(s.id, caps, props.aptxad96k ?? true).includes(fitted.sample_rate)) fitted.sample_rate = 0;
        if (!offeredBitDepths(s.id, caps).includes(fitted.bit_depth)) fitted.bit_depth = 0;
        if (s.qualities.length > 0 && !offeredQualities(s.id, fitted.sample_rate, caps?.max_bitrate).some((q) => q.id === fitted.quality))
          fitted.quality = "hq";
        const range = bitpoolRange(s.id, props.sbcCaps);
        const key = s.id === "sbcxq" ? "sbcxq_bitpool" : "sbc_bitpool";
        if (s.bitpool && fitted[key] > range.max) fitted[key] = range.max;
        setV(fitted);
      },
    ),
  );
  const spec = () => codecSpec(v.codec);
  const name = () => t(spec().label);
  /** What the device listed for this codec (null: not known) */
  const caps = () => capsOf(props.caps ?? [], v.codec);
  const maxBitrate = () => caps()?.max_bitrate ?? 0;
  // SBC and SBC XQ keep their own bitpool
  const bpKey = () => (v.codec === "sbcxq" ? "sbcxq_bitpool" : "sbc_bitpool");
  const bp = () => v[bpKey()];
  const bpRange = () => bitpoolRange(v.codec, props.sbcCaps);
  const bpMin = () => bpRange().min;
  const bpMax = () => bpRange().max;
  const bitpoolError = () =>
    bp() !== 0 && !(Number.isInteger(bp()) && bp() >= bpMin() && bp() <= bpMax())
      ? t("error.sbc_bitpool_range", bpMin(), bpMax())
      : undefined;
  // The qualities with their bit rates on this device; one is shown, not chosen
  const qualities = () => offeredQualities(v.codec, v.sample_rate, maxBitrate(), v.quality);
  const qualityText = (q: { label: string; kbps?: string }) => (q.kbps ? `${t(q.label)} (${q.kbps} kbps)` : t(q.label));
  const rates = () => {
    const offered = offeredSampleRates(v.codec, caps(), props.aptxad96k ?? true);
    return offered.includes(v.sample_rate) ? offered : [...offered, v.sample_rate];
  };
  const depths = () => {
    const offered = offeredBitDepths(v.codec, caps());
    return offered.includes(v.bit_depth) ? offered : [...offered, v.bit_depth];
  };
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
          {/* One quality (or a fixed bit rate): shown, not a choice (issue
              #86: High is the highest bit rate the device takes) */}
          <Show
            when={qualities().length > 1}
            fallback={
              <Field
                label={t("connection.quality")}
                hint={t(spec().qualities.length ? "ui.profile.quality_highest_hint" : "tooltip.quality")}
              >
                {(id, described) => (
                  <output id={id} class="input readonly" aria-describedby={described}>
                    {qualities().length ? qualityText(qualities()[0]) : t("ui.profile.fixed_bitrate")}
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
                  disabled={qualityIgnored(v.codec, v.abr)}
                  options={qualities().map((q) => ({ value: q.id, label: qualityText(q) }))}
                  onChange={(x) => setV("quality", x)}
                />
              )}
            </Field>
          </Show>
        </Show>
        <Field label={t("connection.sample_rate")} hint={t("tooltip.sample_rate")}>
          {(id, described) => (
            <Select
              id={id}
              describedBy={described}
              value={v.sample_rate}
              options={rates().map((r) => {
                const label = r === 0 ? t("codec.auto") : `${r} Hz`;
                const listed = offeredSampleRates(v.codec, caps(), props.aptxad96k ?? true).includes(r);
                return { value: r, label: listed ? label : t("ui.profile.sbc_not_offered", label) };
              })}
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
              disabled={depths().length === 1}
              options={depths().map((b) => ({
                value: b,
                label:
                  b === 0
                    ? depths().length > 1
                      ? t("codec.auto")
                      : t(`bitdepth.${fixedBitDepth(v.codec)}`)
                    : offeredBitDepths(v.codec, caps()).includes(b)
                      ? t(`bitdepth.${b}`)
                      : t("ui.profile.sbc_not_offered", t(`bitdepth.${b}`)),
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
            hint={
              props.sbcCaps
                ? `${t("tooltip.sbc_bitpool", BITPOOL_MIN, bitpoolRange(v.codec, null).max)} ${t("ui.profile.sbc_bitpool_device", bpMin(), bpMax())}`
                : t("tooltip.sbc_bitpool", bpMin(), bpMax())
            }
            error={bitpoolError()}
          >
            {(id, described) => (
              <div class="inline">
                <input
                  id={id}
                  class="input num narrow"
                  type="number"
                  inputmode="numeric"
                  min={bpMin()}
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
