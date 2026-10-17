import { unwrap } from "solid-js/store";
import type { Settings } from "../ipc/types";
import type { AppContext } from "./context";

/** Changes some settings: the core takes the whole object and sends it back as an event */
export function saveSettings(app: AppContext, patch: Partial<Settings>): Promise<boolean> {
  const current = unwrap(app.core.state.snap!.settings);
  return app.run(() => app.core.request("settings.set", { settings: { ...current, ...patch } }));
}
