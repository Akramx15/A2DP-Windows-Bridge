// Picking the headphones: the saved devices and the ones a scan found
// (device.scan), or an address typed by hand; a saved device can be renamed
// or removed from the list (v1 OnDeviceEditName / OnDeviceDelete). Used by
// the profile editor and the first-run setup.

import { createMemo, createSignal, createUniqueId, For, Show } from "solid-js";
import { useApp } from "../app/context";
import { t } from "../i18n";
import type { Device } from "../ipc/types";
import { parseAddress } from "../lib/format";
import { validDeviceName } from "../lib/profile";
import { Button, Dialog, Field, IconButton } from "./controls";
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
  const uid = createUniqueId();

  // Rename: the device and the name being typed
  const [renaming, setRenaming] = createSignal<Device | null>(null);
  const [newName, setNewName] = createSignal("");
  const [renameTried, setRenameTried] = createSignal(false);
  const nameError = () => (renameTried() && !validDeviceName(newName()) ? t("ui.device.err_name") : null);
  const [forgetting, setForgetting] = createSignal<Device | null>(null);
  const [busy, setBusy] = createSignal(false);

  const rename = async () => {
    const d = renaming();
    const n = newName().trim();
    setRenameTried(true);
    if (!d || !validDeviceName(n) || busy()) return;
    setBusy(true);
    const ok = await app.run(() => app.core.request("device.rename", { address: d.address, name: n }));
    setBusy(false);
    if (!ok) return;
    setRenaming(null);
    // The profile shows the new name (v1 set the name field too)
    if (d.address === props.address) props.onPick(d.address, n);
  };
  const forget = async () => {
    const d = forgetting();
    if (!d || busy()) return;
    setBusy(true);
    await app.run(() => app.core.request("device.forget", { address: d.address }));
    setBusy(false);
    setForgetting(null);
  };

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
            {(d, i) => {
              const label = () => d.name || t("device.name_unknown");
              const nameId = () => `${uid}-${i()}`;
              return (
                // The radio covers the row (app.css); the buttons sit above it
                <div class="device-row" classList={{ on: d.address === props.address, dim: !d.audio }}>
                  <input
                    type="radio"
                    name={name}
                    value={d.address}
                    checked={d.address === props.address}
                    aria-labelledby={nameId()}
                    onChange={() => props.onPick(d.address, d.name)}
                  />
                  <Icon name={d.audio ? "headphones" : "bluetooth"} />
                  <span class="device-row-text" id={nameId()}>
                    <span class="device-row-name">{label()}</span> <span class="mono muted">{d.address}</span>
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
                  <Show when={d.saved}>
                    <span class="inline device-row-actions">
                      <IconButton
                        icon="edit"
                        label={`${t("device.edit_name")}: ${label()}`}
                        onClick={() => {
                          setNewName(d.name);
                          setRenameTried(false);
                          setRenaming(d);
                        }}
                      />
                      <IconButton
                        icon="delete"
                        label={`${t("ui.device.forget")}: ${label()}`}
                        onClick={() => setForgetting(d)}
                      />
                    </span>
                  </Show>
                </div>
              );
            }}
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
      <Dialog
        open={renaming() !== null}
        title={t("device.edit_name_title")}
        confirm={t("modal.save")}
        cancel={t("modal.cancel")}
        busy={busy()}
        onConfirm={() => void rename()}
        onCancel={() => setRenaming(null)}
      >
        <Field
          label={t("device.enter_name")}
          hint={renaming()?.address}
          error={nameError()}
        >
          {(id, described) => (
            <input
              id={id}
              class="input"
              value={newName()}
              maxLength={64}
              aria-describedby={described}
              aria-invalid={nameError() ? "true" : undefined}
              data-autofocus
              onInput={(e) => setNewName(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void rename();
                }
              }}
            />
          )}
        </Field>
      </Dialog>
      <Dialog
        open={forgetting() !== null}
        title={t("modal.confirm_title")}
        confirm={t("ui.device.forget")}
        cancel={t("modal.cancel")}
        danger
        busy={busy()}
        onConfirm={() => void forget()}
        onCancel={() => setForgetting(null)}
      >
        <p>{t("ui.device.forget_confirm")}</p>
        <p>
          <strong>{forgetting()?.name || t("device.name_unknown")}</strong>{" "}
          <span class="mono muted">{forgetting()?.address}</span>
        </p>
      </Dialog>
    </div>
  );
}
