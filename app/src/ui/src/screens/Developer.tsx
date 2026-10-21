// Developer (shown while developer mode is on): the peer receiver test, and
// overrides of the codec and the media path for finding out why a sink stays
// silent (issue #87). The overrides are read when the next stream is set up.

import { createSignal, For, Show } from "solid-js";
import { useApp } from "../app/context";
import { saveSettings } from "../app/settings";
import { Banner, Button, Field, Section, Toggle } from "../components/controls";
import { Icon } from "../components/Icon";
import type { IconName } from "../components/icons";
import { t } from "../i18n";
import type { DevOverrides } from "../ipc/types";
import { ReceiverTest } from "./ReceiverTest";

type Tab = "receiver" | "overrides";

const NO_OVERRIDES: DevOverrides = {
  rtp_marker: false,
  packet_dump: false,
  aac_config_bitrate0: false,
  aac_bitrate_kbps: 0,
  aac_no_peak: false,
  aac_no_afterburner: false,
  aac_latm_v0: false,
};

export function Developer() {
  const app = useApp();
  const { core } = app;
  const [tab, setTab] = createSignal<Tab>("overrides");
  const settings = () => core.state.snap!.settings;
  const tabs = (): { id: Tab; label: string; icon: IconName }[] => [
    { id: "overrides", label: t("ui.dev.overrides"), icon: "wrench" },
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
    </div>
  );
}
