// Audio Devices: the headphones and speakers A2DPWB knows. Scan for new
// ones, add one by its address, rename or remove one. Profiles then pick
// from this list.

import { DevicePicker } from "../components/DevicePicker";
import { t } from "../i18n";

export function Devices() {
  return (
    <div class="page">
      <header class="page-head">
        <h1>{t("ui.nav.devices")}</h1>
      </header>
      <p class="muted">{t("ui.devices.page_hint")}</p>
      <DevicePicker manage address="" onPick={() => {}} />
    </div>
  );
}
