<script lang="ts">
  import { addDays, dayHeading, localDay, monthName, parseDay, plural } from "./format";
  import { LANES, laneColor, type Lane } from "./kinds";
  import { touch } from "./platform";
  import { narrow, watcher } from "./state.svelte";
  import type { DayCount, Point } from "./types";
  import type { Attachment } from "svelte/attachments";

  let {
    days,
    contributions = [],
  }: {
    days: DayCount[];
    /** The owner's own contributions per day, drawn as a first row above the visitors. */
    contributions?: Point[];
  } = $props();

  const MAX_WEEKS = 13;
  /** Fewer weeks than this fit across, so the grid scrolls sideways instead of shrinking. */
  const FEWEST_WEEKS = 8;

  /** A finger gets bigger squares and a shorter column of row names. */
  const pitch = $derived(touch.current ? 18 : 14);
  const cell = $derived(pitch - 3);
  const label = $derived(touch.current ? 78 : 104);

  let width = $state(0);
  let grid = $state<HTMLElement>();

  const fits = $derived(Math.floor((width - label) / (pitch * 7)));
  /** Whether all the weeks are drawn and the grid scrolls to show them. */
  const scrolls = $derived(width > 0 && fits < FEWEST_WEEKS);
  const weeks = $derived(scrolls ? MAX_WEEKS : Math.max(2, Math.min(MAX_WEEKS, fits)));
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

  /** A scrolling grid opens on today, at its right-hand end, and stays there until scrolled. */
  const atToday: Attachment<HTMLElement> = (rows) => {
    const track = rows.parentElement!;
    let kept = true;
    const pin = () => {
      if (kept) track.scrollLeft = track.scrollWidth;
    };
    const scrolled = () => {
      kept = track.scrollLeft + track.clientWidth >= track.scrollWidth - 2;
    };
    const resized = new ResizeObserver(pin);
    resized.observe(rows);
    track.addEventListener("scroll", scrolled, { passive: true });
    return () => {
      resized.disconnect();
      track.removeEventListener("scroll", scrolled);
    };
  };

  function level(n: number): 0 | 1 | 2 | 3 {
    return n >= 5 ? 3 : n >= 2 ? 2 : n >= 1 ? 1 : 0;
  }

  interface Tip {
    x: number;
    y: number;
    title: string;
    value: string;
    /** The square a finger chose, which the tip offers to show in the activity list. */
    square?: { lane: Lane; date: string };
  }

  let tip = $state<Tip | null>(null);

  /** Places the tip above a square, kept inside the grid's own edges. */
  function above(target: HTMLElement): { x: number; y: number } {
    const outer = grid!.getBoundingClientRect();
    const box = target.getBoundingClientRect();
    const x = box.left - outer.left + box.width / 2;
    return { x: Math.min(Math.max(x, 76), outer.width - 76), y: box.top - outer.top };
  }

  function show(e: PointerEvent, lane: (typeof LANES)[number], date: string, n: number) {
    if (e.pointerType === "touch") return;
    tip = {
      ...above(e.currentTarget as HTMLElement),
      title: dayHeading(date, new Date(watcher.now)),
      value: n ? plural(n, ...lane.unit) : `No ${lane.unit[1]}`,
    };
  }

  function showOwn(e: PointerEvent, date: string, n: number) {
    tip = {
      ...above(e.currentTarget as HTMLElement),
      title: dayHeading(date, new Date(watcher.now)),
      value: n ? plural(n, "contribution") : "No contributions",
    };
  }

  const picked = (lane: Lane, date: string) => watcher.lane === lane && watcher.day === date;
  const peeked = (lane: Lane, date: string) =>
    tip?.square?.lane === lane && tip.square.date === date;

  /** Narrows the activity list to one square. */
  function showSquare(lane: Lane, date: string) {
    tip = null;
    watcher.setCell(lane, date);
    if (narrow.current) watcher.go("activity");
  }

  /** Narrows the list to one square, or widens it again when that square is already picked. */
  function pick(lane: Lane, date: string) {
    tip = null;
    if (picked(lane, date)) return watcher.setDay(null);
    showSquare(lane, date);
  }

  /** A finger first sees what a square holds, and shows it from the tip or with a second tap. */
  function choose(e: MouseEvent, lane: (typeof LANES)[number], date: string, n: number) {
    if (!touch.current) return pick(lane.id, date);
    if (peeked(lane.id, date)) return showSquare(lane.id, date);
    tip = {
      ...above(e.currentTarget as HTMLElement),
      title: dayHeading(date, new Date(watcher.now)),
      value: plural(n, ...lane.unit),
      square: { lane: lane.id, date },
    };
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
    grid?.querySelector<HTMLElement>(`[data-cell="${last}"]`)?.focus();
  }

  function pickLane(lane: Lane) {
    watcher.setLane(watcher.lane === lane ? null : lane);
    if (narrow.current) watcher.go("activity");
  }

  /** A tap anywhere outside the grid puts the tip away. */
  function away(e: PointerEvent) {
    if (tip && !grid?.contains(e.target as Node)) tip = null;
  }
