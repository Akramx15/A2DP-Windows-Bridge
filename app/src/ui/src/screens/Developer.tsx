// Developer (shown while developer mode is on): the peer receiver test,
// overrides of the codec and the media path for finding out why a sink stays
// silent (issue #87; read when the next stream is set up), and how the core
// logs: retention, the daily size cap, verbose logging that stays on, the
// stream health line every interval (issue #84) and marker lines.

import { createSignal, For, Show } from "solid-js";
import { useApp } from "../app/context";
import { saveSettings } from "../app/settings";
import { Banner, Button, Field, Section, Select, Toggle } from "../components/controls";
import { Icon } from "../components/Icon";
import type { IconName } from "../components/icons";
import { t } from "../i18n";
import type { DevLog, DevOverrides, LogLevel } from "../ipc/types";
import { ReceiverTest } from "./ReceiverTest";

type Tab = "receiver" | "overrides" | "logging";

const NO_OVERRIDES: DevOverrides = {
  rtp_marker: false,
  packet_dump: false,
  aac_config_bitrate0: false,
  aac_bitrate_kbps: 0,
  aac_no_peak: false,
  aac_no_afterburner: false,
  aac_latm_v0: false,
  aptxad_96k: false,
};

export function Developer() {
  const app = useApp();
  const { core } = app;
  const [tab, setTab] = createSignal<Tab>("overrides");
  const settings = () => core.state.snap!.settings;
  const tabs = (): { id: Tab; label: string; icon: IconName }[] => [
    { id: "overrides", label: t("ui.dev.overrides"), icon: "wrench" },
    { id: "logging", label: t("ui.dev.logging"), icon: "record" },
    { id: "receiver", label: t("receiver.title"), icon: "beaker" },
  ];

  const onTabKey = (e: KeyboardEvent) => {
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
    <div class="page developer">
      <header class="page-head">
        <h1>{t("ui.nav.developer")}</h1>
      </header>
      <div class="tabs" role="tablist" aria-label={t("ui.nav.developer")} onKeyDown={onTabKey}>
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
        <Show when={tab() === "overrides"}>
          <Overrides value={settings().dev_overrides} save={(dev_overrides) => saveSettings(app, { dev_overrides })} />
        </Show>
        <Show when={tab() === "logging"}>
          <Logging value={settings().dev_log} save={(dev_log) => saveSettings(app, { dev_log })} />
        </Show>
        <Show when={tab() === "receiver"}>
          <ReceiverTest />
        </Show>
      </div>
    </div>
  );
}

function Overrides(props: { value: DevOverrides; save: (v: DevOverrides) => Promise<boolean> }) {
  const set = (patch: Partial<DevOverrides>) => void props.save({ ...props.value, ...patch });
  const active = () => JSON.stringify(props.value) !== JSON.stringify(NO_OVERRIDES);
  // The bit rate commits when the field is left, so typing does not restart anything
  const commitBitrate = (text: string) => {
    const n = Math.round(Number(text));
    set({ aac_bitrate_kbps: Number.isFinite(n) && n > 0 ? Math.min(Math.max(n, 32), 512) : 0 });
  };
  return (
    <div>
      <Banner kind={active() ? "warn" : "info"} title={active() ? t("ui.dev.active") : t("ui.dev.intro")} />
      <Section
        title={t("ui.dev.section_media")}
        actions={
          <Button icon="arrow_reset" disabled={!active()} onClick={() => void props.save({ ...NO_OVERRIDES })}>
            {t("ui.dev.reset")}
          </Button>
        }
      >
        <Toggle
          label={t("ui.dev.rtp_marker")}
          hint={t("ui.dev.rtp_marker_hint")}
          checked={props.value.rtp_marker}
          onChange={(v) => set({ rtp_marker: v })}
        />
        <Toggle
          label={t("ui.dev.packet_dump")}
          hint={t("ui.dev.packet_dump_hint")}
          checked={props.value.packet_dump}
          onChange={(v) => set({ packet_dump: v })}
        />
      </Section>
      <Section title="AAC">
        <Toggle
          label={t("ui.dev.aac_latm_v0")}
          hint={t("ui.dev.aac_latm_v0_hint")}
          checked={props.value.aac_latm_v0}
          onChange={(v) => set({ aac_latm_v0: v })}
        />
        <Toggle
          label={t("ui.dev.aac_config_bitrate0")}
          hint={t("ui.dev.aac_config_bitrate0_hint")}
          checked={props.value.aac_config_bitrate0}
          onChange={(v) => set({ aac_config_bitrate0: v })}
        />
        <Field label={t("ui.dev.aac_bitrate")} hint={t("ui.dev.aac_bitrate_hint")}>
          {(id, describedBy) => (
            <input
              id={id}
              class="input"
              type="number"
              min="0"
              max="512"
              step="1"
              aria-describedby={describedBy}
              value={props.value.aac_bitrate_kbps || ""}
              placeholder={t("ui.dev.aac_bitrate_auto")}
              onChange={(e) => commitBitrate(e.currentTarget.value)}
            />
          )}
        </Field>
        <Toggle
          label={t("ui.dev.aac_no_peak")}
          checked={props.value.aac_no_peak}
          onChange={(v) => set({ aac_no_peak: v })}
        />
        <Toggle
          label={t("ui.dev.aac_no_afterburner")}
          checked={props.value.aac_no_afterburner}
          onChange={(v) => set({ aac_no_afterburner: v })}
        />
      </Section>
      {/* Off by default until 96 kHz works on real headphones (issue #86) */}
      <Section title="aptX Adaptive">
        <Toggle
          label={t("ui.dev.aptxad_96k")}
          hint={t("ui.dev.aptxad_96k_hint")}
          checked={props.value.aptxad_96k}
          onChange={(v) => set({ aptxad_96k: v })}
        />
      </Section>
    </div>
  );
}

