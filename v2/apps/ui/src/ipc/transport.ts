// What the store needs from the host: the Tauri backend (tauri.ts) or the
// fake core used in a plain browser and in tests (mock.ts).

import type { ServerMessage } from "./types";

export type ConnStatus =
  | { state: "connecting" }
  | { state: "connected"; server: string }
  | { state: "core_missing"; detail: string }
  | { state: "rejected"; reason: string };

export type WindowKind = "main" | "flyout";

export interface Transport {
  readonly window: WindowKind;
  /** The window has the Mica backdrop: keep the page background translucent */
  readonly mica: boolean;
  /** The UI's own version */
  readonly version: string;
  /** Delivers every server message and status change from now on */
  start(onMessage: (m: ServerMessage) => void, onStatus: (s: ConnStatus) => void): Promise<void>;
  /** One request; rejects with an ErrorInfo-shaped value */
  call(method: string, params?: unknown): Promise<unknown>;

  showMain(route?: string): Promise<void>;
  hideWindow(): Promise<void>;
  openUrl(url: string): Promise<void>;
  openFolder(path: string): Promise<void>;
  revealPath(path: string): Promise<void>;
  setTheme(theme: string): Promise<void>;
  /** The backend asks the main window to show a screen */
  onNavigate(cb: (route: string) => void): void;
  /** The window was minimized */
  onMinimized(cb: () => void): void;
}
