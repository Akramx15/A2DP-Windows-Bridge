// The engines of a folder, listed once for the whole app: the Codecs screen
// shows the stages a device can run, Home labels the effect a profile has.
// An empty folder is the core's own engines folder (crates/core/src/spatial.rs
// default_registry), so both screens ask without knowing a path.

import { createResource, createRoot, type Resource } from "solid-js";
import { t } from "../i18n";
import type { SpatialEngine } from "../ipc/bindings/SpatialEngine";
import type { Core } from "../ipc/store";

/** The labels of the stage controls the app knows, by their config key */
const CONTROL_KEYS: Record<string, string> = {
  center_focus: "spatial.control_center_focus",
  ambience_spread: "spatial.control_ambience_spread",
  correction: "spatial.control_correction",
};

/** The labels of the choices the app knows, by their value */
const CHOICE_KEYS: Record<string, string> = {
  none: "spatial.correction_none",
  "sony-xm6-ba": "spatial.correction_sony_xm6_ba",
  autoeq: "spatial.correction_autoeq",
  "autoeq-wh1000xm6": "spatial.correction_autoeq",
};

/** A control's label: the app's own words for the ones it knows, the stage
 *  manifest's otherwise */
export function controlLabel(key: string, label: string): string {
  const known = CONTROL_KEYS[key];
  return known ? t(known) : label;
}

export function choiceLabel(value: string): string {
  const known = CHOICE_KEYS[value];
  return known ? t(known) : value;
}


/** The name a stage reads in the picker: the names the Linux app used for
 *  the same folders, the manifest's label otherwise */
const STAGE_NAMES: Record<string, string> = {
  "sony-native-reference": "Sony — المرجع القياسي",
  "personal-hpp-ba": "Sony XM6 BA",
  "personal-hpp-no-ba": "Sony HPP (بدون تصحيح)",
  "personal-hps": "Sony HPS",
  "experimental-stft": "Upmixer",
};

export function stageLabel(e: Pick<SpatialEngine, "path" | "label">): string {
  const parts = e.path.split(/[\/]/).filter(Boolean);
  const folder = parts.length > 1 ? parts[parts.length - 2] : "";
  return STAGE_NAMES[folder] ?? e.label;
}

/** The name a family reads on the cards ("Sony — Reference", "Upmixer"):
 *  the app's own words for it, not the stage manifest's long label */
export function familyLabel(e: Pick<SpatialEngine, "family">): string {
  return t(e.family === "upmixer" ? "profile.spatial_upmixer" : "profile.spatial_sony");
}

export interface EngineList {
  engines: Resource<SpatialEngine[]>;
  /** Asks the core again (a folder whose engines were installed since) */
  refetch(): void;
}

/** One list per core and folder: a second card showing the same folder does
 *  not ask the core again */
const cache = new WeakMap<Core, Map<string, EngineList>>();

export function spatialEngines(core: Core, folder: string): EngineList {
  let byFolder = cache.get(core);
  if (!byFolder) {
    byFolder = new Map();
    cache.set(core, byFolder);
  }
  const known = byFolder.get(folder);
  if (known) return known;
  // Its own root: the list outlives the card that asked for it first
  const list = createRoot(() => {
    const [engines, { refetch }] = createResource(folder, (path) => core.request("spatial.engines", { path }));
    return { engines, refetch: () => void refetch() };
  });
  byFolder.set(folder, list);
  return list;
}
