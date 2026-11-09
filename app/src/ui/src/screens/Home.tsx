// Home: a card per saved audio device, side by side (issue #86), each with
// its connection ring, the connection in one line (state, battery, codec,
// link quality), the effect's shortcuts and the profile's own settings, and
// Connect while it does not stream. One device streams at a time: Connect on
// another one switches to it (the core stops the running stream first). The
// volume is on the card of the device in use, as each device keeps its own
// (issue #86); capture source (chosen right there) and link quality below,
// for the device in use.

import { createEffect, createSignal, For, on, Show } from "solid-js";
import { useApp, type AppContext } from "../app/context";
import { Banner, Button } from "../components/controls";
import { Icon } from "../components/Icon";
import { CapturePicker } from "../components/CapturePicker";
import { DeviceEffects } from "../components/DeviceEffects";
import {
  ConnectionLine,
  currentProfile,
  ErrorPanel,
  FirmwareDownload,
  LinkSparkline,
  openFirmwareFolder,
  QualityIndicator,
  Ring,
  VolumeControl,
} from "../components/stream";
import { t } from "../i18n";
import { IpcError, type AdapterInfo, type Snapshot, type StreamState } from "../ipc/types";
import { audioEntries, entryByKey, entryOf, registryFolder, type AudioEntry } from "../lib/profile";
import { familyLabel, spatialEngines } from "../lib/spatial";
import { chooseCodec, saveEntry } from "./Codecs";

/** Adapters whose firmware files are missing from the settings folder or
 *  invalid (v1's "Firmware not found!" / "Firmware file is invalid!" bar) */
export function adaptersMissingFirmware(snap: Snapshot): AdapterInfo[] {
  return snap.adapter.adapters.filter((a) => a.firmware_present === false);
}

/** The bar over Home while an adapter lacks its firmware files, or the
 *  running stack found its Realtek adapter on the ROM code (issue #81) */
function FirmwareMissing() {
  const app = useApp();
  const { core, navigate } = app;
  const missing = () => adaptersMissingFirmware(core.state.snap!);
  // One whose files the core can fetch (the first: one download at a time)
  const downloadable = () => missing().find((a) => a.firmware_download);
  const toFirmware = () => navigate({ name: "adapter", arg: "firmware" });
  return (
    <Show
      when={missing().length > 0}
      fallback={
        <Show when={core.state.snap!.adapter.rom_firmware}>
          <Banner
            kind="warn"
            title={t("ui.adapter.rom_firmware")}
            actions={
              <Button kind="strong" icon="usb_stick" onClick={toFirmware}>
                {t("ui.home.firmware_fix")}
              </Button>
            }
          >
            {t("firmware.rom_status")}
          </Banner>
        </Show>
      }
    >
      <Banner
        kind="warn"
        title={missing().some((a) => a.firmware_invalid) ? t("firmware.invalid_short") : t("firmware.missing_manual")}
        actions={
          // The Adapter screen's actions right here (issue #81): the
          // download, the folder for the files fetched by hand, the details
          <>
            <Show when={downloadable()}>{(a) => <FirmwareDownload files={a().firmware_files} kind="strong" />}</Show>
            <Button icon="folder_open" onClick={() => void openFirmwareFolder(app)}>
              {t("firmware.open_folder")}
            </Button>
            <Button kind={downloadable() ? undefined : "strong"} icon="usb_stick" onClick={toFirmware}>
              {t("ui.home.firmware_fix")}
            </Button>
          </>
        }
      >
        <For each={missing()}>
          {(a) => (
            <span>
              <strong>{a.display_name}</strong> ·{" "}
              {a.firmware_invalid ? (
                t("firmware.invalid", a.firmware_invalid)
              ) : (
                <>
                  {t("firmware.required_files")} <span class="mono">{a.firmware_files.join(", ")}</span>
                </>
              )}
            </span>
          )}
        </For>
        {/* Missing files and the ROM code at once: say it is not usable (issue #81) */}
        <Show when={core.state.snap!.adapter.rom_firmware}>
          <span>{t("firmware.rom_short")}</span>
        </Show>
      </Banner>
    </Show>
  );
}

/** v1 still runs (both cannot use the adapter): on Home and in the setup */
export function V1Running() {
  const { core } = useApp();
  return (
    <Show when={core.state.snap?.v1_running}>
      <Banner kind="warn" title={t("ui.v1_running.title")}>
        {t("ui.v1_running.text")}
      </Banner>
    </Show>
  );
}

/** The stream state a device's card shows: the stream's for the device of
 *  the profile streaming (or that last failed), idle for the others */
export function entryState(snap: Snapshot, entry: AudioEntry): StreamState {
  const s = snap.stream;
  if (s.state === "idle" || !s.profile) return "idle";
  return entryOf([entry], snap.profiles.find((p) => p.name === s.profile) ?? null) ? s.state : "idle";
}

