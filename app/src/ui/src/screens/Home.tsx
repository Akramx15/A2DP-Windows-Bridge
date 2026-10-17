// Home: the device, its connection ring and the biggest element on the
// screen, Connect / Disconnect; volume right below; codec, capture source
// and link quality small but always there.

import { createEffect, createSignal, For, on, Show } from "solid-js";
import { useApp } from "../app/context";
import { Banner, Button } from "../components/controls";
import { Icon } from "../components/Icon";
import type { IconName } from "../components/icons";
import {
  CaptureSource,
  CodecBadge,
  ConnectButton,
  currentProfile,
  ErrorPanel,
  LinkSparkline,
  QualityIndicator,
  Ring,
  VolumeControl,
} from "../components/stream";
import { ProfileSwitcher } from "../components/ProfileSwitcher";
import { t, tm } from "../i18n";
import { IpcError, type AdapterInfo, type Profile, type Snapshot } from "../ipc/types";

export function captureText(p: Profile): { icon: IconName; text: string } {
  switch (p.capture_mode) {
    case "virtual":
      return { icon: "desktop_speaker", text: p.audio_device_name || t("capture.virtual") };
    case "app":
      return { icon: "window_apps", text: p.app_name || p.app_exe || t("capture.app") };
    default:
      return { icon: "speaker_2", text: t("capture.loopback") };
  }
}

/** Adapters whose firmware files are missing from the settings folder
 *  (v1's "Firmware not found!" bar) */
export function adaptersMissingFirmware(snap: Snapshot): AdapterInfo[] {
  return snap.adapter.adapters.filter((a) => a.firmware_present === false);
}

/** The bar over Home while an adapter lacks its firmware files */
function FirmwareMissing() {
  const { core, navigate } = useApp();
  const missing = () => adaptersMissingFirmware(core.state.snap!);
  return (
    <Show when={missing().length > 0}>
      <Banner
        kind="warn"
        title={t("firmware.missing_manual")}
        actions={
          <Button kind="strong" icon="usb_stick" onClick={() => navigate({ name: "adapter" })}>
            {t("ui.home.firmware_fix")}
          </Button>
        }
      >
        <For each={missing()}>
          {(a) => (
            <span>
              <strong>{a.display_name}</strong> · {t("firmware.required_files")}{" "}
              <span class="mono">{a.firmware_files.join(", ")}</span>
            </span>
          )}
        </For>
      </Banner>
    </Show>
  );
}

export function Home() {
  const { core, navigate } = useApp();
  const snap = () => core.state.snap!;
  const stream = () => snap().stream;
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
        <Show when={snap().profiles.length > 0}>
          <ProfileSwitcher />
        </Show>
      </header>

      <Show when={error()}>{(e) => <ErrorPanel code={e().code} message={e().message} />}</Show>
      <FirmwareMissing />

      <Show
        when={profile()}
        fallback={
          <section class="card empty">
            <Icon name="headphones" class="empty-icon" />
            <p>{t("profile.empty")}</p>
            <Button kind="strong" icon="add" onClick={() => navigate({ name: "profiles", arg: "new" })}>
              {t("profile.new")}
            </Button>
          </section>
        }
      >
        {(p) => (
          <>
            <section class="card device-card" aria-label={t("ui.home.device")}>
              <Ring state={stream().state} volume={stream().volume}>
                <Icon name="headphones" />
              </Ring>
              <div class="device-info">
                <div class="device-name">{p().device_name || p().device_address}</div>
                <div class="muted device-sub">
                  <span>{p().name}</span>
                  <span class="mono">{p().device_address}</span>
                </div>
                <div class={`status-line st-${stream().state}`} role="status" aria-live="polite">
                  <span class="dot" aria-hidden="true" />
                  <span>
                    {/* An error's text is in the panel above, with its fix */}
                    {stream().state === "error" ? t("status.error") : tm(stream().message) || t(`status.${stream().state}`)}
                  </span>
                </div>
                <CodecBadge />
              </div>
            </section>

            <ConnectButton onError={(e) => setReqError(IpcError.from(e))} />
            <VolumeControl />

            <div class="home-grid">
              <section class="card mini" aria-labelledby="home-capture">
                <h2 id="home-capture" class="mini-title">
                  {t("capture.mode")}
                </h2>
                <div class="mini-body">
                  {/* While it streams: what the core captures, "waiting for
                      <app>" included; otherwise what the profile would */}
                  <Show
                    when={stream().info?.source}
                    fallback={
                      <>
                        <Icon name={captureText(p()).icon} />
                        <span class="ellipsis">{captureText(p()).text}</span>
                      </>
                    }
                  >
                    <CaptureSource profile={p()} />
                  </Show>
                </div>
                <button type="button" class="link" onClick={() => navigate({ name: "profiles", arg: p().name })}>
                  {t("profile.edit")}
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
          </>
        )}
      </Show>
    </div>
  );
}
