import { MediaQuery } from "svelte/reactivity";

/** A phone or tablet, where there is no tray, keyring, GitHub CLI or self-updater. */
export const mobile = /Android|iPhone|iPad/.test(navigator.userAgent);

/** Below this width the activity list becomes a tab instead of a side rail. */
export const narrow = new MediaQuery("max-width: 900px");

/** A finger rather than a mouse is doing the pointing. */
export const touch = new MediaQuery("pointer: coarse");

/** A phone held in the hand: tabs move to the bottom, where a thumb can reach them. */
export const handheld = new MediaQuery("(max-width: 900px) and (pointer: coarse)");

/** What the phone's activity offers the page. */
interface AndroidBridge {
  /** Colours the status bar and the window behind the page to match the theme. */
  setTheme(theme: string): void;
  /** Hands a PNG picture to the share sheet, with a line of text to go with it. */
  shareImage(name: string, base64: string, text: string): boolean;
}

export const android = (window as { BirdfeederAndroid?: AndroidBridge }).BirdfeederAndroid;

/** A short buzz to confirm a gesture landed. Not every device can. */
export function tap() {
  try {
    navigator.vibrate?.(8);
  } catch {
    // Vibration is a nicety.
  }
}
