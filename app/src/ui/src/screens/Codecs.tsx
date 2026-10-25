// Codecs (issue #86, it replaced Profiles): each saved audio device with
// its codec cards, the codecs it offered when it last connected in green
// and the others in red (hidden unless asked for), each with its settings
// behind "...". "Auto-select the codec with the highest quality" is one
// setting for every device (settings.auto_codec, on by default; each
// device had its own before): on, the cards only show what Auto picks;
// off, choosing a card is choosing that device's codec (a device with none
// chosen yet, its profile's codec "auto", goes on with Auto's pick), and a
// codec the device does not offer falls back to the next one below it in
// quality (lib/codecs.ts AUTO_ORDER, as the core). The codec chosen stays
// in the device's profile while Auto-select is on. Choosing changes the
// device's profile (made when first needed; the core keeps one per device
// and starts a stream in use again with it). What belongs to the device
// rather than the codec (capture source, volume sync, packet size) is on
// Audio Devices. An LE Audio entry (experimental) has LC3 only.
// Developer mode only: "Auto's order..." opens a dialog
// (components/AutoOrder.tsx) where the order Auto picks in, and a chosen
// codec falls back down, is set by dragging (settings.auto_order; empty:
// highest sound quality first); the cards are shown in that order. An
// order saved there still applies outside developer mode.
// Every codec is offered, the experimental ones too (issue #86: they are
// marked in the README only; the core no longer reads
// settings.experimental_codecs).

import { createSignal, For, Show } from "solid-js";
import { unwrap } from "solid-js/store";
import { useApp } from "../app/context";
import { saveSettings } from "../app/settings";
import { AutoOrderDialog } from "../components/AutoOrder";
import { CodecCard, CodecSettings, type CodecChoice } from "../components/CodecCards";
import { Banner, Button, Check, Toggle } from "../components/controls";
import { audioDevices } from "../components/DevicePicker";
import { Icon } from "../components/Icon";
import { t } from "../i18n";
import type { Profile } from "../ipc/types";
import {
  autoOrder,
  CODECS,
  codecLabelKey,
  codecSpec,
  codecSupport,
  effectiveCodec,
  isDefaultOrder,
  qualityRank,
  type CodecId,
  type CodecSpec,
} from "../lib/codecs";
import {
  audioEntries,
  entryBase,
  entryByKey,
  followedName,
  knownCodecs,
  normalizeProfile,
  withCodec,
  type AudioEntry,
} from "../lib/profile";
import { knownSbcCaps } from "../lib/sbc";

const HIDE_KEY = "a2dpwb.codecs.hide_unsupported";

function loadHide(): boolean {
  try {
    return localStorage.getItem(HIDE_KEY) !== "0";
  } catch {
    return true;
  }
}

/** Saves the device's profile with `codec` chosen (`values`: from its "..."
 *  dialog); its name follows the codec while it is "<device> | <codec>".
 *  The profile's name once saved, None if that failed. */
export async function chooseCodec(
  app: ReturnType<typeof useApp>,
  entry: AudioEntry,
  codec: string,
  values?: Omit<CodecChoice, "codec">,
): Promise<string | null> {
  // A device's first profile captures from a virtual device if one is installed
  const devices = entry.profile ? [] : await audioDevices(app);
  const snap = app.core.state.snap!;
  const cur = entry.profile ? structuredClone(unwrap(entry.profile)) : null;
  const base = cur ?? entryBase(entry, snap.profiles, devices);
  const next = entry.le_audio ? base : withCodec(base, codec, values);
  const taken = snap.profiles.map((p) => p.name).filter((n) => n !== cur?.name);
  next.name = followedName(next, entry.le_audio ? "" : t(codecSpec(next.codec).label), taken);
  if (cur && JSON.stringify(next) === JSON.stringify(cur)) return cur.name;
  const saved = normalizeProfile(next);
  const ok = await app.run(() => app.core.request("profile.save", { profile: saved, original_name: cur?.name ?? null }));
  return ok ? saved.name : null;
}

