<script lang="ts">
  import { addDays, dayHeading, localDay, monthName, parseDay, plural } from "./format";
  import { LANES, laneColor, type Lane } from "./kinds";
  import { narrow, watcher } from "./state.svelte";
  import type { DayCount, Point } from "./types";

  let {
    days,
    contributions = [],
  }: {
    days: DayCount[];
    /** The owner's own contributions per day, drawn as a first row above the visitors. */
    contributions?: Point[];
  } = $props();

  const PITCH = 14;
  const LABEL = 104;
  const MAX_WEEKS = 13;

  let width = $state(0);
  let tip = $state<{ x: number; y: number; title: string; value: string } | null>(null);

  const weeks = $derived(
    Math.max(2, Math.min(MAX_WEEKS, Math.floor((width - LABEL) / (PITCH * 7)))),
  );
  const today = $derived(localDay(new Date(watcher.now)));
  const dates = $derived(
    Array.from({ length: weeks * 7 }, (_, i) => addDays(today, i - weeks * 7 + 1)),
  );

  /** Events per lane and day. Lost stars and unfollows are not activity to show. */
  const counts = $derived.by(() => {
    const laneOf = Object.fromEntries(LANES.map((lane) => [lane.kinds[0], lane.id]));
    const tally: Record<string, number> = {};
    for (const d of days) {
      const lane = laneOf[d.kind];
      if (lane) tally[`${lane}|${d.date}`] = (tally[`${lane}|${d.date}`] ?? 0) + d.count;
    }
    return tally;
  });

  const own = $derived(Object.fromEntries(contributions.map((p) => [p.date, p.value])));

  const months = $derived(
    dates
      .map((date, i) => ({ date, i }))
      .filter(({ date, i }) => parseDay(date).getDate() === 1 || i === 0)
      .filter(({ i }, at, all) => i !== 0 || (all[at + 1]?.i ?? Infinity) > 3),
  );

  function level(n: number): 0 | 1 | 2 | 3 {
    return n >= 5 ? 3 : n >= 2 ? 2 : n >= 1 ? 1 : 0;
  }

  function show(e: PointerEvent, lane: (typeof LANES)[number], date: string, n: number) {
    // The grid is the cell's offset parent, so these are already grid coordinates.
    const cell = e.currentTarget as HTMLElement;
    tip = {
      x: cell.offsetLeft + cell.offsetWidth / 2,
      y: cell.offsetTop,
      title: dayHeading(date, new Date(watcher.now)),
      value: n ? plural(n, ...lane.unit) : `No ${lane.unit[1]}`,
    };
  }

  function showOwn(e: PointerEvent, date: string, n: number) {
    const cell = e.currentTarget as HTMLElement;
    tip = {
      x: cell.offsetLeft + cell.offsetWidth / 2,
      y: cell.offsetTop,
      title: dayHeading(date, new Date(watcher.now)),
      value: n ? plural(n, "contribution") : "No contributions",
    };
  }

  const picked = (lane: Lane, date: string) => watcher.lane === lane && watcher.day === date;

  /** Narrows the list to one square, or widens it again when that square is already picked. */
  function pick(lane: Lane, date: string) {
    if (picked(lane, date)) return watcher.setDay(null);
    watcher.setCell(lane, date);
    if (narrow.current) watcher.go("activity");
  }

  /** The days with something in them, for each row. */
  const filled = $derived(
    LANES.map((lane) => dates.filter((date) => counts[`${lane.id}|${date}`] > 0)),
  );

  let last = $state<string | null>(null);

  /** The one square the Tab key lands on: the picked one, the last one used, or the newest. */
  const stop = $derived.by(() => {
    if (watcher.lane && watcher.day && counts[`${watcher.lane}|${watcher.day}`]) {
      return `${watcher.lane}|${watcher.day}`;
    }
    if (last && counts[last]) return last;
    let newest: string | null = null;
    filled.forEach((row, i) => {
      const date = row[row.length - 1];
      if (date && (!newest || date > newest.split("|")[1])) newest = `${LANES[i].id}|${date}`;
    });
    return newest;
  });

  /** Moves between filled squares with the arrow keys. */
  function move(e: KeyboardEvent, row: number, date: string) {
    const across = e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : 0;
    const down = e.key === "ArrowUp" ? -1 : e.key === "ArrowDown" ? 1 : 0;
    if (!across && !down) return;
    e.preventDefault();
    let to: string | undefined;
    if (across) {
      to = filled[row][filled[row].indexOf(date) + across];
    } else {
      const at = dates.indexOf(date);
      const gap = (d: string) => Math.abs(dates.indexOf(d) - at);
      for (row += down; row >= 0 && row < LANES.length; row += down) {
        to = filled[row].reduce<string | undefined>(
          (best, d) => (best === undefined || gap(d) < gap(best) ? d : best),
          undefined,
        );
        if (to) break;
      }
    }
    if (!to) return;
    last = `${LANES[row].id}|${to}`;
    const grid = (e.currentTarget as HTMLElement).closest(".grid");
    grid?.querySelector<HTMLElement>(`[data-cell="${last}"]`)?.focus();
  }

  function pickLane(lane: Lane) {
    watcher.setLane(watcher.lane === lane ? null : lane);
    if (narrow.current) watcher.go("activity");
  }
