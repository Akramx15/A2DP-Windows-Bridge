// The codec cards of the Codecs screen: one codec with its three
// comparison bars and whether the device supported it when it last
// connected (green / red, issue #86), and behind "..." that codec's
// quality, sample rate, bit depth
// and ABR (SBC and SBC XQ: bitpool and frame setup). The codecs offered
// and what they can be set to come from lib/codecs.ts.

import { createEffect, on, Show } from "solid-js";
import { createStore } from "solid-js/store";
import { t } from "../i18n";
import type { Profile } from "../ipc/types";
import {
  abrCarries,
  bitDepths,
  codecSpec,
  fitToCodec,
  BITPOOL_MAX,
  BITPOOL_MAX_XQ,
  BITPOOL_MIN,
  qualityIgnored,
  sampleRates,
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
  /** Auto chooses (issue #86): the card only shows whether Auto picked it,
   *  without its settings */
  readOnly?: boolean;
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
      <label class="codec-card" classList={{ on: props.selected, ro: props.readOnly }}>
        <input
          type="radio"
          name={props.group}
          value={props.spec.id}
          checked={props.selected}
          disabled={props.disabled || props.readOnly}
          onChange={() => props.onPick()}
        />
        <span class="codec-card-name">{name()}</span>
        {/* The wrapper keeps the tag its own size in the card's column */}
        <Show when={props.support !== "unknown" && props.support}>
          {(s) => (
            <span>
              <span class={`tag ${s() === "offered" ? "st-good" : "st-bad"}`}>
                <Icon name={s() === "offered" ? "checkmark" : "dismiss"} />
                {s() === "offered" ? t("ui.profile.codec_supported") : t("ui.profile.codec_not_supported")}
              </span>
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
      <Show when={!props.readOnly}>
        <IconButton
          icon="more_horizontal"
          class="codec-more"
          label={t("ui.profile.codec_settings", name())}
          aria-haspopup="dialog"
          onClick={() => props.onSettings()}
        />
      </Show>
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

/**
 * One codec's quality, sample rate, bit depth and ABR (what "Advanced" had
 * for the chosen codec); SBC and SBC XQ also their bitpool and frame setup,
 * offering only what the device listed when it last connected (`sbcCaps`),
 * with the bit rate that gives. It starts from the profile's values fitted
 * to the codec, or from what that codec was last set to (`kept`: the
 * profile's codec_settings); OK chooses the codec with them, Cancel leaves
 * the profile alone.
 */
export function CodecSettings(props: {
  spec: CodecSpec | null;
  profile: Profile;
  kept: (codec: string) => Omit<CodecChoice, "codec"> | undefined;
  sbcCaps: SbcCaps | null;
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