const DEFAULT_LOG: DevLog = { keep_days: 7, day_cap_mb: 100, always_verbose: null, health_every_interval: false };

/** A whole number in min..=max from a field; the fallback when it is not one */
function clampInt(text: string, min: number, max: number, fallback: number): number {
  const n = Math.round(Number(text));
  return text.trim() !== "" && Number.isFinite(n) ? Math.min(Math.max(n, min), max) : fallback;
}

function Logging(props: { value: DevLog; save: (v: DevLog) => Promise<boolean> }) {
  const { core } = useApp();
  const set = (patch: Partial<DevLog>) => void props.save({ ...props.value, ...patch });
  const changed = () => JSON.stringify(props.value) !== JSON.stringify(DEFAULT_LOG);
  const logDir = () => core.state.snap!.log_dir;
  const [note, setNote] = createSignal("");
  const [marked, setMarked] = createSignal<number | null>(null);
  const mark = async () => {
    const n = await core.request("diag.mark", { note: note() });
    setMarked(n);
    setNote("");
  };
  return (
    <div>
      <Banner kind="info" title={t("ui.dev.log_intro")} />
      <Section
        title={t("ui.dev.log_files")}
        actions={
          <>
            <Button icon="folder_open" kind="subtle" disabled={!logDir()} onClick={() => void core.transport.openFolder(logDir())}>
              {t("ui.diag.open_logs")}
            </Button>
            <Button icon="arrow_reset" disabled={!changed()} onClick={() => void props.save({ ...DEFAULT_LOG })}>
              {t("ui.dev.log_reset")}
            </Button>
          </>
        }
      >
        <Field label={t("ui.dev.log_keep_days")} hint={t("ui.dev.log_keep_days_hint")}>
          {(id, describedBy) => (
            <input
              id={id}
              class="input"
              type="number"
              min="1"
              max="365"
              step="1"
              aria-describedby={describedBy}
              value={props.value.keep_days}
              onChange={(e) => {
                const v = clampInt(e.currentTarget.value, 1, 365, props.value.keep_days);
                e.currentTarget.value = String(v);
                set({ keep_days: v });
              }}
            />
          )}
        </Field>
        <Field label={t("ui.dev.log_day_cap")} hint={t("ui.dev.log_day_cap_hint")}>
          {(id, describedBy) => (
            <input
              id={id}
              class="input"
              type="number"
              min="10"
              max="10240"
              step="10"
              aria-describedby={describedBy}
              value={props.value.day_cap_mb}
              onChange={(e) => {
                const v = clampInt(e.currentTarget.value, 10, 10240, props.value.day_cap_mb);
                e.currentTarget.value = String(v);
                set({ day_cap_mb: v });
              }}
            />
          )}
        </Field>
      </Section>
      <Section title={t("ui.dev.log_detail")}>
        <Field label={t("ui.dev.log_always")} hint={t("ui.dev.log_always_hint")}>
          {(id, describedBy) => (
            <Select<"" | LogLevel>
              id={id}
              describedBy={describedBy}
              value={props.value.always_verbose ?? ""}
              options={[
                { value: "", label: t("ui.diag.verbose_off") },
                { value: "debug", label: t("ui.dev.log_always_debug") },
                { value: "trace", label: t("ui.dev.log_always_trace") },
              ]}
              onChange={(v) => set({ always_verbose: v === "" ? null : v })}
            />
          )}
        </Field>
        <Show when={props.value.always_verbose === "trace"}>
          <Banner kind="warn" title={t("ui.dev.log_trace_warn")} />
        </Show>
        <Toggle
          label={t("ui.dev.log_health")}
          hint={t("ui.dev.log_health_hint")}
          checked={props.value.health_every_interval}
          onChange={(v) => set({ health_every_interval: v })}
        />
      </Section>
      <Section title={t("ui.dev.mark")}>
        <Field label={t("ui.dev.mark_note")} hint={t("ui.dev.mark_hint")}>
          {(id, describedBy) => (
            <input
              id={id}
              class="input"
              type="text"
              maxLength={200}
              aria-describedby={describedBy}
              value={note()}
              placeholder={t("ui.dev.mark_placeholder")}
              onInput={(e) => setNote(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void mark();
                }
              }}
            />
          )}
        </Field>
        <div class="row">
          <Button icon="add" onClick={() => void mark()}>
            {t("ui.dev.mark_add")}
          </Button>
          <Show when={marked()}>
            {(n) => (
              <span class="muted" role="status">
                {t("ui.dev.mark_done", n())}
              </span>
            )}
          </Show>
        </div>
      </Section>
    </div>
  );
}