export function Codecs() {
  const app = useApp();
  const { core, navigate } = app;
  const snap = () => core.state.snap!;
  const entries = () => audioEntries(snap());
  const [hide, setHideRaw] = createSignal(loadHide());
  const setHide = (v: boolean) => {
    setHideRaw(v);
    try {
      localStorage.setItem(HIDE_KEY, v ? "1" : "0");
    } catch {
      // Only remembered when the WebView keeps storage
    }
  };
  const autoAll = () => snap().settings.auto_codec;
  const order = () => autoOrder(snap().settings.auto_order);
  const dev = () => snap().settings.developer_mode;
  const [ordering, setOrdering] = createSignal(false);

  return (
    <div class="page codecs">
      <header class="page-head">
        <h1>{t("ui.nav.codecs")}</h1>
      </header>
      <div class="codecs-options">
        <Toggle
          label={t("ui.codecs.auto_select")}
          checked={autoAll()}
          onChange={(v) => void saveSettings(app, { auto_codec: v })}
        />
        <Show when={dev()}>
          <div class="codecs-order">
            <Button icon="settings" onClick={() => setOrdering(true)} aria-haspopup="dialog">
              {t("ui.codecs.order_edit")}
            </Button>
            <span class="hint">{isDefaultOrder(order()) ? t("ui.codecs.order_default") : t("ui.codecs.order_custom")}</span>
          </div>
          <AutoOrderDialog
            open={ordering()}
            order={order()}
            onCancel={() => setOrdering(false)}
            onSave={(next: CodecId[]) => {
              setOrdering(false);
              void saveSettings(app, { auto_order: isDefaultOrder(next) ? [] : next });
            }}
          />
        </Show>
        <Toggle label={t("ui.profile.codecs_hide")} checked={hide()} onChange={setHide} />
      </div>
      <Show
        when={entries().length > 0}
        fallback={
          <section class="card empty">
            <Icon name="headphones" class="empty-icon" />
            <p>{t("ui.home.no_device")}</p>
            <Button kind="strong" icon="add" onClick={() => navigate({ name: "devices" })}>
              {t("ui.home.add_device")}
            </Button>
          </section>
        }
      >
        {/* By key: a device keeps its section (and an open dialog) across updates */}
        <For each={entries().map((e) => e.key)}>
          {(key) => {
            const entry = entryByKey(entries, key);
            return (
              <DeviceCodecs entry={entry()} hide={hide()} auto={autoAll()} order={order()} />
            );
          }}
        </For>
      </Show>
    </div>
  );
}

