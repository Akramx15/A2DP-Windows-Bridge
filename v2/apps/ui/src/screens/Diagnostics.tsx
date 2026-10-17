// Diagnostics: live link graphs, the eleven-step connection flow, the log
// with level and text filters, HCI capture, "copy diagnostic info", and in
// developer mode the receiver test.

import { createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { errorText, useApp } from "../app/context";
import { LineChart } from "../components/charts";
import { Button, Check, Select } from "../components/controls";
import { Icon } from "../components/Icon";
import type { IconName } from "../components/icons";
import { t } from "../i18n";
import type { LogLevel, LogLine } from "../ipc/types";
import { STEPS, type StepStatus } from "../lib/flow";
import { clock, copyText, kbps, maskAddresses } from "../lib/format";
import { ReceiverTest } from "./ReceiverTest";

type Tab = "link" | "flow" | "log" | "receiver";

export function Diagnostics() {
  const app = useApp();
  const { core } = app;
  const [tab, setTab] = createSignal<Tab>("link");
  const dev = () => core.state.snap!.settings.developer_mode;
  const [capture, setCapture] = createSignal<string | null>(null);
  const [capBusy, setCapBusy] = createSignal(false);

  const toggleCapture = async () => {
    setCapBusy(true);
    try {
      if (capture()) {
        const path = capture()!;
        await core.request("diag.capture_stop");
        setCapture(null);
        app.notify(t("ui.diag.capture_saved", path));
      } else {
        setCapture(await core.request("diag.capture_start"));
      }
    } catch (e) {
      app.notify(t("debug.capture_failed", errorText(e)), "error");
    } finally {
      setCapBusy(false);
    }
  };

  const copyInfo = async () => {
    try {
      const info = await core.request("diag.info");
      app.notify((await copyText(info.text)) ? t("ui.diag.copied") : t("dbgc.copy_failed"), "info");
    } catch (e) {
      app.notify(errorText(e), "error");
    }
  };

  const tabs = (): { id: Tab; label: string; icon: IconName }[] => [
    { id: "link", label: t("quality.title"), icon: "pulse" },
    { id: "flow", label: t("dbgc.section_flow"), icon: "checkmark_circle" },
    { id: "log", label: t("dbgc.section_log"), icon: "people_list" },
    ...(dev() ? [{ id: "receiver" as Tab, label: t("receiver.title"), icon: "beaker" as IconName }] : []),
  ];

  const onTabKey = (e: KeyboardEvent) => {
    // Arrow keys move between tabs (WAI-ARIA tabs pattern)
    const list = tabs();
    const i = list.findIndex((x) => x.id === tab());
    let n = -1;
    if (e.key === "ArrowRight") n = (i + 1) % list.length;
    else if (e.key === "ArrowLeft") n = (i - 1 + list.length) % list.length;
    else if (e.key === "Home") n = 0;
    else if (e.key === "End") n = list.length - 1;
    if (n < 0) return;
    e.preventDefault();
    setTab(list[n].id);
    document.getElementById(`tab-${list[n].id}`)?.focus();
  };

  return (
    <div class="page diagnostics">
      <header class="page-head">
        <h1>{t("ui.nav.diagnostics")}</h1>
        <div class="head-actions">
          <Button icon={capture() ? "stop" : "record"} busy={capBusy()} onClick={toggleCapture} classList={{ recording: !!capture() }}>
            {capture() ? t("debug.capture_stop") : t("debug.capture_start")}
          </Button>
          <Button icon="copy" onClick={copyInfo}>
            {t("dbgc.copy_diag")}
          </Button>
        </div>
      </header>
      <Show when={capture()}>
        <p class="capture-line" role="status">
          <span class="dot rec" aria-hidden="true" />
          {t("ui.diag.capturing")} <span class="mono">{capture()}</span>
        </p>
      </Show>
      <div class="tabs" role="tablist" aria-label={t("ui.nav.diagnostics")} onKeyDown={onTabKey}>
        <For each={tabs()}>
          {(x) => (
            <button
              type="button"
              role="tab"
              id={`tab-${x.id}`}
              class="tab"
              aria-selected={tab() === x.id}
              aria-controls={`panel-${x.id}`}
              tabindex={tab() === x.id ? 0 : -1}
              onClick={() => setTab(x.id)}
            >
              <Icon name={x.icon} />
              {x.label}
            </button>
          )}
        </For>
      </div>
      <div class="tab-panel" role="tabpanel" id={`panel-${tab()}`} aria-labelledby={`tab-${tab()}`}>
        <Show when={tab() === "link"}>
          <LinkPanel />
        </Show>
        <Show when={tab() === "flow"}>
          <FlowPanel />
        </Show>
        <Show when={tab() === "log"}>
          <LogPanel />
        </Show>
        <Show when={tab() === "receiver" && dev()}>
          <ReceiverTest />
        </Show>
      </div>
    </div>
  );
}

function LinkPanel() {
  const { core } = useApp();
  onCleanup(core.wantLink());
  const samples = () => core.link();
  const streaming = () => core.state.snap!.stream.state === "streaming";
  const last = () => samples()[samples().length - 1];
  // Drops per sample (the counters run since the stream started)
  const drops = createMemo(() => {
    const s = samples();
    return s.map((x, i) => (i === 0 ? 0 : Math.max(0, x.dropped - s[i - 1].dropped + (x.capture_dropped - s[i - 1].capture_dropped))));
  });
  return (
    <div>
      <Show when={!streaming()}>
        <p class="muted">{t("quality.not_streaming")}</p>
      </Show>
      <div class="chart-grid">
        <LineChart
          title={t("ui.diag.rssi")}
          values={samples().map((s) => s.rssi)}
          min={-30}
          max={10}
          format={(v) => `${v > 0 ? "+" : ""}${Math.round(v)} dB`}
          status={(v) => (v >= -5 ? "good" : v >= -12 ? "warn" : "bad")}
        />
        <LineChart
          title={t("quality.bitrate")}
          values={samples().map((s) => kbps(s.bytes_per_second))}
          min={0}
          format={(v) => `${Math.round(v)} kbps`}
        />
        <LineChart
          title={t("quality.queue")}
          values={samples().map((s) => s.queue_depth)}
          min={0}
          format={(v) => `${Math.round(v)}`}
          status={(v) => (v < 8 ? "good" : v < 16 ? "warn" : "bad")}
        />
        <LineChart
          title={t("quality.dropped")}
          values={drops()}
          min={0}
          format={(v) => `${Math.round(v)}`}
          status={(v) => (v === 0 ? "good" : v < 3 ? "warn" : "bad")}
        />
      </div>
      <Show when={last()}>
        {(s) => (
          <dl class="stats">
            <div>
              <dt>{t("quality.airtime")}</dt>
              <dd class={`num ${s().airtime > 0.8 ? "st-warn" : ""}`}>{(s().airtime * 100).toFixed(1)} %</dd>
            </div>
            <div>
              <dt>{t("quality.packets")}</dt>
              <dd class="num">{s().packets_per_second.toFixed(1)} /s</dd>
            </div>
            <div>
              <dt>{t("quality.radio")}</dt>
              <dd class="num">
                {s().afh_in_use === null ? "–" : t("radio.afh", s().afh_in_use!)}
                {s().afh_avoided ? t("radio.afh_avoid", s().afh_avoided) : ""}
              </dd>
            </div>
            <div>
              <dt>{t("quality.dropped")}</dt>
              <dd class="num">{s().dropped}</dd>
            </div>
            <div>
              <dt>{t("quality.capture_dropped")}</dt>
              <dd class="num">{s().capture_dropped}</dd>
            </div>
          </dl>
        )}
      </Show>
      <p class="hint">{t("quality.band_help")}</p>
    </div>
  );
}

const STATUS_ICON: Record<StepStatus, IconName | null> = {
  pending: null,
  running: "arrow_sync",
  done: "checkmark_circle",
  warning: "warning",
  failed: "error_circle",
};

function FlowPanel() {
  const app = useApp();
  const session = () => app.core.flow();
  const statusText = (s: StepStatus) =>
    s === "done" ? t("dbgc.status.done") : s === "pending" ? t("dbgc.status.pending") : t(`dbgc.status.${s}`);
  const copyFlow = async () => {
    const s = session();
    if (!s) return;
    const lines = STEPS.map((st) => {
      const x = s.steps[st];
      const at = x.time_ms === null ? "" : `+${((x.time_ms - s.start_ms) / 1000).toFixed(3)} s`;
      return `${t(`dbgc.step.${st}`)}\t${statusText(x.status)}\t${at}\t${x.detail}`;
    });
    const ok = await copyText(maskAddresses(lines.join("\n")));
    app.notify(ok ? t("dbgc.copied", lines.length) : t("dbgc.copy_failed"), ok ? "info" : "error");
  };
  return (
    <div>
      <div class="panel-head">
        <span class="muted">
          {session() ? t("dbgc.session", session()!.id, clock(session()!.start_ms)) : t("dbgc.no_session")}
          {session()?.stop_ms != null ? t("dbgc.session_stopped", clock(session()!.stop_ms!)) : ""}
        </span>
        <Button icon="copy" kind="subtle" disabled={!session()} onClick={copyFlow}>
          {t("dbgc.copy_flow")}
        </Button>
      </div>
      <ol class="flow">
        <For each={STEPS}>
          {(st) => {
            const step = () => session()?.steps[st];
            const status = () => step()?.status ?? "pending";
            return (
              <li class={`flow-step st-${status()}`}>
                <span class="flow-icon" aria-hidden="true">
                  <Show when={STATUS_ICON[status()]} fallback={<span class="flow-dot" />}>
                    {(i) => <Icon name={i()} />}
                  </Show>
                </span>
                <span class="flow-name">{t(`dbgc.step.${st}`)}</span>
                <span class="flow-status">{statusText(status())}</span>
                <span class="flow-time num">
                  {step()?.time_ms != null && session() ? `+${((step()!.time_ms! - session()!.start_ms) / 1000).toFixed(3)} s` : ""}
                </span>
                <span class="flow-detail" title={step()?.detail}>
                  {st === "link" && session() && session()!.reconnects > 0
                    ? t("dbgc.link_reconnects", session()!.reconnects)
                    : step()?.detail}
                </span>
              </li>
            );
          }}
        </For>
      </ol>
    </div>
  );
}

const LEVEL_FILTERS: { value: LogLevel | "all"; label: string }[] = [
  { value: "all", label: "dbgc.level_all" },
  { value: "info", label: "dbgc.level_info" },
  { value: "warn", label: "dbgc.level_warn" },
  { value: "error", label: "dbgc.level_error" },
];
const ORDER: LogLevel[] = ["error", "warn", "info", "debug", "trace"];
/** Lines drawn at most (the filter and copy see all kept lines) */
const LOG_SHOWN = 1500;

function LogPanel() {
  const app = useApp();
  const { core } = app;
  const [level, setLevel] = createSignal<LogLevel | "all">("info");
  const [query, setQuery] = createSignal("");
  const [mask, setMask] = createSignal(true);
  const [follow, setFollow] = createSignal(true);
  // "All" shows debug lines: ask the core for them while this is open
  let release = core.wantLogs("debug");
  onCleanup(() => release());
  const setLevelAndSubscribe = (l: LogLevel | "all") => {
    setLevel(l);
    release();
    release = core.wantLogs(l === "all" ? "debug" : "info");
  };

  const filtered = createMemo(() => {
    const q = query().trim().toLowerCase();
    const max = level() === "all" ? ORDER.length : ORDER.indexOf(level() as LogLevel) + 1;
    return core.logs().filter(
      (l) => ORDER.indexOf(l.level) < max && (!q || l.message.toLowerCase().includes(q) || l.target.toLowerCase().includes(q)),
    );
  });
  const shown = createMemo(() => filtered().slice(-LOG_SHOWN));
  const text = (l: LogLine) => (mask() ? maskAddresses(l.message) : l.message);

  let box: HTMLDivElement | undefined;
  const scrollDown = () => {
    if (follow() && box) box.scrollTop = box.scrollHeight;
  };
  // Follow new lines
  createEffect(() => {
    shown();
    queueMicrotask(scrollDown);
  });

  const copy = async () => {
    const lines = filtered().map((l) => `${clock(l.time_ms)} ${l.level.toUpperCase().padEnd(5)} ${l.target}: ${text(l)}`);
    const ok = await copyText(lines.join("\n"));
    app.notify(ok ? t("dbgc.copied", lines.length) : t("dbgc.copy_failed"), ok ? "info" : "error");
  };

  return (
    <div class="log-panel">
      <div class="log-tools">
        <Select
          label={t("dbgc.col_level")}
          value={level()}
          options={LEVEL_FILTERS.map((f) => ({ value: f.value, label: t(f.label) }))}
          onChange={setLevelAndSubscribe}
        />
        <div class="search">
          <Icon name="search" />
          <input
            type="search"
            class="input"
            placeholder={t("dbgc.search_hint")}
            aria-label={t("dbgc.search_hint")}
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
        </div>
        <Check label={t("dbgc.mask")} title={t("dbgc.mask_tip")} checked={mask()} onChange={setMask} />
        <Check label={t("dbgc.autoscroll")} checked={follow()} onChange={setFollow} />
        <span class="muted num">{t("dbgc.count", filtered().length, core.logs().length)}</span>
        <Button icon="copy" kind="subtle" onClick={copy}>
          {t("dbgc.copy_log")}
        </Button>
        <Button icon="delete" kind="subtle" onClick={() => core.clearLogs()}>
          {t("ui.diag.clear")}
        </Button>
      </div>
      <div class="log" ref={box} role="log" aria-live="off" tabindex="0" aria-label={t("dbgc.section_log")}>
        <Show when={shown().length > 0} fallback={<p class="muted empty-line">{t("ui.diag.no_log")}</p>}>
          <For each={shown()}>
            {(l) => (
              <div class={`log-line lv-${l.level}`}>
                <span class="log-time num">{clock(l.time_ms)}</span>
                <span class="log-level">{l.level.toUpperCase()}</span>
                <span class="log-target">{l.target}</span>
                <span class="log-msg">{text(l)}</span>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
}

