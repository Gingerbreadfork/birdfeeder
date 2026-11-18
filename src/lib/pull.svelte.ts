import type { Attachment } from "svelte/attachments";
import { tap } from "./platform";
import { watcher } from "./state.svelte";

/** How far the indicator must come down before letting go checks GitHub. */
export const READY = 72;
const FARTHEST = 112;
/** A finger moves this far before the gesture is read as a pull or a scroll. */
const SLOP = 6;

class Pull {
  /** How far the indicator has come down, in pixels. */
  distance = $state(0);
  /** Whether a finger is holding the indicator, so it follows without easing. */
  holding = $state(false);
  /** Whether a check started by a pull is running. */
  busy = $state(false);

  get ready(): boolean {
    return this.distance >= READY;
  }
}

export const pull = new Pull();

/** Drawing a scrolled-to-the-top list down past the indicator checks GitHub now. */
export const pullToRefresh: Attachment<HTMLElement> = (node) => {
  let from: { x: number; y: number } | null = null;
  let pulling = false;

  function down(e: TouchEvent) {
    from = null;
    pulling = false;
    if (pull.busy || e.touches.length !== 1 || node.scrollTop > 0) return;
    from = { x: e.touches[0].clientX, y: e.touches[0].clientY };
  }

  function move(e: TouchEvent) {
    if (!from) return;
    const dx = e.touches[0].clientX - from.x;
    const dy = e.touches[0].clientY - from.y;
    if (!pulling) {
      if (Math.abs(dx) < SLOP && Math.abs(dy) < SLOP) return;
      if (dy <= 0 || Math.abs(dx) > Math.abs(dy) || node.scrollTop > 0) {
        from = null;
        return;
      }
      pulling = true;
      pull.holding = true;
    }
    e.preventDefault();
    const wasReady = pull.ready;
    pull.distance = Math.min(FARTHEST, Math.max(0, dy) * 0.55);
    if (pull.ready && !wasReady) tap();
  }

  async function up() {
    const wasPulling = pulling;
    from = null;
    pulling = false;
    pull.holding = false;
    if (!wasPulling) return;
    if (!pull.ready) {
      pull.distance = 0;
      return;
    }
    pull.busy = true;
    pull.distance = READY;
    try {
      await watcher.checkNow();
    } finally {
      pull.busy = false;
      pull.distance = 0;
    }
  }

  function cancel() {
    from = null;
    pulling = false;
    pull.holding = false;
    if (!pull.busy) pull.distance = 0;
  }

  node.addEventListener("touchstart", down, { passive: true });
  node.addEventListener("touchmove", move, { passive: false });
  node.addEventListener("touchend", up);
  node.addEventListener("touchcancel", cancel);
  return () => {
    node.removeEventListener("touchstart", down);
    node.removeEventListener("touchmove", move);
    node.removeEventListener("touchend", up);
    node.removeEventListener("touchcancel", cancel);
  };
};
