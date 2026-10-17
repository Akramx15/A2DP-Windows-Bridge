// Settings: language, theme, startup, updates, headphone buttons, volume,
// developer mode, the settings folder, about.

import { createSignal, Match, onMount, Show, Switch } from "solid-js";
import { useApp } from "../app/context";
import { saveSettings } from "../app/settings";
import { Banner, Button, Dialog, Field, Section, Segmented, Select, Toggle } from "../components/controls";
import { LANGUAGES, t } from "../i18n";
import type { UpdatePhase } from "../ipc/transport";
import { LINKS } from "../lib/links";

export function Settings() {
  const app = useApp();
  const { core } = app;
  const snap = () => core.state.snap!;
  const s = () => snap().settings;
  const set = (patch: Parameters<typeof saveSettings>[1]) => void saveSettings(app, patch);
  const [quitting, setQuitting] = createSignal(false);
  const update = () => snap().update;
  // The in-app update (installed copies); otherwise the release page
  const [phase, setPhase] = createSignal<UpdatePhase>({ state: "unavailable", reason: "portable" });
  onMount(() => {
    core.transport.onUpdatePhase(setPhase);
    void core.transport.updateStatus().then(setPhase, () => {});
  });
  const canInstall = () => phase().state === "idle" || phase().state === "failed";
  const installUpdate = (manifestUrl: string) => app.run(() => core.transport.updateInstall(manifestUrl));
  const mb = (bytes: number) => (bytes / 1048576).toFixed(1);

  const checkNow = async () => {
    await app.run(async () => {
      const u = await core.request("app.check_update");
      if (!u.available) app.notify(t("update.uptodate", u.current_version));
    });
  };

  return (
    <div class="page settings">
      <header class="page-head">
        <h1>{t("settings.header")}</h1>
      </header>

      <Section title={t("ui.settings.appearance")}>
        <Field label={t("settings.language")}>
          {(id) => (
            <Select
              id={id}
              value={LANGUAGES.some((l) => l.code === s().language) ? s().language : "en"}
              options={LANGUAGES.map((l) => ({ value: l.code, label: l.name }))}
              onChange={(v) => set({ language: v })}
            />
          )}
        </Field>
        <div class="field">
          <span class="field-label" id="theme-label">
            {t("settings.theme")}
          </span>
          <Segmented
            label={t("settings.theme")}
            value={s().theme}
            options={[
              { value: "system", label: t("settings.theme_system") },
              { value: "light", label: t("settings.theme_light") },
              { value: "dark", label: t("settings.theme_dark") },
            ]}
            onChange={(v) => set({ theme: v })}
          />
        </div>
      </Section>

      <Section title={t("ui.settings.startup")}>
        <Toggle label={t("settings.start_with_windows")} checked={s().start_with_windows} onChange={(v) => set({ start_with_windows: v })} />
        <Toggle
          label={t("ui.settings.start_minimized")}
          hint={t("ui.settings.start_minimized_hint")}
          checked={s().start_minimized}
          onChange={(v) => set({ start_minimized: v })}
        />
        <Toggle label={t("settings.minimize_to_tray")} checked={s().minimize_to_tray} onChange={(v) => set({ minimize_to_tray: v })} />
      </Section>

      <Section title={t("ui.settings.headphones")}>
        <Toggle label={t("settings.media_keys")} checked={s().media_keys} onChange={(v) => set({ media_keys: v })} />
        <Toggle label={t("settings.remember_volume")} checked={s().remember_volume} onChange={(v) => set({ remember_volume: v })} />
      </Section>

      <Section
        title={t("ui.settings.updates")}
        actions={
          <Button icon="arrow_sync" busy={update().checking} onClick={checkNow}>
            {t("update.check")}
          </Button>
        }
      >
        <Show when={update().available}>
          {(u) => (
            <Banner
              kind="info"
              title={t("update.available_title")}
              actions={
                <>
                  <Show when={canInstall()}>
                    <Button kind="strong" icon="arrow_sync" onClick={() => installUpdate(u().manifest_url)}>
                      {t("ui.update.install")}
                    </Button>
                  </Show>
                  <Button
                    kind={phase().state === "unavailable" ? "strong" : undefined}
                    icon="open"
                    onClick={() => core.transport.openUrl(u().url)}
                  >
                    {t("ui.settings.open_download")}
                  </Button>
                </>
              }
            >
              {t("ui.settings.update_available", u().version, update().current_version)}
              <Switch>
                <Match when={phase().state === "unavailable" && phase()}>
                  {(p) => {
                    const reason = (p() as Extract<UpdatePhase, { state: "unavailable" }>).reason;
                    return <div class="hint">{t(reason === "unsigned" ? "ui.update.unsigned" : "ui.update.portable")}</div>;
                  }}
                </Match>
                <Match when={phase().state === "downloading" && phase()}>
                  {(p) => {
                    const d = p() as Extract<UpdatePhase, { state: "downloading" }>;
                    const progress = d.total ? `${mb(d.downloaded)} / ${mb(d.total)} MB` : `${mb(d.downloaded)} MB`;
                    return <div class="hint">{t("ui.update.downloading", d.version || u().version, progress)}</div>;
                  }}
                </Match>
                <Match when={phase().state === "waiting" && phase()}>
                  {(p) => <div class="hint">{t("ui.update.waiting", (p() as { version: string }).version)}</div>}
                </Match>
                <Match when={phase().state === "installing" && phase()}>
                  {(p) => <div class="hint">{t("ui.update.installing", (p() as { version: string }).version)}</div>}
                </Match>
                <Match when={phase().state === "failed" && phase()}>
                  {(p) => <div class="hint">{t("ui.update.failed", (p() as { code: string }).code)}</div>}
                </Match>
              </Switch>
            </Banner>
          )}
        </Show>
        <Toggle
          label={t("settings.check_updates_on_startup")}
          checked={s().check_updates_on_startup}
          onChange={(v) => set({ check_updates_on_startup: v })}
        />
        <Toggle
          label={t("settings.include_prereleases")}
          checked={s().include_prereleases}
          onChange={(v) => set({ include_prereleases: v })}
        />
      </Section>

      <Section title={t("ui.settings.advanced")}>
        <Toggle
          label={t("ui.settings.developer_mode")}
          hint={t("ui.settings.developer_mode_hint")}
          checked={s().developer_mode}
          onChange={(v) => set({ developer_mode: v })}
        />
        <div class="row">
          <span class="row-text">
            <span class="row-label">{t("ui.settings.config_folder")}</span>
            <span class="hint mono">{snap().config_dir}</span>
          </span>
          <Button icon="folder_open" onClick={() => core.transport.openFolder(snap().config_dir)}>
            {t("firmware.open_folder")}
          </Button>
        </div>
        <div class="row">
          <span class="row-text">
            <span class="row-label">{t("ui.setup.title")}</span>
            <span class="hint">{t("ui.settings.run_setup_hint")}</span>
          </span>
          <Button icon="play" onClick={() => app.setSetupActive(true)}>
            {t("ui.settings.run_setup")}
          </Button>
        </div>
      </Section>

      <Section title={t("help.about_title")}>
        <div class="about">
          <img src="/app-icon.png" alt="" width="48" height="48" />
          <div>
            <strong>{t("help.full_name")}</strong>
            <p class="muted num">
              {t("help.version")} {core.transport.version} · core {update().current_version}
            </p>
            <p class="muted">{t("help.author")}</p>
          </div>
        </div>
        <p class="hint">{t("help.license")}</p>
        <p class="hint">{t("help.binary_license")}</p>
        <p class="hint">{t("ui.settings.icons_license")}</p>
        <div class="inline wrap">
          <Button icon="open" kind="subtle" onClick={() => core.transport.openUrl(LINKS.repo)}>
            {t("help.github")}
          </Button>
          <Button icon="open" kind="subtle" onClick={() => core.transport.openUrl(LINKS.thirdParty)}>
            {t("help.third_party_link")}
          </Button>
          <Button icon="open" kind="subtle" onClick={() => core.transport.openUrl(LINKS.issues)}>
            {t("menu.help.report_issue")}
          </Button>
          <Button icon="open" kind="subtle" onClick={() => core.transport.openUrl(LINKS.sponsor)}>
            {t("help.sponsor")}
          </Button>
        </div>
      </Section>

      <div class="danger-zone">
        <Button kind="danger" icon="power" onClick={() => setQuitting(true)}>
          {t("ui.settings.quit")}
        </Button>
        <span class="hint">{t("ui.settings.quit_hint")}</span>
      </div>
      <Dialog
        open={quitting()}
        title={t("ui.settings.quit")}
        confirm={t("ui.settings.quit")}
        cancel={t("modal.cancel")}
        danger
        onConfirm={() => {
          setQuitting(false);
          void app.run(() => core.request("app.quit"));
        }}
        onCancel={() => setQuitting(false)}
      >
        <p>{t("ui.settings.quit_confirm")}</p>
      </Dialog>
    </div>
  );
}
