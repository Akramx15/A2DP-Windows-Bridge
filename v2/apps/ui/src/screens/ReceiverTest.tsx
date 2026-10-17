// The receiver test of v1 (Peer Receiver Test, app/src/wx_receiver_dialog.cpp):
// streams to tools/linux_sink on another PC and compares what was sent
// with what arrived. The core side is not there yet, so this is the form
// only: the receiver address is the real setting (settings.remote_sink);
// everything else is shown disabled and clearly marked.

import { createSignal, For } from "solid-js";
import { useApp } from "../app/context";
import { Banner, Button, Field, Section, Segmented, Select } from "../components/controls";
import { t } from "../i18n";
import { CODECS } from "../lib/codecs";

export function ReceiverTest() {
  const app = useApp();
  const settings = () => app.core.state.snap!.settings;
  const [sink, setSink] = createSignal(settings().remote_sink);
  const saveSink = () => {
    if (sink() === settings().remote_sink) return;
    void app.run(() => app.core.request("settings.set", { settings: { ...settings(), remote_sink: sink().trim() } }));
  };
  const rows = [
    "receiver.row_codec",
    "receiver.row_bitrate",
    "receiver.row_total",
    "receiver.row_loss",
    "receiver.row_timing",
    "receiver.row_buffer",
    "receiver.row_audio",
    "receiver.row_errors",
    "receiver.row_signal",
  ];
  return (
    <div class="receiver">
      <Banner kind="warn" title={t("ui.receiver.preview_title")}>
        {t("ui.receiver.preview_body")}
      </Banner>
      <p class="hint">{t("receiver.intro")}</p>
      <Section title={t("receiver.address")}>
        <Field label={t("receiver.address")} hint={t("receiver.address_tooltip")}>
          {(id, described) => (
            <input
              id={id}
              class="input mono"
              placeholder={t("receiver.address_hint")}
              value={sink()}
              aria-describedby={described}
              onInput={(e) => setSink(e.currentTarget.value)}
              onBlur={saveSink}
              onKeyDown={(e) => e.key === "Enter" && saveSink()}
            />
          )}
        </Field>
      </Section>
      <fieldset class="card section preview" disabled aria-describedby="receiver-preview-note">
        <legend class="sr-only">{t("receiver.title")}</legend>
        <p id="receiver-preview-note" class="muted">
          {t("receiver.off")}
        </p>
        <div class="advanced-grid">
          <Field label={t("receiver.codec")}>
            {(id) => (
              <Select id={id} value="sbc" disabled options={CODECS.filter((c) => c.id !== "auto").map((c) => ({ value: c.id, label: t(c.label) }))} onChange={() => {}} />
            )}
          </Field>
          <Field label={t("receiver.quality")}>
            {(id) => <Select id={id} value="hq" disabled options={[{ value: "hq", label: t("quality.high") }]} onChange={() => {}} />}
          </Field>
          <Field label={t("receiver.sample_rate")}>
            {(id) => <Select id={id} value={48000} disabled options={[{ value: 48000, label: "48000 Hz" }]} onChange={() => {}} />}
          </Field>
          <Field label={t("receiver.bit_depth")}>
            {(id) => <Select id={id} value={16} disabled options={[{ value: 16, label: t("bitdepth.16") }]} onChange={() => {}} />}
          </Field>
          <Field label={t("receiver.mtu")} hint={t("receiver.mtu_tooltip")}>
            {(id) => <Select id={id} value={0} disabled options={[{ value: 0, label: t("receiver.as_started") }]} onChange={() => {}} />}
          </Field>
          <Field label={t("receiver.buffer")}>
            {(id) => <Select id={id} value={0} disabled options={[{ value: 0, label: t("receiver.as_started") }]} onChange={() => {}} />}
          </Field>
        </div>
        <Segmented
          label={t("receiver.source")}
          value="tone"
          options={[
            { value: "tone", label: t("receiver.source_tone"), disabled: true },
            { value: "system", label: t("receiver.source_system"), disabled: true },
          ]}
          onChange={() => {}}
        />
        <div class="form-actions">
          <Button kind="strong" icon="play" disabled>
            {t("receiver.start")}
          </Button>
        </div>
        <table class="table">
          <thead>
            <tr>
              <th scope="col" />
              <th scope="col">{t("receiver.col_sent")}</th>
              <th scope="col">{t("receiver.col_received")}</th>
            </tr>
          </thead>
          <tbody>
            <For each={rows}>
              {(r) => (
                <tr>
                  <th scope="row">{t(r)}</th>
                  <td class="muted">–</td>
                  <td class="muted">–</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
        <p class="hint">{t("receiver.help")}</p>
      </fieldset>
    </div>
  );
}
