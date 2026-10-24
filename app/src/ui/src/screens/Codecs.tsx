// Codecs (issue #86, it replaced Profiles): each saved audio device with
// its codec cards, the codecs it offered when it last connected in green
// and the others in red (hidden unless asked for), each with its settings
// behind "...". Each device has "Auto select the highest supported codec in
// quality" (its profile's codec "auto"): on, the cards only show what Auto
// picks; off, choosing a card is choosing that device's codec, and a codec
// the device does not offer falls back to the next one below it in
// quality (lib/codecs.ts AUTO_ORDER, as the core). Either changes the
// device's profile (made when first needed; the core keeps one per device
// and starts a stream in use again with it). What belongs to the device
// rather than the codec (capture source, volume sync, packet size) is on
// Audio Devices. An LE Audio entry (experimental) has LC3 only.

import { createSignal, For, Show } from "solid-js";
import { unwrap } from "solid-js/store";
import { useApp } from "../app/context";
import { CodecCard, CodecSettings, type CodecChoice } from "../components/CodecCards";
import { Banner, Button, Check, HelpTip, Toggle } from "../components/controls";
import { Icon } from "../components/Icon";
import { t } from "../i18n";
import type { Profile } from "../ipc/types";
import { AUTO_ORDER, CODECS, codecLabelKey, codecSpec, codecSupport, effectiveCodec, qualityRank, type CodecSpec } from "../lib/codecs";
import {
  audioEntries,
  entryBase,
  entryByKey,
  followedName,
  knownCodecs,
  codecValues,
  newProfile,
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
  const snap = app.core.state.snap!;
  const cur = entry.profile ? structuredClone(unwrap(entry.profile)) : null;
  const base = cur ?? entryBase(entry, snap.profiles);
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
  // What the codec of each entry is not offered by: counted for the check box
  const hiddenOf = (e: AudioEntry) => {
    if (e.le_audio) return 0;
    const known = knownCodecs(snap().devices, e.address);
    const chosen = e.profile?.codec ?? "auto";
    return CODECS.filter((c) => c.id !== "auto" && c.id !== chosen && codecSupport(c.id, known) === "not_offered").length;
  };
  const hidden = () => entries().reduce((n, e) => n + hiddenOf(e), 0);

  return (
    <div class="page codecs">
      <header class="page-head">
        <h1>{t("ui.nav.codecs")}</h1>
      </header>
      <div class="codecs-options">
        <Check label={t("ui.profile.codecs_hide", hidden())} title={t("ui.codecs.hide_hint")} checked={hide()} onChange={setHide} />
      </div>
      <p class="hint">
        {t("ui.codecs.page_hint")}{" "}
        <button type="button" class="link" onClick={() => navigate({ name: "devices" })}>
          {t("ui.nav.devices")}
        </button>
      </p>
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
            return <DeviceCodecs entry={entry()} hide={hide()} />;
          }}
        </For>
      </Show>
    </div>
  );
}

/** One device: its name, address and tags, then its codec cards */
function DeviceCodecs(props: { entry: AudioEntry; hide: boolean }) {
  const app = useApp();
  const snap = () => app.core.state.snap!;
  const e = () => props.entry;
  const [busy, setBusy] = createSignal(false);
  // The profile shown: the device's, or what it would start from
  const shown = (): Profile => e().profile ?? entryBase(e(), snap().profiles);
  const codec = () => shown().codec;
  const auto = () => codec() === "auto";
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
  const picked = () => (auto() ? (streaming() ?? effectiveCodec("auto", known())) : codec());
  // A chosen codec the device did not offer: the one used instead
  const instead = () => (!auto() && support(codec()) === "not_offered" ? effectiveCodec(codec(), known()) : null);
  // Highest quality first, as Auto goes
  const cards = () =>
    CODECS.filter(
      (c) => c.id !== "auto" && (!props.hide || c.id === codec() || c.id === picked() || support(c.id) !== "not_offered"),
    ).sort((a, b) => qualityRank(a.id) - qualityRank(b.id));
  const [settingsOf, setSettingsOf] = createSignal<CodecSpec | null>(null);
  const choose = async (id: string, values?: Omit<CodecChoice, "codec">) => {
    setBusy(true);
    await chooseCodec(app, e(), id, values);
    setBusy(false);
  };
  // On: Auto with its own defaults (the codec left keeps its settings in
  // codec_settings). Off: the codec Auto picked (else SBC, which every
  // device has), with the settings it was last given.
  const setAuto = (on: boolean) => {
    if (on) void choose("auto", codecValues(newProfile()));
    else void choose(picked() ?? "sbc");
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
        {/* Nothing chosen yet: Auto, which Connect uses */}
        <Toggle
          label={t("ui.codecs.auto_select")}
          help={() => (
            <HelpTip label={t("ui.codecs.auto_order")}>
              {t("ui.codecs.auto_order")}
              <ol>
                <For each={AUTO_ORDER}>{(id) => <li>{t(codecSpec(id).label)}</li>}</For>
              </ol>
            </HelpTip>
          )}
          hint={
            auto()
              ? picked()
                ? t("ui.codecs.auto_uses", t(codecSpec(picked()!).label))
                : undefined
              : t("ui.codecs.auto_off")
          }
          checked={auto()}
          disabled={busy()}
          onChange={setAuto}
        />
        <div class="codec-grid" role="radiogroup" aria-labelledby={titleId()} aria-disabled={auto() ? "true" : undefined}>
          <For each={cards()}>
            {(c) => (
              <CodecCard
                spec={c}
                group={`codec-${e().key}`}
                selected={picked() === c.id}
                disabled={busy()}
                readOnly={auto()}
                support={support(c.id)}
                onPick={() => void choose(c.id)}
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