</script>

<svelte:document onpointerdown={away} />

<div
  class={["grid", { scrolls }]}
  bind:this={grid}
  bind:clientWidth={width}
  onpointerleave={(e) => {
    if (e.pointerType !== "touch") tip = null;
  }}
  style:--pitch="{pitch}px"
  style:--square="{cell}px"
  style:--label="{label}px"
  role="group"
  aria-label="Activity by day"
>
  <div class="track" onscroll={() => (tip = null)}>
    <div class="rows" {@attach atToday}>
      {#if contributions.length}
        <div class="lane you" style:--kind="var(--ink-3)">
          <span class="name">You</span>
          {#each dates as date (date)}
            {@const n = own[date] ?? 0}
            <span
              class={["day", n > 0 && `l${level(n)}`]}
              role="presentation"
              onpointerenter={(e) => showOwn(e, date, n)}
              onpointerdown={(e) => e.pointerType === "touch" && showOwn(e, date, n)}
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
            {touch.current ? (lane.chip ?? lane.label) : lane.label}
          </button>
          {#each dates as date (date)}
            {@const n = counts[`${lane.id}|${date}`] ?? 0}
            {#if n > 0}
              <button
                class={["day", `l${level(n)}`, { picked: picked(lane.id, date) || peeked(lane.id, date) }]}
                aria-label="{plural(n, ...lane.unit)} on {dayHeading(date)}"
                aria-pressed={picked(lane.id, date)}
                data-cell="{lane.id}|{date}"
                tabindex={stop === `${lane.id}|${date}` ? 0 : -1}
                onclick={(e) => choose(e, lane, date, n)}
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
          <span style:left="{label + m.i * pitch}px">{monthName(m.date)}</span>
        {/each}
      </div>
    </div>
  </div>

  {#if tip}
    <div class={["tip", { act: tip.square }]} style:left="{tip.x}px" style:top="{tip.y}px">
      <strong>{tip.value}</strong>
      <span class="tip-title">{tip.title}</span>
      {#if tip.square}
        {@const { lane, date } = tip.square}
        <button onclick={() => showSquare(lane, date)}>Show in activity</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .grid {
    position: relative;
  }

  .track {
    overflow-x: clip;
  }

  .rows {
    position: relative;
    width: max-content;
    min-width: 100%;
    padding-bottom: 20px;
  }

  .scrolls .track {
    overflow-x: auto;
    overscroll-behavior-x: contain;
    scrollbar-width: none;
  }

  .scrolls .track::-webkit-scrollbar {
    display: none;
  }

  .lane {
    display: flex;
    align-items: center;
    width: max-content;
    height: var(--pitch);
  }

  /* The owner's row sits a little apart from the visitors it brought. */
  .you {
    margin-bottom: 5px;
  }

  .name {
    flex: none;
    width: var(--label);
    height: var(--pitch);
    padding: 0;
    border: 0;
    background: none;
    color: var(--ink-2);
    font-size: var(--fs-xs);
    line-height: var(--pitch);
    text-align: left;
    white-space: nowrap;
  }

  /* Row names stay put while the squares scroll beneath them. */
  .scrolls .name {
    position: sticky;
    left: 0;
    z-index: 1;
    background: var(--surface);
    box-shadow: 6px 0 6px -4px var(--surface);
  }

  .name[aria-pressed="true"] {
    color: var(--ink);
    font-weight: 650;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  @media (hover: hover) {
    button.name:hover {
      color: var(--ink);
      text-decoration: underline;
      text-underline-offset: 2px;
    }

    button.day:hover {
      outline: 2px solid var(--ink);
      outline-offset: 1px;
    }
  }

  .day {
    flex: none;
    width: var(--square);
    height: var(--square);
    margin-right: calc(var(--pitch) - var(--square));
    padding: 0;
    border: 0;
    border-radius: var(--cell-radius);
    background: var(--cell);
  }

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

  .tip.act {
    z-index: 3;
    pointer-events: auto;
    white-space: nowrap;
  }

  .tip button {
    margin: 6px -4px -3px;
    padding: 6px 4px 3px;
    border: 0;
    border-top: 1px solid color-mix(in srgb, var(--surface) 25%, transparent);
    background: none;
    color: var(--seed);
    font-weight: 650;
    text-align: left;
  }
</style>