/** One device: its name, address and tags, then its codec cards */
function DeviceCodecs(props: {
  entry: AudioEntry;
  hide: boolean;
  auto: boolean;
  order: readonly CodecId[];
}) {
  const app = useApp();
  const snap = () => app.core.state.snap!;
  const e = () => props.entry;
  const [busy, setBusy] = createSignal(false);
  // The profile shown: the device's, or what it would start from
  const shown = (): Profile => e().profile ?? entryBase(e(), snap().profiles);
  const codec = () => shown().codec;
  // Auto-select (for every device) on, or no codec chosen for the device yet
  const auto = () => props.auto || codec() === "auto";
  const known = () => knownCodecs(snap().devices, e().address);
  const support = (id: string) => codecSupport(id, known());
  // The codec the device streams with now, when its profile streams
  const streaming = () => {
    const st = snap().stream;
    const p = e().profile;
    if (st.state !== "streaming" || !st.info || !p || st.profile !== p.name) return null;
    const key = codecLabelKey(st.info.codec);
    return CODECS.find((c) => c.id !== "auto" && c.label === key)?.id ?? null;
  };
  // Auto: the codec it streams with, else the one it would pick (null when
  // the device's codecs are not known); else the codec chosen
  const picked = () => (auto() ? (streaming() ?? effectiveCodec("auto", known(), true, props.order)) : codec());
  // A chosen codec the device did not offer: the one used instead (null:
  // the device's codecs are not known yet)
  const instead = () =>
    !auto() && support(codec()) === "not_offered" ? effectiveCodec(codec(), known(), true, props.order) : null;
  // In Auto's order (highest quality first unless set)
  const cards = () =>
    CODECS.filter(
      (c) =>
        c.id !== "auto" &&
        (!props.hide || c.id === codec() || c.id === picked() || support(c.id) !== "not_offered"),
    ).sort((a, b) => qualityRank(a.id, props.order) - qualityRank(b.id, props.order));
  const [settingsOf, setSettingsOf] = createSignal<CodecSpec | null>(null);
  const choose = async (id: string, values?: Omit<CodecChoice, "codec">) => {
    setBusy(true);
    await chooseCodec(app, e(), id, values);
    setBusy(false);
  };
  const titleId = () => `codecs-${e().key.replace(/[^0-9A-Za-z]/g, "")}`;
  const leSupport = () => snap().adapter.le_audio;

  return (
    <section class="card device-codecs" aria-labelledby={titleId()} aria-busy={busy() ? "true" : undefined}>
      <div class="device-codecs-head">
        <Icon name="headphones" />
        <div class="device-codecs-title">
          <h2 id={titleId()} class="ellipsis">
            {e().name || t("device.name_unknown")}
          </h2>
          <span class="mono muted">{e().address}</span>
        </div>
        <span class="tags">
          <span class="tag">{t("device.tag_saved")}</span>
          <Show when={e().device.audio}>
            <span class="tag">{t("device.tag_audio")}</span>
          </Show>
          <Show when={e().le_audio}>
            <span class="tag">{t("device.tag_le_audio")}</span>
          </Show>
        </span>
      </div>
      <Show
        when={!e().le_audio}
        fallback={
          // LE Audio: LC3, the configuration the device offers best
          <>
            <div class="codec-grid" role="radiogroup" aria-labelledby={titleId()}>
              <div class="codec-cell">
                <label class="codec-card" classList={{ on: e().profile !== null }}>
                  <input
                    type="radio"
                    name={`codec-${e().key}`}
                    value="lc3"
                    checked={e().profile !== null}
                    disabled={busy()}
                    onChange={() => void choose("auto")}
                  />
                  <span class="codec-card-name">{t("ui.codec.lc3")}</span>
                  <span class="muted codec-card-desc">{t("ui.codec.lc3_desc")}</span>
                </label>
              </div>
            </div>
            <Show when={e().profile}>
              {(p) => (
                <Check
                  label={t("settings.le_low_latency")}
                  title={t("ui.settings.le_low_latency_hint")}
                  checked={p().le_low_latency}
                  disabled={busy()}
                  onChange={(v) =>
                    void app.run(() =>
                      app.core.request("profile.save", {
                        profile: { ...structuredClone(unwrap(p())), le_low_latency: v },
                        original_name: p().name,
                      }),
                    )
                  }
                />
              )}
            </Show>
            <Show when={leSupport() !== true}>
              <Banner kind="warn">
                {leSupport() === false ? t("ui.profile.le_audio_unsupported") : t("ui.profile.le_audio_unchecked")}
              </Banner>
            </Show>
          </>
        }
      >
        <div class="codec-grid" role="radiogroup" aria-labelledby={titleId()} aria-disabled={props.auto ? "true" : undefined}>
          <For each={cards()}>
            {(c) => (
              <CodecCard
                spec={c}
                group={`codec-${e().key}`}
                selected={picked() === c.id}
                disabled={busy()}
                readOnly={props.auto}
                support={support(c.id)}
                onPick={() => void choose(c.id)}
                onReadOnly={() => app.notify(t("ui.codecs.auto_read_only"))}
                onSettings={() => setSettingsOf(c)}
              />
            )}
          </For>
        </div>
        <Show when={known().length === 0}>
          <p class="hint">{t("ui.profile.codecs_unknown")}</p>
        </Show>
        <Show when={instead()}>
          {(c) => (
            <Banner kind="warn" title={t("error.codec_not_supported", t(codecSpec(codec()).label))}>
              {t("ui.codecs.fallback_to", t(codecSpec(c()).label))}
            </Banner>
          )}
        </Show>
        <CodecSettings
          spec={settingsOf()}
          profile={shown()}
          kept={(id) => shown().codec_settings?.[id]}
          sbcCaps={knownSbcCaps(snap().devices, e().address)}
          onApply={(values) => {
            const { codec: id, ...rest } = values;
            setSettingsOf(null);
            void choose(id, rest);
          }}
          onCancel={() => setSettingsOf(null)}
        />
      </Show>
    </section>
  );
}