</script>

<div
  class="grid"
  bind:clientWidth={width}
  onpointerleave={() => (tip = null)}
  role="group"
  aria-label="Activity by day"
>
  {#if contributions.length}
    <div class="lane you" style:--kind="var(--ink-3)">
      <span class="name">You</span>
      {#each dates as date (date)}
        {@const n = own[date] ?? 0}
        <span
          class={["day", n > 0 && `l${level(n)}`]}
          role="presentation"
          onpointerenter={(e) => showOwn(e, date, n)}
        ></span>
      {/each}
    </div>
  {/if}
  {#each LANES as lane, row (lane.id)}
    <div class="lane" style:--kind={laneColor(lane.id)}>
      <button
        class="name"
        aria-pressed={watcher.lane === lane.id}
        onclick={() => pickLane(lane.id)}
        title="Show only {lane.label.toLowerCase()} in the activity list"
      >
        {lane.label}
      </button>
      {#each dates as date (date)}
        {@const n = counts[`${lane.id}|${date}`] ?? 0}
        {#if n > 0}
          <button
            class={["day", `l${level(n)}`, { picked: picked(lane.id, date) }]}
            aria-label="{plural(n, ...lane.unit)} on {dayHeading(date)}"
            aria-pressed={picked(lane.id, date)}
            data-cell="{lane.id}|{date}"
            tabindex={stop === `${lane.id}|${date}` ? 0 : -1}
            onclick={() => pick(lane.id, date)}
            onkeydown={(e) => move(e, row, date)}
            onfocus={() => (last = `${lane.id}|${date}`)}
            onpointerenter={(e) => show(e, lane, date, n)}
          ></button>
        {:else}
          <span class="day" role="presentation" onpointerenter={(e) => show(e, lane, date, 0)}></span>
        {/if}
      {/each}
    </div>
  {/each}

  <div class="months" aria-hidden="true">
    {#each months as m (m.date)}
      <span style:left="{LABEL + m.i * PITCH}px">{monthName(m.date)}</span>
    {/each}
  </div>

  {#if tip}
    <div class="tip" style:left="{tip.x}px" style:top="{tip.y}px">
      <strong>{tip.value}</strong>
      <span class="tip-title">{tip.title}</span>
    </div>
  {/if}
</div>

<style>
  .grid {
    position: relative;
    padding-bottom: 20px;
    overflow-x: clip;
  }

  .lane {
    display: flex;
    align-items: center;
    height: 14px;
  }

  /* The owner's row sits a little apart from the visitors it brought. */
  .you {
    margin-bottom: 5px;
  }

  .name {
    flex: none;
    width: 104px;
    height: 14px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--ink-2);
    font-size: var(--fs-xs);
    line-height: 14px;
    text-align: left;
    white-space: nowrap;
  }

  button.name:hover,
  .name[aria-pressed="true"] {
    color: var(--ink);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .name[aria-pressed="true"] {
    font-weight: 650;
  }

  .day {
    flex: none;
    width: 11px;
    height: 11px;
    margin-right: 3px;
    padding: 0;
    border: 0;
    border-radius: var(--cell-radius);
    background: var(--cell);
  }

  button.day:hover,
  .day.picked {
    outline: 2px solid var(--ink);
    outline-offset: 1px;
  }

  .l1 {
    background: color-mix(in srgb, var(--kind) 55%, var(--cell));
  }

  .l2 {
    background: color-mix(in srgb, var(--kind) 78%, var(--cell));
  }

  .l3 {
    background: var(--kind);
  }

  .months {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 16px;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }

  .months span {
    position: absolute;
  }

  .tip {
    display: flex;
    flex-direction: column;
    transform: translate(-50%, calc(-100% - 8px));
  }
</style>
