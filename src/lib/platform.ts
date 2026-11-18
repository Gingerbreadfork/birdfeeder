import { MediaQuery } from "svelte/reactivity";

/** A phone or tablet, where there is no tray, keyring, GitHub CLI or self-updater. */
export const mobile = /Android|iPhone|iPad/.test(navigator.userAgent);

/** Below this width the activity list becomes a tab instead of a side rail. */
export const narrow = new MediaQuery("max-width: 900px");

/** A finger rather than a mouse is doing the pointing. */
export const touch = new MediaQuery("pointer: coarse");

/** A phone held in the hand: tabs move to the bottom, where a thumb can reach them. */
export const handheld = new MediaQuery("(max-width: 900px) and (pointer: coarse)");

/** A short buzz to confirm a gesture landed. Not every device can. */
export function tap() {
  try {
    navigator.vibrate?.(8);
  } catch {
    // Vibration is a nicety.
  }
}
