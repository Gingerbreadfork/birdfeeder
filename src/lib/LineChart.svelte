<script lang="ts">
  import { localDay, niceTicks, parseDay, plural, short, shortDate } from "./format";
  import type { Point } from "./types";

  let {
    points,
    unit,
    color = "var(--k-star)",
    height = 200,
  }: {
    /** Running totals, one point per day the total changed. */
    points: Point[];
    unit: [string, string];
    color?: string;
    height?: number;
  } = $props();

  const PAD = { top: 12, right: 52, bottom: 24, left: 40 };
  const DAY = 86_400_000;

  let width = $state(0);
  let hover = $state<number | null>(null);

  const series = $derived(points.map((p) => ({ t: parseDay(p.date).getTime(), value: p.value })));
  const span = $derived.by(() => {
    const last = series.at(-1)?.t ?? Date.now();
    const first = series[0]?.t ?? last;
    return { start: Math.min(first, last - DAY), end: last };
  });
  const ticks = $derived(niceTicks(Math.max(1, ...series.map((p) => p.value)), 5));
  const top = $derived(ticks[ticks.length - 1]);
  const plotW = $derived(Math.max(0, width - PAD.left - PAD.right));
  const plotH = $derived(height - PAD.top - PAD.bottom);

  const x = (t: number) => PAD.left + ((t - span.start) / (span.end - span.start)) * plotW;
  const y = (v: number) => PAD.top + plotH - (v / top) * plotH;

  /** A stepped line: the total holds until the day it changes. */
  const line = $derived.by(() => {
    if (!series.length) return "";
    let d = `M${x(span.start)},${y(series[0].t > span.start ? 0 : series[0].value)}`;
    for (const p of series) d += `H${x(p.t)}V${y(p.value)}`;
    return `${d}H${x(span.end)}`;
  });
  const area = $derived(line && `${line}V${y(0)}H${x(span.start)}Z`);

  const timeTicks = $derived.by(() => {
    const days = (span.end - span.start) / DAY;
    const out: { t: number; label: string }[] = [];
    const start = new Date(span.start);
    if (days > 800) {
      for (let year = start.getFullYear() + 1; ; year++) {
        const t = new Date(year, 0, 1).getTime();
        if (t > span.end) break;
        out.push({ t, label: String(year) });
      }
    } else if (days > 100) {
      const every = Math.ceil(days / 30 / 6);
      for (let month = start.getMonth() + 1; ; month += every) {
        const t = new Date(start.getFullYear(), month, 1).getTime();
        if (t > span.end) break;
        out.push({
          t,
          label: new Date(t).toLocaleDateString(undefined, {
            month: "short",
            year: month % 12 < every ? "numeric" : undefined,
          }),
        });
      }
    } else {
      const every = Math.max(1, Math.ceil(days / 6));
      for (let t = span.end; t >= span.start; t -= every * DAY) {
        out.unshift({ t, label: shortDate(localDay(new Date(t))) });
      }
    }
    return out.filter((tick) => x(tick.t) > PAD.left + 12 && x(tick.t) < PAD.left + plotW - 12);
  });

  const end = $derived(series.at(-1));
  const read = $derived.by(() => {
    if (hover === null) return null;
    const t = span.start + ((hover - PAD.left) / plotW) * (span.end - span.start);
    const value = series.findLast((p) => p.t <= t)?.value ?? 0;
    return { t, value };
  });

  function track(e: PointerEvent) {
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    hover = Math.min(PAD.left + plotW, Math.max(PAD.left, e.clientX - box.left));
  }
</script>

<div
  class="chart"
  bind:clientWidth={width}
  onpointermove={track}
  onpointerleave={() => (hover = null)}
  role="img"
  aria-label="{plural(end?.value ?? 0, ...unit)} as of today, charted over time"
>
  {#if width > 0 && end}
    <svg {width} {height} style:--series={color}>
      {#each ticks as tick (tick)}
        <line class="rule" x1={PAD.left} x2={PAD.left + plotW} y1={y(tick)} y2={y(tick)} />
        <text class="tick" x={PAD.left - 8} y={y(tick)} dy="0.32em" text-anchor="end">{short(tick)}</text>
      {/each}
      {#each timeTicks as tick (tick.t)}
        <text class="tick" x={x(tick.t)} y={height - 6} text-anchor="middle">{tick.label}</text>
      {/each}
      <path class="area" d={area} />
      <path class="line" d={line} />
      {#if read}
        <line class="cross" x1={hover} x2={hover} y1={PAD.top} y2={PAD.top + plotH} />
        <circle class="dot" cx={hover} cy={y(read.value)} r="4" />
      {:else}
        <circle class="dot" cx={x(end.t)} cy={y(end.value)} r="4" />
        <text class="end" x={x(end.t) + 9} y={y(end.value)} dy="0.32em">{short(end.value)}</text>
      {/if}
    </svg>
    {#if read && hover !== null}
      <div
        class="tip"
        style:left="{Math.min(Math.max(hover, 70), width - 70)}px"
        style:top="{y(read.value)}px"
      >
        <strong>{plural(read.value, ...unit)}</strong>
        <span class="tip-title">{shortDate(localDay(new Date(read.t)))}</span>
      </div>
    {/if}
  {/if}
</div>

<style>
  .chart {
    position: relative;
    min-height: 60px;
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

  .area {
    fill: var(--series);
    opacity: 0.12;
  }

  .line {
    fill: none;
    stroke: var(--series);
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  .dot {
    fill: var(--series);
    stroke: var(--surface);
    stroke-width: 2;
  }

  .cross {
    stroke: var(--line-strong);
    stroke-width: 1;
  }

  .end {
    fill: var(--ink);
    font-size: 12px;
    font-weight: 650;
  }

  .tip {
    display: flex;
    flex-direction: column;
    transform: translate(-50%, calc(-100% - 12px));
  }
</style>
