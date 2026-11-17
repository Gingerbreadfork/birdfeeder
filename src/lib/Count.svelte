<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import { Tween, prefersReducedMotion } from "svelte/motion";
  import { short } from "./format";

  let { value }: { value: number } = $props();

  /** Rolls to a new value when it changes, so a gain is seen arriving. */
  const shown = Tween.of(() => value, {
    duration: () => (prefersReducedMotion.current ? 0 : 700),
    easing: cubicOut,
  });
</script>

{short(Math.round(shown.current))}
