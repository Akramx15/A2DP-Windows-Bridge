// Codecs (issue #86, it replaced Profiles): each saved audio device with
// its codec cards, the codecs it offered when it last connected in green
// and the others in red (hidden unless asked for), each with its settings
// behind "...", and a dot beside the device's name that is green while it
// streams. "Auto-select the codec with the highest quality" is one setting
// for every device (settings.auto_codec, on by default; each device had its
// own before): on, the cards only show what Auto picks, in the default
// order (highest sound quality first), and a click on one, or on its "...",
// offers to turn Auto-select off; off, choosing a card is choosing that
// device's codec (a device with none chosen yet, its profile's codec
// "auto", goes on with Auto's pick), and the cards are dragged (or moved
// with Alt+arrow keys) into the device's own order (its profile's
// codec_order, issue #86 round 8). A codec the device does not offer falls
// back to the first card it offers in that order (lib/codecs.ts
// effectiveCodec, as the core), which is also what Auto picks then. The
// codec chosen stays in the device's profile while Auto-select is on.
// Choosing changes the device's profile (made when first needed; the core
// keeps one per device and starts a stream in use again with it). What
// belongs to the device rather than the codec (capture source, volume sync,
// packet size) is on Audio Devices. An LE Audio entry (experimental) has LC3
// only.
// Every codec is offered, the experimental ones too (issue #86: they are
// marked in the README only; the core no longer reads
// settings.experimental_codecs).

import { createEffect, createSignal, createUniqueId, For, on, Show } from "solid-js";
import { unwrap } from "solid-js/store";
import { useApp } from "../app/context";
import { saveSettings } from "../app/settings";
import { CodecCard, CodecSettings, type CodecChoice } from "../components/CodecCards";
import { Banner, Button, Check, Dialog, Toggle } from "../components/controls";
import { audioDevices } from "../components/DevicePicker";
import { Icon } from "../components/Icon";
import { stateLabel } from "../components/stream";
import { t } from "../i18n";
import type { Profile } from "../ipc/types";
import {
  AUTO_ORDER,
  CARD_ORDER,
  cardOrder,
  CODECS,
  codecLabelKey,
  codecSpec,
  codecSupport,
  effectiveCodec,
  isDefaultOrder,
  moved,
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
import { entryState } from "./Home";

const HIDE_KEY = "a2dpwb.codecs.hide_unsupported";

function loadHide(): boolean {
  try {
    return localStorage.getItem(HIDE_KEY) !== "0";
  } catch {
    return true;
  }
}

/** Saves the device's profile as `edit` changes it (the profile is made
 *  when the device has none yet); its name follows the codec while it is
 *  "<device> | <codec>". The profile's name once saved, None if that failed. */
async function saveEntry(
  app: ReturnType<typeof useApp>,
  entry: AudioEntry,
  edit: (p: Profile) => Profile,
): Promise<string | null> {
  // A device's first profile captures from a virtual device if one is installed
  const devices = entry.profile ? [] : await audioDevices(app);
  const snap = app.core.state.snap!;
  const cur = entry.profile ? structuredClone(unwrap(entry.profile)) : null;
  const next = edit(cur ?? entryBase(entry, snap.profiles, devices));
  const taken = snap.profiles.map((p) => p.name).filter((n) => n !== cur?.name);
  next.name = followedName(next, entry.le_audio ? "" : t(codecSpec(next.codec).label), taken);
  if (cur && JSON.stringify(next) === JSON.stringify(cur)) return cur.name;
  const saved = normalizeProfile(next);
  const ok = await app.run(() => app.core.request("profile.save", { profile: saved, original_name: cur?.name ?? null }));
  return ok ? saved.name : null;
}

/** Saves the device's profile with `codec` chosen (`values`: from its "..."
 *  dialog; `order`: the device's codec order, when one is not saved yet).
 *  The profile's name once saved, None if that failed. */
export function chooseCodec(
  app: ReturnType<typeof useApp>,
  entry: AudioEntry,
  codec: string,
  values?: Omit<CodecChoice, "codec">,
  order?: readonly CodecId[] | null,
): Promise<string | null> {
  return saveEntry(app, entry, (base) => {
    if (entry.le_audio) return base;
    const next = withCodec(base, codec, values);
    return order ? { ...next, codec_order: orderKeys(order) } : next;
  });
}

/** A codec order as a profile keeps it: none for the default one */
function orderKeys(order: readonly CodecId[]): string[] {
  return isDefaultOrder(order) ? [] : [...order];
}

/** Saves the device's codec order (its cards' order) */
export function saveCodecOrder(app: ReturnType<typeof useApp>, entry: AudioEntry, order: readonly CodecId[]) {
  return saveEntry(app, entry, (base) => ({ ...base, codec_order: orderKeys(order) }));
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
  // A card, or its "...", clicked under Auto-select (issue #86 round 8)
  const [locked, setLocked] = createSignal(false);

  return (
    <div class="page codecs">
      <header class="page-head">
        <h1>{t("ui.nav.codecs")}</h1>
      </header>
      <div class="codecs-options">
        <Toggle
          label={t("ui.codecs.auto_select")}
          hint={t("ui.codecs.auto_select_hint")}
          checked={autoAll()}
          onChange={(v) => void saveSettings(app, { auto_codec: v })}
        />
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
            return <DeviceCodecs entry={entry()} hide={hide()} auto={autoAll()} onLocked={() => setLocked(true)} />;
          }}
        </For>
      </Show>
      <Dialog
        open={locked()}
        title={t("ui.codecs.auto_dialog_title")}
        confirm={t("ui.codecs.auto_dialog_disable")}
        cancel={t("modal.cancel")}
        onConfirm={() => {
          setLocked(false);
          // The codec stays as Auto-select left it: the user chooses next
          void saveSettings(app, { auto_codec: false });
        }}
        onCancel={() => setLocked(false)}
      >
        <p>{t("ui.codecs.auto_dialog_text")}</p>
      </Dialog>
    </div>
  );
}

