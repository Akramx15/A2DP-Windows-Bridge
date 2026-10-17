// The core's state as the UI sees it: the last Snapshot with each Event
// applied (each event replaces one part), plus the streams the UI
// subscribes to (link samples, log lines). The UI keeps no state of its
// own beyond this and view state.
//
// Revisions: every state event carries the next revision; link samples
// and log lines are not state and carry none of their own. A gap means a
// lost event, so the snapshot is fetched again (state.snapshot). Its answer
// travels apart from the events and says nothing about which events it
// already contains, so the state events that arrive while it is on its way
// are kept and applied again on top of it, in order: each replaces a whole
// part, so the newest one of each part ends up in place.

import { batch, createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { FlowTracker, type FlowSession } from "../lib/flow";
import type { ConnStatus, Transport } from "./transport";
import {
  IpcError,
  type ArgsOf,
  type Event,
  type LinkSample,
  type LogLevel,
  type LogLine,
  type MethodName,
  type ResultOf,
  type ServerMessage,
  type Snapshot,
} from "./types";

/** Link samples kept: one minute at 4 per second */
export const LINK_KEEP = 240;
/** Log lines kept in the window (the core keeps its own files) */
export const LOG_KEEP = 5000;

const LEVELS: LogLevel[] = ["error", "warn", "info", "debug", "trace"];

export interface CoreState {
  conn: ConnStatus;
  /** Revision of the snapshot plus the events applied; null before the first */
  rev: number | null;
  snap: Snapshot | null;
}

export type Core = ReturnType<typeof createCore>;

export function createCore(transport: Transport) {
  const [state, setState] = createStore<CoreState>({ conn: { state: "connecting" }, rev: null, snap: null });

  // The streams change several times a second: plain arrays and a signal
  // that says "changed" are cheaper than store arrays
  const linkBuf: LinkSample[] = [];
  const [linkTick, setLinkTick] = createSignal(0);
  const logBuf: LogLine[] = [];
  const [logTick, setLogTick] = createSignal(0);
  const tracker = new FlowTracker();
  const [flowTick, setFlowTick] = createSignal(0);

  /** A state.snapshot on its way: the revision when asked, and the state events since */
  let pending: { from: number | null; events: { rev: number; ev: Event }[] } | null = null;

  async function request<M extends MethodName>(method: M, ...args: ArgsOf<M>): Promise<ResultOf<M>> {
    try {
      return (await transport.call(method, args[0])) as ResultOf<M>;
    } catch (e) {
      throw IpcError.from(e);
    }
  }

  function setSnapshot(snap: Snapshot, rev: number | null) {
    batch(() => {
      setState("snap", state.snap ? reconcile(snap) : snap);
      setState("rev", rev);
    });
  }

  function resync() {
    if (pending) return;
    // A window that opens late has no revision yet: the next event after
    // the answer then sets it, without a gap check
    const p = { from: state.rev, events: [] as { rev: number; ev: Event }[] };
    pending = p;
    request("state.snapshot")
      .then((snap) => {
        // A snapshot message of the core came first and replaced this
        if (pending !== p) return;
        pending = null;
        batch(() => {
          setSnapshot(snap, p.from);
          for (const e of p.events) {
            if (p.from === null || e.rev > p.from) {
              applyState(e.ev);
              if (e.rev > (state.rev ?? 0)) setState("rev", e.rev);
            }
          }
        });
      })
      .catch(() => {
        // Not connected: the next connection brings a snapshot anyway
        if (pending === p) pending = null;
      });
  }

  function applyData(ev: Event) {
    switch (ev.kind) {
      case "link":
        linkBuf.push(ev.value);
        if (linkBuf.length > LINK_KEEP) linkBuf.splice(0, linkBuf.length - LINK_KEEP);
        setLinkTick((n) => n + 1);
        return;
      case "log":
        logBuf.push(ev.value);
        if (logBuf.length > LOG_KEEP) logBuf.splice(0, logBuf.length - LOG_KEEP);
        setLogTick((n) => n + 1);
        if (tracker.feed(ev.value)) setFlowTick((n) => n + 1);
        return;
    }
  }

  function applyState(ev: Event) {
    // Before the first snapshot there is nothing to patch: it comes next
    if (!state.snap) return;
    switch (ev.kind) {
      case "adapter":
        setState("snap", "adapter", reconcile(ev.value));
        break;
      case "devices":
        setState("snap", "devices", reconcile(ev.value));
        break;
      case "scanning":
        setState("snap", "scanning", ev.value);
        break;
      case "stream":
        setState("snap", "stream", reconcile(ev.value));
        break;
      case "profiles":
        batch(() => {
          setState("snap", "profiles", reconcile(ev.value.profiles));
          setState("snap", "active_profile", ev.value.active_profile);
        });
        break;
      case "settings":
        setState("snap", "settings", reconcile(ev.value));
        break;
      case "update":
        setState("snap", "update", reconcile(ev.value));
        break;
      default:
        // An event kind of a newer core: the next snapshot has it
        break;
    }
  }

  function applyEvent(rev: number, ev: Event) {
    if (ev.kind === "link" || ev.kind === "log") {
      applyData(ev);
      return;
    }
    if (ev.kind === "bye") {
      setState("snap", null);
      return;
    }
    const cur = state.rev;
    if (cur !== null) {
      if (rev <= cur) return; // already in the snapshot
      if (rev > cur + 1) resync(); // lost one: the snapshot fixes it
    }
    if (pending) pending.events.push({ rev, ev });
    setState("rev", rev);
    applyState(ev);
  }

  function onMessage(m: ServerMessage) {
    switch (m.type) {
      case "snapshot":
        // The core's own snapshot (after Hello, or when this client fell
        // behind) has its true revision: a requested one is not needed
        pending = null;
        setSnapshot(m.state, m.rev);
        return;
      case "event":
        applyEvent(m.rev, m.event);
        return;
      default:
        // welcome / reject / response: the transport handles them
        return;
    }
  }

  function onStatus(s: ConnStatus) {
    const was = state.conn.state;
    setState("conn", reconcile(s));
    if (s.state === "connected" && was !== "connected") {
      // A window opened after Hello missed the snapshot; on a fresh
      // connection the core sends one too (harmless)
      lastSent = undefined;
      resync();
      pushSubscription();
    }
  }

  // ---- Subscriptions (diag.subscribe) -----------------------------------
  // Views ask for what they show while they are mounted; the window asks
  // for nothing while it is hidden. The backend merges both windows.

  const linkWants = new Set<symbol>();
  const logWants = new Map<symbol, LogLevel>();
  let lastSent: string | undefined;
  let pushQueued = false;

  function windowHidden() {
    return typeof document !== "undefined" && document.visibilityState === "hidden";
  }

  function pushSubscription() {
    if (pushQueued) return;
    pushQueued = true;
    // Mounting one view and unmounting another in the same tick sends once
    queueMicrotask(() => {
      pushQueued = false;
      const hidden = windowHidden();
      const link = !hidden && linkWants.size > 0;
      let logs: LogLevel | null = null;
      if (!hidden) {
        for (const l of logWants.values()) {
          if (logs === null || LEVELS.indexOf(l) > LEVELS.indexOf(logs)) logs = l;
        }
      }
      const key = JSON.stringify({ logs, link });
      if (key === lastSent || state.conn.state !== "connected") return;
      lastSent = key;
      request("diag.subscribe", { logs, link }).catch(() => {
        lastSent = undefined;
      });
    });
  }

  if (typeof document !== "undefined") {
    document.addEventListener("visibilitychange", pushSubscription);
  }

  /** Link samples while the caller lives; returns the release */
  function wantLink(): () => void {
    const k = Symbol("link");
    linkWants.add(k);
    pushSubscription();
    return () => {
      linkWants.delete(k);
      pushSubscription();
    };
  }

  /** Log lines at `level` and above while the caller lives */
  function wantLogs(level: LogLevel): () => void {
    const k = Symbol("logs");
    logWants.set(k, level);
    pushSubscription();
    return () => {
      logWants.delete(k);
      pushSubscription();
    };
  }

  void transport.start(onMessage, onStatus);

  return {
    state,
    transport,
    request,
    /** The last LINK_KEEP samples (reactive) */
    link: (): readonly LinkSample[] => {
      linkTick();
      return linkBuf;
    },
    /** The last LOG_KEEP lines (reactive) */
    logs: (): readonly LogLine[] => {
      logTick();
      return logBuf;
    },
    clearLogs() {
      logBuf.length = 0;
      setLogTick((n) => n + 1);
    },
    /** The latest connection attempt (reactive) */
    flow: (): FlowSession | null => {
      flowTick();
      return tracker.session;
    },
    wantLink,
    wantLogs,
  };
}
