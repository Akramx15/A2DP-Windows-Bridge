// Adapter: the USB Bluetooth adapters with their driver and firmware,
// WinUSB assign / restore, the chip for the firmware, and AFH.

import { createSignal, For, Show } from "solid-js";
import { useApp } from "../app/context";
import { saveSettings } from "../app/settings";
import { Banner, Button, Field, Section, Segmented, Select } from "../components/controls";
import { Icon } from "../components/Icon";
import { ErrorPanel, WinusbDialog } from "../components/stream";
import { t } from "../i18n";
import type { AdapterInfo, DriverKind } from "../ipc/types";
import { CHIP_AUTO, CHIP_CUSTOM, CHIP_NOT_REALTEK, REALTEK_CHIPS, stemFor } from "../lib/chips";
import { firmwareUrl, LINKS } from "../lib/links";

const hex4 = (n: number) => n.toString(16).toUpperCase().padStart(4, "0");

export function driverText(d: DriverKind): { text: string; status: "good" | "warn" | "bad" } {
  switch (d) {
    case "winusb":
      return { text: t("ui.adapter.driver_winusb"), status: "good" };
    case "bluetooth":
      return { text: t("ui.adapter.driver_bluetooth"), status: "warn" };
    case "other":
      return { text: t("ui.adapter.driver_other"), status: "warn" };
    default:
      return { text: t("ui.adapter.driver_none"), status: "bad" };
  }
}

export function Adapter() {
  const app = useApp();
  const { core } = app;
  const snap = () => core.state.snap!;
  const adapter = () => snap().adapter;
  const [dialog, setDialog] = createSignal<{ adapter: AdapterInfo; mode: "assign" | "restore" } | null>(null);
  const [listing, setListing] = createSignal(false);

  const refresh = async () => {
    setListing(true);
    await app.run(() => core.request("adapter.list"));
    setListing(false);
  };

  return (
    <div class="page">
      <header class="page-head">
        <h1>{t("ui.nav.adapter")}</h1>
        <div class="head-actions">
          <Button icon="arrow_sync" busy={listing()} onClick={refresh}>
            {t("ui.adapter.refresh")}
          </Button>
          <Button icon="arrow_reset" onClick={() => app.run(() => core.request("adapter.restart"))}>
            {t("ui.adapter.restart")}
          </Button>
        </div>
      </header>

      <p class={`status-line st-${adapter().status}`} role="status">
        <span class="dot" aria-hidden="true" />
        {t("ui.adapter.stack")}: {t(`ui.adapter.status_${adapter().status}`)}
      </p>
      <Show when={adapter().status === "error" && adapter().error}>{(m) => <ErrorPanel message={m()} />}</Show>

      <Show
        when={adapter().adapters.length > 0}
        fallback={
          <Banner kind="warn" title={t("firmware.no_adapters")}>
            {t("ui.adapter.none_hint")}
          </Banner>
        }
      >
        <For each={adapter().adapters}>
          {(a) => {
            const drv = () => driverText(a.driver);
            return (
              <section class="card adapter-card" aria-label={a.display_name}>
                <div class="adapter-head">
                  <Icon name="usb_stick" />
                  <div>
                    <h2>{a.display_name}</h2>
                    <span class="muted">
                      {a.device_name} · {a.vendor} · <span class="mono">{hex4(a.vid)}:{hex4(a.pid)}</span>
                    </span>
                  </div>
                </div>
                <dl class="kv">
                  <div>
                    <dt>{t("ui.adapter.driver")}</dt>
                    <dd>
                      <span class={`tag st-${drv().status}`}>{drv().text}</span>
                      <span class="muted mono"> {a.driver_service}</span>
                    </dd>
                  </div>
                  <div>
                    <dt>{t("ui.adapter.firmware")}</dt>
                    <dd>
                      <Show
                        when={a.firmware_present !== null}
                        fallback={<span class="muted">{t("ui.adapter.firmware_not_needed")}</span>}
                      >
                        <span class={`tag st-${a.firmware_present ? "good" : "bad"}`}>
                          {a.firmware_present ? t("firmware.present") : t("firmware.missing_short")}
                        </span>
                        <Show when={a.realtek_chip}>
                          <span class="muted"> {a.realtek_chip}</span>
                        </Show>
                      </Show>
                    </dd>
                  </div>
                </dl>
                <Show when={a.firmware_present === false}>
                  <Banner
                    kind="warn"
                    title={t("firmware.missing_manual")}
                    actions={
                      <>
                        <Button icon="open" onClick={() => core.transport.openUrl(firmwareUrl(a.vendor))}>
                          {t("firmware.open_download_page")}
                        </Button>
                        <Button icon="folder_open" onClick={() => core.transport.openFolder(snap().config_dir)}>
                          {t("firmware.open_folder")}
                        </Button>
                      </>
                    }
                  >
                    <p>{t("firmware.required_files")}</p>
                    <ul class="mono files">
                      <For each={a.firmware_files}>{(f) => <li>{f}</li>}</For>
                    </ul>
                    <p class="hint">{t("firmware.manual_instructions")}</p>
                  </Banner>
                </Show>
                <div class="adapter-actions">
                  <Show
                    when={a.driver === "winusb"}
                    fallback={
                      <>
                        <Button kind="strong" icon="shield" onClick={() => setDialog({ adapter: a, mode: "assign" })}>
                          {t("ui.adapter.assign_winusb")}
                        </Button>
                        <span class="hint">{t("ui.adapter.assign_short")}</span>
                      </>
                    }
                  >
                    <Button icon="arrow_reset" onClick={() => setDialog({ adapter: a, mode: "restore" })}>
                      {t("ui.adapter.restore_driver")}
                    </Button>
                    <span class="hint">{t("ui.adapter.restore_short")}</span>
                  </Show>
                </div>
              </section>
            );
          }}
        </For>
      </Show>

      <ChipSection />
      <AfhSection />

      <WinusbDialog adapter={dialog()?.adapter ?? null} mode={dialog()?.mode ?? "assign"} onClose={() => setDialog(null)} />
    </div>
  );
}

