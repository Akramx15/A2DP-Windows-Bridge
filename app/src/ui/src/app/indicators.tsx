// What the main window shows above or beside every screen: a newer
// release (the update check's result, not only on Settings), a crash of
// A2DPWB itself and a running HCI capture (v1 put " - [HCI capture
// running]" in the title bar).

import { createSignal, Show } from "solid-js";
import { Button, IconButton } from "../components/controls";
import { Icon } from "../components/Icon";
import { copyReport, openReport } from "../components/stream";
import { t } from "../i18n";
import type { CrashReport } from "../ipc/types";
import { useApp } from "./context";

/** "HCI capture running" from v1's title suffix (" - [HCI capture running]") */
export function captureRunningText(): string {
  return t("debug.capture_title_suffix")
    .replace(/^\s*-\s*/, "")
    .replace(/^\[(.*)\]$/, "$1")
    .trim();
}

/** A small red sign while an HCI capture runs; it leads to Diagnostics */
export function CaptureSign(props: { onOpen?: () => void }) {
  const { core, navigate } = useApp();
  const capture = () => core.state.snap?.capture ?? null;
  return (
    <Show when={capture()}>
      {(c) => (
        <button
          type="button"
          class="capture-sign"
          title={`${captureRunningText()}\n${c().path}`}
          aria-label={captureRunningText()}
          onClick={() => (props.onOpen ? props.onOpen() : navigate({ name: "diagnostics" }))}
        >
          <span class="dot rec" aria-hidden="true" />
          <span class="capture-sign-text ellipsis">{captureRunningText()}</span>
        </button>
      )}
    </Show>
  );
}

/** The head of a crash's report: what stopped, when, why */
export function crashHead(c: CrashReport): string {
  return `${t("ui.crash.title")}: ${c.process}, ${c.time}\n${c.reason}`;
}

/** A crash report of A2DPWB's own processes (a panic, a crash of the core
 *  or the window), until it is dismissed: its cause and backtrace go into
 *  the report with the diagnostic info */
export function CrashNotice() {
  const app = useApp();
  const crash = () => app.core.state.snap?.crashes?.[0] ?? null;
  const more = () => Math.max(0, (app.core.state.snap?.crashes?.length ?? 0) - 1);
  const dismiss = () => void app.core.request("diag.crashes_seen").catch(() => undefined);
  return (
    <Show when={crash()}>
      {(c) => (
        <div class="banner banner-error app-crash" role="alert">
          <Icon name="error_circle" class="banner-icon" />
          <div class="banner-body">
            <strong class="banner-title">{t("ui.crash.title")}</strong>
            <div class="banner-text">
              {t("ui.crash.when", c().process, c().time)}
              <Show when={more() > 0}> {t("ui.crash.more", more())}</Show>
            </div>
            <Show when={c().reason}>
              <div class="banner-text crash-reason ellipsis" title={c().reason}>
                {c().reason}
              </div>
            </Show>
            <div class="banner-actions">
              <Button kind="strong" icon="open" onClick={() => void openReport(app, crashHead(c()), c().text)}>
                {t("ui.diag.report_github")}
              </Button>
              <Button icon="copy" onClick={() => void copyReport(app, crashHead(c()), c().text)}>
                {t("ui.error.copy")}
              </Button>
            </div>
          </div>
          <IconButton icon="dismiss" label={t("modal.close")} onClick={dismiss} />
        </div>
      )}
    </Show>
  );
}

/** A newer release, on every screen but Settings (which has the details
 *  and the install button); closing it hides it for that version */
export function UpdateNotice() {
  const { core, route, navigate } = useApp();
  const [dismissed, setDismissed] = createSignal<string | null>(null);
  const available = () => {
    const u = core.state.snap?.update.available ?? null;
    if (!u || u.version === dismissed() || route().name === "settings") return null;
    return u;
  };
  return (
    <Show when={available()}>
      {(u) => (
        <div class="banner banner-info app-update" role="status">
          <Icon name="info" class="banner-icon" />
          <div class="banner-body">
            <strong class="banner-title">{t("update.available_title")}</strong>
            <div class="banner-text">
              {t("ui.settings.update_available", u().version, core.state.snap!.update.current_version)}
            </div>
            <div class="banner-actions">
              <Button kind="strong" icon="arrow_sync" onClick={() => navigate({ name: "settings" })}>
                {t("ui.update.show")}
              </Button>
              <Button icon="open" onClick={() => void core.transport.openUrl(u().url)}>
                {t("ui.settings.open_download")}
              </Button>
            </div>
          </div>
          <IconButton icon="dismiss" label={t("modal.close")} onClick={() => setDismissed(u().version)} />
        </div>
      )}
    </Show>
  );
}
