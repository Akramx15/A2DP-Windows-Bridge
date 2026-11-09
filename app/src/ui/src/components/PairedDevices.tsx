// What Windows paired through an adapter about to go to WinUSB (issue #86:
// a built-in adapter takes the keyboard and mouse with it). The setup asks
// first when there are any; the Adapters screen's dialog names them. A list
// that cannot be read asks nothing.

import { Show } from "solid-js";
import type { AppContext } from "../app/context";
import { t } from "../i18n";
import type { AdapterInfo, WindowsPairedDevice } from "../ipc/types";

/** The devices Windows paired through the adapter; none when it is on
 *  WinUSB already or the list cannot be read */
export function windowsDevices(app: AppContext, a: AdapterInfo): Promise<WindowsPairedDevice[]> {
  if (a.driver === "winusb") return Promise.resolve([]);
  return app.core
    .request("adapter.windows_devices", { instance_id: a.instance_id })
    .then((list) => list ?? [])
    .catch(() => []);
}

/** Names shown before "and N more" */
export const NAMES_SHOWN = 3;

/** "Keyboard, Mouse, Headphones and 2 more": Windows' names (the address
 *  of one without), the first few */
export function pairedNames(list: readonly WindowsPairedDevice[], shown = NAMES_SHOWN): string {
  const names = list.slice(0, shown).map((d) => d.name || d.address);
  const rest = list.length - names.length;
  const joined = names.join(", ");
  return rest > 0 ? t("ui.adapter.paired_more", joined, rest) : joined;
}

/** The warning: which devices stop working with Windows, and what keeps
 *  them */
export function PairedDevicesNote(props: { devices: readonly WindowsPairedDevice[] }) {
  return (
    <Show when={props.devices.length > 0}>
      <div class="stack-tight">
        <p>{t("ui.adapter.paired_text")}</p>
        <p>
          <strong>{pairedNames(props.devices)}</strong>
        </p>
        <ul class="hint">
          <li>{t("ui.adapter.paired_receiver")}</li>
          <li>{t("ui.adapter.paired_keep")}</li>
        </ul>
      </div>
    </Show>
  );
}