function ChipSection() {
  const app = useApp();
  const settings = () => app.core.state.snap!.settings;
  const detected = () => app.core.state.snap!.adapter.adapters.find((a) => a.realtek_chip)?.realtek_chip ?? null;
  const options = () => [
    { value: CHIP_AUTO, label: t("firmware.chip_auto", detected() ?? t("firmware.chip_auto_none")) },
    { value: CHIP_NOT_REALTEK, label: t("firmware.chip_non_realtek") },
    ...REALTEK_CHIPS.map((c) => ({ value: c.pid, label: `${c.name} (${c.stem}_fw.bin)` })),
    { value: CHIP_CUSTOM, label: "Custom (custom_fw.bin, custom_config.bin)" },
  ];
  // A value no entry stands for (a chip since dropped from the table) stays choosable
  const all = () => {
    const o = options();
    const pid = settings().bt_chip_pid;
    return o.some((x) => x.value === pid) ? o : [...o, { value: pid, label: `0x${hex4(pid)} ${settings().bt_chip_fw_stem}` }];
  };
  const hint = () =>
    settings().bt_chip_pid === CHIP_NOT_REALTEK
      ? t("firmware.non_realtek")
      : settings().bt_chip_pid === CHIP_AUTO
        ? detected()
          ? t("firmware.auto_selected")
          : t("firmware.auto_unrecognized")
        : t("firmware.select_chip");
  return (
    <Section title={t("firmware.title")}>
      <Field label={t("settings.bt_chip")} hint={hint()}>
        {(id, described) => (
          <Select
            id={id}
            describedBy={described}
            value={settings().bt_chip_pid}
            options={all()}
            onChange={(pid) => void saveSettings(app, { bt_chip_pid: pid, bt_chip_fw_stem: stemFor(pid) })}
          />
        )}
      </Field>
      <div class="inline">
        <Button icon="folder_open" kind="subtle" onClick={() => app.core.transport.openFolder(app.core.state.snap!.config_dir)}>
          {t("firmware.open_folder")}
        </Button>
        <Button icon="open" kind="subtle" onClick={() => app.core.transport.openUrl(LINKS.setupGuide)}>
          {t("ui.setup.guide")}
        </Button>
      </div>
    </Section>
  );
}

/** "6,11": Wi-Fi channels 1..14, at least one */
export function parseAfhChannels(s: string): string | null {
  const parts = s.split(/[\s,]+/).filter(Boolean);
  if (parts.length === 0) return null;
  const nums = parts.map(Number);
  if (nums.some((n) => !Number.isInteger(n) || n < 1 || n > 14)) return null;
  return [...new Set(nums)].sort((a, b) => a - b).join(",");
}

function AfhSection() {
  const app = useApp();
  const afh = () => app.core.state.snap!.settings.afh;
  const mode = () => (afh() === "auto" || afh() === "off" ? afh() : "manual");
  const [channels, setChannels] = createSignal(mode() === "manual" ? afh() : "");
  const [manual, setManual] = createSignal(mode() === "manual");
  const bad = () => manual() && channels().trim() !== "" && parseAfhChannels(channels()) === null;
  const saveChannels = () => {
    const c = parseAfhChannels(channels());
    if (c && c !== afh()) void saveSettings(app, { afh: c });
  };
  return (
    <Section title={t("receiver.afh")}>
      <p class="hint">{t("receiver.afh_tooltip")}</p>
      <Segmented
        label={t("receiver.afh")}
        value={manual() ? "manual" : mode()}
        options={[
          { value: "auto", label: t("receiver.afh_auto") },
          { value: "off", label: t("receiver.afh_off") },
          { value: "manual", label: t("receiver.afh_manual") },
        ]}
        onChange={(v) => {
          setManual(v === "manual");
          if (v === "manual") saveChannels();
          else void saveSettings(app, { afh: v });
        }}
      />
      <Show when={manual()}>
        <Field label={t("receiver.afh_channels")} hint={t("ui.adapter.afh_channels_hint")} error={bad() ? t("ui.adapter.afh_bad") : null}>
          {(id, described) => (
            <input
              id={id}
              class="input mono narrow"
              placeholder="6,11"
              value={channels()}
              aria-describedby={described}
              aria-invalid={bad() ? "true" : undefined}
              onInput={(e) => setChannels(e.currentTarget.value)}
              onBlur={saveChannels}
              onKeyDown={(e) => e.key === "Enter" && saveChannels()}
            />
          )}
        </Field>
      </Show>
    </Section>
  );
}