/** One device: its activity, name, address and tags, then its codec cards */
function DeviceCodecs(props: { entry: AudioEntry; hide: boolean; auto: boolean; onLocked: () => void }) {
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
  // The device's order while it is being dragged (null: the saved one)
  const [dragOrder, setDragOrder] = createSignal<CodecId[] | null>(null);
  // Its cards' order: Auto-select's is the default one (highest sound
  // quality first), read-only
  const order = (): CodecId[] => (props.auto ? [...CARD_ORDER] : (dragOrder() ?? cardOrder(shown().codec_order)));
  // The order Auto and the fallback go by
  const pickOrder = (): readonly string[] => (props.auto ? AUTO_ORDER : order());
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
  const picked = () => (auto() ? (streaming() ?? effectiveCodec("auto", known(), true, pickOrder())) : codec());
  // A chosen codec the device did not offer: the one used instead, the
  // first card it offers (null: the device's codecs are not known yet)
  const instead = () =>
    !auto() && support(codec()) === "not_offered" ? effectiveCodec(codec(), known(), true, pickOrder()) : null;
  const cards = () =>
    order()
      .filter((id) => !props.hide || id === codec() || id === picked() || support(id) !== "not_offered")
      .map((id) => codecSpec(id));
  const [settingsOf, setSettingsOf] = createSignal<CodecSpec | null>(null);
  // The device's saves one after the other: a card chosen right after one
  // was moved starts from the profile with the new order
  let saving: Promise<unknown> = Promise.resolve();
  const queued = <T,>(save: () => Promise<T>): Promise<T> => {
    const done = saving.then(save);
    saving = done.catch(() => undefined);
    return done;
  };
  const choose = async (id: string, values?: Omit<CodecChoice, "codec">) => {
    setBusy(true);
    // An order moved but not back from the core yet goes with it
    await queued(() => chooseCodec(app, e(), id, values, props.auto ? null : dragOrder()));
    setBusy(false);
  };
  const titleId = () => `codecs-${e().key.replace(/[^0-9A-Za-z]/g, "")}`;
  const leSupport = () => snap().adapter.le_audio;
  const state = () => entryState(snap(), e());

  // ---- the device's order (issue #86 round 8) ----
  // Not `busy`: the card moved with the keyboard keeps the focus
  const [ordering, setOrdering] = createSignal(false);
  const hintId = createUniqueId();
  const [said, setSaid] = createSignal("");
  // A saved order replaces the one dragged
  createEffect(on(() => shown().codec_order.join(), () => setDragOrder(null), { defer: true }));
  const saveOrder = async (next: CodecId[]) => {
    if (next.every((c, i) => c === cardOrder(shown().codec_order)[i])) {
      setDragOrder(null);
      return;
    }
    setDragOrder(next);
    setOrdering(true);
    const ok = await queued(() => saveCodecOrder(app, e(), next));
    setOrdering(false);
    if (!ok) setDragOrder(null);
  };
  // `id` put where `over` is in the full order (hidden cards keep their place)
  const placed = (list: CodecId[], id: CodecId, over: CodecId) => moved(list, list.indexOf(id), list.indexOf(over));
  let grid: HTMLDivElement | undefined;
  const focusCard = (id: CodecId) =>
    queueMicrotask(() => grid?.querySelector<HTMLInputElement>(`input[value="${id}"]`)?.focus());
  // Alt+arrow keys: next to the card before or after it among those shown
  const moveBy = (id: CodecId, by: -1 | 1) => {
    const shownIds = cards().map((c) => c.id);
    const at = shownIds.indexOf(id);
    const to = shownIds[at + by];
    if (!to) return;
    const next = placed(order(), id, to);
    setSaid(t("ui.codecs.moved", t(codecSpec(id).label), at + by + 1, shownIds.length));
    void saveOrder(next);
    focusCard(id);
  };
  // Dragging with the pointer: the card follows it over the others and the
  // order is saved when it is let go (a click without moving still chooses)
  let drag: { id: CodecId; x: number; y: number; pointer: number; moving: boolean } | null = null;
  let swallowClick = false;
  const [dragging, setDragging] = createSignal<CodecId | null>(null);
  const cellAt = (x: number, y: number): CodecId | null => {
    const el = document.elementFromPoint?.(x, y);
    const cell = el?.closest<HTMLElement>(".codec-cell[data-codec]");
    return cell && grid?.contains(cell) ? (cell.dataset.codec as CodecId) : null;
  };
  const onPointerDown = (ev: PointerEvent) => {
    if (props.auto || busy() || ev.button !== 0) return;
    const target = ev.target as Element;
    const cell = target.closest<HTMLElement>(".codec-cell[data-codec]");
    if (!cell || target.closest(".codec-more")) return;
    drag = { id: cell.dataset.codec as CodecId, x: ev.clientX, y: ev.clientY, pointer: ev.pointerId, moving: false };
  };
  const onPointerMove = (ev: PointerEvent) => {
    if (!drag || ev.pointerId !== drag.pointer) return;
    if (!drag.moving) {
      if (Math.hypot(ev.clientX - drag.x, ev.clientY - drag.y) < 6) return;
      drag.moving = true;
      setDragging(drag.id);
      grid?.setPointerCapture?.(ev.pointerId);
    }
    ev.preventDefault();
    const over = cellAt(ev.clientX, ev.clientY);
    if (over && over !== drag.id) setDragOrder(placed(order(), drag.id, over));
  };
  const endDrag = (ev: PointerEvent, cancelled: boolean) => {
    if (!drag || ev.pointerId !== drag.pointer) return;
    const d = drag;
    drag = null;
    setDragging(null);
    if (!d.moving) return;
    // The click that ends a drag does not choose the card
    swallowClick = true;
    setTimeout(() => (swallowClick = false), 0);
    if (cancelled) setDragOrder(null);
    else {
      // Where it is let go, if the last move did not get there
      const over = cellAt(ev.clientX, ev.clientY);
      if (over && over !== d.id) setDragOrder(placed(order(), d.id, over));
      const next = order();
      const at = cards().findIndex((c) => c.id === d.id);
      setSaid(t("ui.codecs.moved", t(codecSpec(d.id).label), at + 1, cards().length));
      void saveOrder(next);
    }
  };

  return (
    <section
      class="card device-codecs"
      aria-labelledby={titleId()}
      aria-busy={busy() || ordering() ? "true" : undefined}
    >
      <div class="device-codecs-head">
        {/* Which device is in use (issue #86 round 8), as the sidebar's dot */}
        <span class={`dot st-${state()}`} role="img" aria-label={stateLabel(state())} title={stateLabel(state())} />
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
        <div
          ref={grid}
          class="codec-grid"
          classList={{ ordering: dragging() !== null }}
          role="radiogroup"
          aria-labelledby={titleId()}
          aria-disabled={props.auto ? "true" : undefined}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={(ev) => endDrag(ev, false)}
          onPointerCancel={(ev) => endDrag(ev, true)}
          onClick={(ev) => {
            if (!swallowClick) return;
            swallowClick = false;
            ev.preventDefault();
            ev.stopPropagation();
          }}
        >
          <For each={cards()}>
            {(c) => (
              <CodecCard
                spec={c}
                group={`codec-${e().key}`}
                selected={picked() === c.id}
                disabled={busy()}
                readOnly={props.auto}
                support={support(c.id)}
                orderable={!props.auto}
                dragging={dragging() === c.id}
                describedBy={hintId}
                onPick={() => void choose(c.id)}
                onReadOnly={props.onLocked}
                onSettings={() => setSettingsOf(c)}
                onMove={(by) => moveBy(c.id, by)}
              />
            )}
          </For>
        </div>
        <span id={hintId} class="sr-only">
          {t("ui.codecs.order_keys")}
        </span>
        <span class="sr-only" role="status" aria-live="polite">
          {said()}
        </span>
        <Show when={!props.auto && !isDefaultOrder(order())}>
          <div class="codecs-order">
            <Button icon="arrow_reset" disabled={busy() || ordering()} onClick={() => void saveOrder([...CARD_ORDER])}>
              {t("ui.codecs.order_reset")}
            </Button>
          </div>
        </Show>
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
