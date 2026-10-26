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

import { batch, createEffect, createSignal, createUniqueId, For, on, Show } from "solid-js";
import { unwrap } from "solid-js/store";
import { useApp } from "../app/context";
import { saveSettings } from "../app/settings";
import { CodecCard, CodecSettings, type CodecChoice } from "../components/CodecCards";
import { Banner, Button, Check, Dialog, Toggle } from "../components/controls";
import { audioDevices } from "../components/DevicePicker";
import { Icon } from "../components/Icon";
import { stateLabel } from "../components/stream";
import { t } from "../i18n";
import type { Profile, Snapshot } from "../ipc/types";
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
  knownCodecCaps,
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

/** The codec a device streams with now, when its profile streams */
export function streamingCodec(snap: Snapshot, entry: AudioEntry): CodecId | null {
  const st = snap.stream;
  const p = entry.profile;
  if (st.state !== "streaming" || !st.info || !p || st.profile !== p.name) return null;
  const key = codecLabelKey(st.info.codec);
  return CODECS.find((c) => c.id !== "auto" && c.label === key)?.id ?? null;
}

/** The codec a device's cards (and Home's Codec card) show chosen: under
 *  Auto (Auto-select on, `autoAll`, or no codec chosen for the device yet)
 *  the one it streams with, else the one it would pick (null when the
 *  device's codecs are not known); else the codec chosen. `order`: its
 *  cards' order. */
export function pickedCodec(
  snap: Snapshot,
  entry: AudioEntry,
  autoAll: boolean,
  order: readonly CodecId[],
): string | null {
  const codec = (entry.profile ?? entryBase(entry, snap.profiles)).codec;
  if (!autoAll && codec !== "auto") return codec;
  const known = knownCodecs(snap.devices, entry.address);
  return streamingCodec(snap, entry) ?? effectiveCodec("auto", known, true, autoAll ? AUTO_ORDER : order);
}

/** Whether a codec's card shows: the codec chosen and the one picked
 *  always, the others unless `hide` and the device did not offer them */
export function codecShown(id: string, codec: string, picked: string | null, known: readonly string[], hide: boolean) {
  return !hide || id === codec || id === picked || codecSupport(id, known) !== "not_offered";
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
        {/* "Show only supported codecs", on unless turned off (kept per
            WebView, under the key it always had) */}
        <Toggle
          label={t("ui.profile.codecs_hide")}
          hint={t("ui.profile.codecs_hide_hint")}
          checked={hide()}
          onChange={setHide}
        />
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
  const picked = () => pickedCodec(snap(), e(), props.auto, order());
  // A chosen codec the device did not offer: the one used instead, the
  // first card it offers (null: the device's codecs are not known yet)
  const instead = () =>
    !auto() && support(codec()) === "not_offered" ? effectiveCodec(codec(), known(), true, pickOrder()) : null;
  const cards = () =>
    order()
      .filter((id) => codecShown(id, codec(), picked(), known(), props.hide))
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

  // ---- motion (issue #86 round 9) ----
  // The card dragged is lifted and follows the pointer, the others slide to
  // their new places (FLIP) instead of jumping, and the card let go settles
  // into its own; moved with the keyboard or reset, the cards slide too.
  // No sliding with prefers-reduced-motion (the card still follows the
  // pointer: that is the drag itself, not an animation).
  const cells = () => [...(grid?.querySelectorAll<HTMLElement>(".codec-cell[data-codec]") ?? [])];
  const cellOf = (id: CodecId) => cells().find((c) => c.dataset.codec === id);
  const still = () => window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  const SLIDE = "codec-slide";
  const slide = (el: HTMLElement, dx: number, dy: number) => {
    if (dx || dy)
      el.animate?.([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }], {
        id: SLIDE,
        duration: 200,
        easing: "cubic-bezier(0.2, 0.8, 0.2, 1)",
      });
  };
  const slides = (el: HTMLElement) => (el.getAnimations?.() ?? []).filter((a) => a.id === SLIDE);
  /** Runs `change` (a new order), then slides each card but `except` from
   *  where it was on screen (mid-slide too) to its new place */
  const flip = (change: () => void, except?: CodecId) => {
    const before = new Map(cells().map((c) => [c.dataset.codec, c.getBoundingClientRect()]));
    batch(change);
    if (still()) return;
    for (const c of cells()) {
      const from = before.get(c.dataset.codec);
      if (!from || c.dataset.codec === except) continue;
      for (const a of slides(c)) a.cancel();
      const to = c.getBoundingClientRect();
      slide(c, from.left - to.left, from.top - to.top);
    }
  };
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
    flip(() => void saveOrder(next));
    focusCard(id);
  };
  // Dragging with the pointer: the card follows it over the others and the
  // order is saved when it is let go (a click without moving still chooses).
  // `left` / `top`: the card's place in the grid when it was picked up.
  let drag: {
    id: CodecId;
    x: number;
    y: number;
    pointer: number;
    moving: boolean;
    left: number;
    top: number;
  } | null = null;
  let swallowClick = false;
  const [dragging, setDragging] = createSignal<CodecId | null>(null);
  // The card under the pointer (the one dragged lets the pointer through);
  // not one still sliding away, which would swap straight back
  const cellAt = (x: number, y: number): CodecId | null => {
    const el = document.elementFromPoint?.(x, y);
    const cell = el?.closest<HTMLElement>(".codec-cell[data-codec]");
    if (!cell || !grid?.contains(cell)) return null;
    return slides(cell).some((a) => a.playState === "running") ? null : (cell.dataset.codec as CodecId);
  };
  // The card dragged under the pointer, wherever its place in the grid is now
  const follow = (ev: PointerEvent) => {
    const el = drag && cellOf(drag.id);
    if (!drag || !el) return;
    const dx = ev.clientX - drag.x - (el.offsetLeft - drag.left);
    const dy = ev.clientY - drag.y - (el.offsetTop - drag.top);
    el.style.transform = `translate(${dx}px, ${dy}px)`;
  };
  const onPointerDown = (ev: PointerEvent) => {
    if (props.auto || busy() || ev.button !== 0) return;
    const target = ev.target as Element;
    const cell = target.closest<HTMLElement>(".codec-cell[data-codec]");
    if (!cell || target.closest(".codec-more")) return;
    drag = {
      id: cell.dataset.codec as CodecId,
      x: ev.clientX,
      y: ev.clientY,
      pointer: ev.pointerId,
      moving: false,
      left: cell.offsetLeft,
      top: cell.offsetTop,
    };
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
    const id = drag.id;
    const over = cellAt(ev.clientX, ev.clientY);
    if (over && over !== id) flip(() => setDragOrder(placed(order(), id, over)), id);
    follow(ev);
  };
  const endDrag = (ev: PointerEvent, cancelled: boolean) => {
    if (!drag || ev.pointerId !== drag.pointer) return;
    const d = drag;
    drag = null;
    if (!d.moving) return;
    // The click that ends a drag does not choose the card
    swallowClick = true;
    setTimeout(() => (swallowClick = false), 0);
    const el = cellOf(d.id);
    const from = el?.getBoundingClientRect();
    flip(() => {
      setDragging(null);
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
    }, d.id);
    // It settles from where it was let go into its place
    if (!el || !from) return;
    el.style.transform = "";
    if (still()) return;
    const to = el.getBoundingClientRect();
    slide(el, from.left - to.left, from.top - to.top);
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
            <Button
              icon="arrow_reset"
              disabled={busy() || ordering()}
              onClick={() => flip(() => void saveOrder([...CARD_ORDER]))}
            >
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
          caps={knownCodecCaps(snap().devices, e().address)}
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
