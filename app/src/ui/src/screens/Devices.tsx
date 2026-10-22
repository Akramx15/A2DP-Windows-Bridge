// Audio Devices: the headphones and speakers A2DPWB knows, each with its own
// settings (capture source, Windows Volume Sync, buttons, remembered volume:
// issue #86), and a card to add one: scan for new ones or type an address.
// Profiles then pick from this list.

import { DevicePicker } from "../components/DevicePicker";
import { DeviceSettings } from "../components/DeviceSettings";
import { t } from "../i18n";

export function Devices() {
  return (
    <div class="page">
      <header class="page-head">
        <h1>{t("ui.nav.devices")}</h1>
      </header>
      <p class="muted">{t("ui.devices.page_hint")}</p>
      <DevicePicker manage address="" onPick={() => {}} settings={(d) => <DeviceSettings device={d} />} />
    </div>
  );
}
