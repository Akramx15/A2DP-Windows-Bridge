// Codecs (issue #86, it replaced Profiles): each saved audio device with
// its codec cards, Auto and the codecs it offered when it last connected
// (the others hidden unless asked for), each with its settings behind
// "...". Choosing a card is choosing that device's codec: it changes the
// device's profile (made when first needed; the core keeps one per device
// and starts a stream in use again with it). What belongs to the device
// rather than the codec (capture source, volume sync, packet size) is on
// Audio Devices. An LE Audio entry (experimental) has LC3 only.

import { createSignal, For, Show } from "solid-js";
import { unwrap } from "solid-js/store";
import { useApp } from "../app/context";
import { CodecCard, CodecSettings, type CodecChoice } from "../components/CodecCards";
import { Banner, Button, Check } from "../components/controls";
import { Icon } from "../components/Icon";
import { t } from "../i18n";
import type { Profile } from "../ipc/types";
import { CODECS, codecSpec, codecSupport, type CodecSpec } from "../lib/codecs";
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
    return CODECS.filter((c) => c.id !== chosen && codecSupport(c.id, known) === "not_offered").length;
  };
  const hidden = () => entries().reduce((n, e) => n + hiddenOf(e), 0);
  // The fallback is each profile's; the check box sets it in all of them
  const classic = () => snap().profiles.filter((p) => !p.le_audio);
  const fallback = () => classic().length === 0 || classic().every((p) => p.codec_fallback);
  const [savingAll, setSavingAll] = createSignal(false);
  const setFallback = async (v: boolean) => {
    setSavingAll(true);
    for (const p of classic()) {
      if (p.codec_fallback === v) continue;
      const ok = await app.run(() =>
        core.request("profile.save", { profile: { ...structuredClone(unwrap(p)), codec_fallback: v }, original_name: p.name }),
      );
      if (!ok) break;
    }
    setSavingAll(false);
  };

  return (
    <div class="page codecs">
      <header class="page-head">
        <h1>{t("ui.nav.codecs")}</h1>
      </header>
      <div class="codecs-options" aria-busy={savingAll() ? "true" : undefined}>
        <Check
          label={t("ui.profile.codecs_hide", hidden())}
          title={t("ui.profile.codecs_force_hint")}
          checked={hide()}
          onChange={setHide}
        />
        <Check
          label={t("connection.codec_fallback")}
          title={t("tooltip.codec_fallback")}
          checked={fallback()}
          disabled={classic().length === 0 || savingAll()}
          onChange={(v) => void setFallback(v)}
        />
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
  const known = () => knownCodecs(snap().devices, e().address);
  const support = (id: string) => codecSupport(id, known());
  const cards = () => CODECS.filter((c) => !props.hide || c.id === codec() || support(c.id) !== "not_offered");
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
        <div class="codec-grid" role="radiogroup" aria-labelledby={titleId()}>
          <For each={cards()}>
            {(c) => (
              <CodecCard
                spec={c}
                group={`codec-${e().key}`}
                // Nothing chosen yet: Auto is what Connect uses
                selected={codec() === c.id}
                disabled={busy()}
                support={c.id === "auto" ? null : support(c.id)}
                onPick={() => void choose(c.id)}
                onSettings={() => setSettingsOf(c)}
              />
            )}
          </For>
        </div>
        <Show when={known().length === 0}>
          <p class="hint">{t("ui.profile.codecs_unknown")}</p>
        </Show>
        <Show when={support(codec()) === "not_offered"}>
          <Banner kind="warn" title={t("error.codec_not_supported", t(codecSpec(codec()).label))}>
            {shown().codec_fallback ? t("ui.profile.codec_fallback_on") : t("ui.profile.codec_fallback_off")}
          </Banner>
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