/** Connect for a device: its profile (Auto made for it if it has none);
 *  another device streaming gives way (stream.start stops it first) */
export async function connectEntry(app: AppContext, entry: AudioEntry): Promise<void> {
  const name = entry.profile?.name ?? (await chooseCodec(app, entry, "auto"));
  if (name) await app.core.request("stream.start", { profile: name });
}

export function Home() {
  const { core, navigate } = useApp();
  const snap = () => core.state.snap!;
  const stream = () => snap().stream;
  const entries = () => audioEntries(snap());
  const profile = () => currentProfile(snap());
  // A failed request (Connect) until the state moves on
  const [reqError, setReqError] = createSignal<IpcError | null>(null);
  createEffect(on(() => stream().state, () => setReqError(null), { defer: true }));

  const error = () => {
    const e = reqError();
    if (e) return { code: e.code, message: e.msg };
    if (stream().state === "error") return { code: null, message: stream().message };
    const a = snap().adapter;
    if (a.status === "error" && a.error) return { code: null, message: a.error };
    return null;
  };

  return (
    <div class="page home">
      <header class="page-head">
        <h1>{t("ui.nav.home")}</h1>
      </header>

      <V1Running />
      <Show when={error()}>{(e) => <ErrorPanel code={e().code} message={e().message} />}</Show>
      <FirmwareMissing />

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
        <div class="home-devices">
          <For each={entries().map((e) => e.key)}>
            {(key) => {
              const entry = entryByKey(entries, key);
              return <DeviceCard entry={entry()} onError={(err) => setReqError(IpcError.from(err))} />;
            }}
          </For>
        </div>

        <Show when={profile()}>
          {(p) => (
            <div class="home-grid">
              <section class="card mini" aria-labelledby="home-capture">
                <h2 id="home-capture" class="mini-title">
                  {t("capture.mode")}
                </h2>
                <div class="mini-body">
                  <CapturePicker profile={p()} />
                </div>
                <button type="button" class="link" onClick={() => navigate({ name: "devices" })}>
                  {t("ui.home.capture_more")}
                </button>
              </section>
              <section class="card mini" aria-labelledby="home-link">
                <h2 id="home-link" class="mini-title">
                  {t("quality.title")}
                </h2>
                <div class="mini-body">
                  <QualityIndicator />
                </div>
                <LinkSparkline />
              </section>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
}

/** One device: the ring, its name and the connection in one line, the
 *  effect's shortcuts with the profile, and Connect while it does not
 *  stream (a streaming device is stopped from the tray) */
function DeviceCard(props: { entry: AudioEntry; onError: (e: unknown) => void }) {
  const app = useApp();
  const snap = () => app.core.state.snap!;
  const stream = () => snap().stream;
  const state = () => entryState(snap(), props.entry);
  const live = () => state() !== "idle";
  const running = () => state() === "streaming" || state() === "connecting" || state() === "reconnecting";
  const [busy, setBusy] = createSignal(false);
  const press = async () => {
    setBusy(true);
    try {
      await connectEntry(app, props.entry);
    } catch (e) {
      props.onError(e);
    } finally {
      setBusy(false);
    }
  };
  const name = () => props.entry.name || props.entry.address;
  // The effect the device's profile runs, by its family's name (empty: none,
  // or its engine is not in the folder any more); the stage manifest's own
  // long label stays on Codecs
  const engine = () => {
    const p = props.entry.profile;
    if (!p?.spatial_engine) return "";
    const list = spatialEngines(app.core, registryFolder(p)).engines() ?? [];
    return familyLabel(list.find((e) => e.path === p.spatial_engine_path) ?? { family: "" });
  };
  return (
    // The group names the device for its button ("Connect" on each card)
    <div class="home-device" classList={{ live: live() }} role="group" aria-label={name()}>
      <section class="card device-card">
        <Ring state={state()} volume={live() ? stream().volume : null}>
          <Icon name="headphones" />
        </Ring>
        <div class="device-info">
          <div class="device-name ellipsis" title={name()}>
            {name()}
          </div>
          {/* State, battery, codec and link quality on one line */}
          <ConnectionLine state={state()} stream={stream()} />
          <Show when={engine()}>
            <span class="tag">{engine()}</span>
          </Show>
        </div>
      </section>
      {/* Its own volume, while it is the one in use */}
      <Show when={live()}>
        <VolumeControl />
      </Show>
      <DeviceEffects entry={props.entry} save={(edit) => saveEntry(app, props.entry, edit)} />
      <Show when={!running()}>
        <button
          type="button"
          class="connect-btn"
          disabled={busy()}
          aria-busy={busy() ? "true" : undefined}
          onClick={press}
        >
          <Icon name="power" />
          <span>{t("connection.connect")}</span>
        </button>
      </Show>
    </div>
  );
}
