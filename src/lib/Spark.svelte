<script lang="ts">
  let { values }: { values: number[] } = $props();

  const W = 64;
  const H = 18;

  const path = $derived.by(() => {
    const max = Math.max(1, ...values);
    const step = W / Math.max(1, values.length - 1);
    return values
      .map((v, i) => `${i ? "L" : "M"}${(i * step).toFixed(1)},${(H - 2 - (v / max) * (H - 4)).toFixed(1)}`)
      .join("");
  });
</script>

<svg width={W} height={H} viewBox="0 0 {W} {H}" aria-hidden="true">
  <path d={path} />
</svg>

<style>
  svg {
    display: block;
  }

  path {
    fill: none;
    stroke: var(--ink-3);
    stroke-width: 1.5;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
</style>
