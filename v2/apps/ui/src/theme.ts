// Dark / light / system. The colours are CSS variables (styles/app.css)
// keyed on <html data-theme>; "system" follows prefers-color-scheme live.
// The backend gets the choice too, for the title bar and Mica.

import type { Transport } from "./ipc/transport";

/** Applies `theme`; returns the cleanup (stops following the system) */
export function applyTheme(theme: string, transport: Transport): () => void {
  const root = document.documentElement;
  const media = typeof matchMedia === "function" ? matchMedia("(prefers-color-scheme: dark)") : null;
  const set = () => {
    const dark = theme === "dark" || (theme !== "light" && (media?.matches ?? true));
    root.dataset.theme = dark ? "dark" : "light";
  };
  set();
  void transport.setTheme(theme).catch(() => {});
  if (theme === "system" && media) {
    media.addEventListener("change", set);
    return () => media.removeEventListener("change", set);
  }
  return () => {};
}
