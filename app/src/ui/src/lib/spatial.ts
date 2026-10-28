// The engines of a folder, listed once for the whole app: the Codecs screen
// shows the stages a device can run, Home labels the effect a profile has.
// An empty folder is the core's own engines folder (crates/core/src/spatial.rs
// default_registry), so both screens ask without knowing a path.

import { createResource, createRoot, type Resource } from "solid-js";
import type { SpatialEngine } from "../ipc/bindings/SpatialEngine";
import type { Core } from "../ipc/store";

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
