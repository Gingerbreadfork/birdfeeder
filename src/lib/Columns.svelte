<script lang="ts">
  import { niceTicks, plural, short, shortDate } from "./format";

  export interface Column {
    date: string;
    value: number;
    /** An extra line for the tooltip, such as unique visitors. */
    note?: string;
  }

  let {
    data,
    unit,
    color = "var(--ink-3)",
    height = 132,
  }: {
    data: Column[];
    unit: [string, string];
    color?: string;
    height?: number;
  } = $props();

  const PAD = { top: 8, right: 4, bottom: 22, left: 34 };

  let width = $state(0);
  let hover = $state<number | null>(null);
  let chart = $state<HTMLElement>();

  const ticks = $derived(niceTicks(Math.max(1, ...data.map((d) => d.value)), 3));
  const top = $derived(ticks[ticks.length - 1]);
  const plotW = $derived(Math.max(0, width - PAD.left - PAD.right));
  const plotH = $derived(height - PAD.top - PAD.bottom);
  const band = $derived(data.length ? plotW / data.length : 0);
  const bar = $derived(Math.max(1, Math.min(24, band - 2)));
  const y = (v: number) => PAD.top + plotH - (v / top) * plotH;
  const labelEvery = $derived(Math.max(1, Math.ceil(data.length / Math.max(1, Math.floor(plotW / 64)))));

  /** A column with a rounded top and a square foot on the baseline. */
  function column(i: number, value: number): string {
    const left = PAD.left + i * band + (band - bar) / 2;
    const base = y(0);
    const h = Math.max(value > 0 ? 2 : 0, base - y(value));
    const r = Math.min(4, bar / 2, h);
    return `M${left},${base}V${base - h + r}q0,${-r} ${r},${-r}H${left + bar - r}q${r},0 ${r},${r}V${base}Z`;
  }

  const picked = $derived(hover === null ? null : data[hover]);

  /** Reads the column under the pointer, or under a finger sliding along the chart. */
  function track(e: PointerEvent) {
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const i = Math.floor((e.clientX - box.left - PAD.left) / band);
    hover = band > 0 && i >= 0 && i < data.length ? i : null;
  }

  /** A finger lifting leaves the reading in place, until a tap elsewhere. */
  function leave(e: PointerEvent) {
    if (e.pointerType !== "touch") hover = null;
  }

  function away(e: PointerEvent) {
    if (hover !== null && !chart?.contains(e.target as Node)) hover = null;
  }
</script>

<svelte:document onpointerdown={away} />

<div
  class="chart"
  bind:this={chart}
  bind:clientWidth={width}
  onpointerdown={track}
  onpointermove={track}
  onpointerleave={leave}
  role="group"
  aria-label="Daily {unit[1]}"
>
  {#if width > 0}
    <svg {width} {height} style:--series={color}>
      {#each ticks as tick (tick)}
        <line class="rule" x1={PAD.left} x2={PAD.left + plotW} y1={y(tick)} y2={y(tick)} />
        <text class="tick" x={PAD.left - 8} y={y(tick)} dy="0.32em" text-anchor="end">{short(tick)}</text>
      {/each}
      {#each data as d, i (d.date)}
        <path class={["bar", { lit: hover === i }]} d={column(i, d.value)} />
        {#if (data.length - 1 - i) % labelEvery === 0}
          {@const last = i === data.length - 1}
          <text
            class="tick"
            x={PAD.left + (last ? data.length : i + 0.5) * band}
            y={height - 6}
            text-anchor={last ? "end" : "middle"}
          >
            {shortDate(d.date)}
          </text>
        {/if}
      {/each}
    </svg>
    {#if picked && hover !== null}
      <div
        class="tip"
        style:left="{Math.min(Math.max(PAD.left + (hover + 0.5) * band, 70), width - 70)}px"
        style:top="{y(picked.value)}px"
      >
        <strong>{plural(picked.value, ...unit)}</strong>
        {#if picked.note}<span>{picked.note}</span>{/if}
        <span class="tip-title">{shortDate(picked.date)}</span>
      </div>
    {/if}
  {/if}
</div>

<style>
  .chart {
    position: relative;
    touch-action: pan-y;
  }

  svg {
    display: block;
  }

  .rule {
    stroke: var(--line);
    stroke-width: 1;
  }

  .tick {
    fill: var(--ink-3);
    font-size: 11px;
  }

  .bar {
    fill: var(--series);
  }

  .bar.lit {
    fill: color-mix(in srgb, var(--series) 78%, var(--ink));
  }


  .tip {
    display: flex;
    flex-direction: column;
    transform: translate(-50%, calc(-100% - 8px));
  }
</style>
