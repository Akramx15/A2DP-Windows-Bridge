// Picking the headphones: the saved devices and the ones a scan found
// (device.scan), or an address typed by hand. Used by the profile editor
// and the first-run setup.

import { createMemo, createSignal, For, Show } from "solid-js";
import { useApp } from "../app/context";
import { t } from "../i18n";
import type { Device } from "../ipc/types";
import { parseAddress } from "../lib/format";
import { Button, Field } from "./controls";
import { Icon } from "./Icon";

export const SCAN_SECONDS = 10;

export function sortDevices(devices: readonly Device[]): Device[] {
  // Saved first, then audio devices by signal; other kinds (keyboards...) last
  return [...devices].sort((a, b) => {
    if (a.audio !== b.audio) return a.audio ? -1 : 1;
    if (a.saved !== b.saved) return a.saved ? -1 : 1;
    return (b.rssi ?? -127) - (a.rssi ?? -127);
  });
}

export function DevicePicker(props: {
  address: string;
  onPick: (address: string, name: string) => void;
  error?: string | null;
}) {
  const app = useApp();
  const snap = () => app.core.state.snap!;
  const devices = createMemo(() => sortDevices(snap().devices));
  const [manual, setManual] = createSignal("");
  const scanning = () => snap().scanning;
  const scan = () => app.run(() => app.core.request("device.scan", { seconds: SCAN_SECONDS }));
  const name = "device-pick";

  return (
    <div class="device-picker">
      <div class="picker-head">
        <span class="muted" role="status" aria-live="polite">
          {scanning() ? t("connection.scanning_hint") : t("ui.devices.count", devices().length)}
        </span>
        <Button icon="search" busy={scanning()} onClick={scan}>
          {scanning() ? t("connection.scanning") : t("connection.scan")}
        </Button>
      </div>
      <div class="device-list" role="radiogroup" aria-label={t("connection.device_addr")}>
        <Show when={devices().length > 0} fallback={<p class="muted empty-line">{t("connection.no_devices")}</p>}>
          <For each={devices()}>
            {(d) => (
              <label class="device-row" classList={{ on: d.address === props.address, dim: !d.audio }}>
                <input
                  type="radio"
                  name={name}
                  value={d.address}
                  checked={d.address === props.address}
                  onChange={() => props.onPick(d.address, d.name)}
                />
                <Icon name={d.audio ? "headphones" : "bluetooth"} />
                <span class="device-row-text">
                  <span class="device-row-name">{d.name || t("device.name_unknown")}</span>
                  <span class="mono muted">{d.address}</span>
                </span>
                <span class="tags">
                  <Show when={d.saved}>
                    <span class="tag">{t("device.tag_saved")}</span>
                  </Show>
                  <Show when={d.audio}>
                    <span class="tag">{t("device.tag_audio")}</span>
                  </Show>
                  <Show when={d.rssi !== null}>
                    <span class="tag num">{d.rssi} dBm</span>
                  </Show>
                </span>
              </label>
            )}
          </For>
        </Show>
      </div>
      <Field
        label={t("connection.manual")}
        hint={t("ui.devices.manual_hint")}
        error={manual() && !parseAddress(manual()) ? t("error.invalid_address_format") : props.error}
      >
        {(id, described) => (
          <div class="inline">
            <input
              id={id}
              class="input mono"
              placeholder="AA:BB:CC:DD:EE:FF"
              value={manual()}
              aria-describedby={described}
              spellcheck={false}
              onInput={(e) => setManual(e.currentTarget.value)}
            />
            <Button
              disabled={!parseAddress(manual())}
              onClick={() => {
                const a = parseAddress(manual());
                if (a) props.onPick(a, "");
              }}
            >
              {t("ui.devices.use_address")}
            </Button>
          </div>
        )}
      </Field>
    </div>
  );
}
