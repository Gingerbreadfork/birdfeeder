import { getCurrentWindow } from "@tauri-apps/api/window";
import { android, mobile } from "./platform";
import type { Theme } from "./types";

/** The theme last chosen, kept for the first frame of the next start. */
const REMEMBERED = "birdfeeder.theme";

/** Shows the page light, dark, or as the system is, and tells the window around it. */
export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
  try {
    localStorage.setItem(REMEMBERED, theme);
  } catch {
    // Only the first frame of the next start relies on it.
  }
  if (mobile) {
    android?.setTheme(theme);
  } else {
    getCurrentWindow()
      .setTheme(theme === "system" ? null : theme)
      .catch(() => {});
  }
}

/** Whether the page is showing its dark theme right now. */
export function showsDark(): boolean {
  const chosen = document.documentElement.dataset.theme;
  return chosen ? chosen === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
}

/** Applies the theme chosen last time before anything is drawn, so it does not flash. */
export function applyRememberedTheme() {
  try {
    const theme = localStorage.getItem(REMEMBERED);
    if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;
  } catch {
    // The settings apply it a moment later anyway.
  }
}
