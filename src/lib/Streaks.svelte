<script lang="ts">
  import Count from "./Count.svelte";
  import { plural, shortDate } from "./format";
  import type { Records, Streak } from "./types";

  let {
    streak,
    records,
    sightings,
  }: {
    streak: Streak | null;
    records: Records;
    /** How much of the life list is ticked, as seen and total. */
    sightings?: [number, number];
  } = $props();
</script>

{#if streak}
  <div class="streak">
    <p class="days"><Count value={streak.current} /></p>
    <div>
      <p class="lead">{streak.current === 1 ? "day" : "days"} of contributions in a row</p>
      <p class="muted">
        {streak.today > 0 ? `${plural(streak.today, "contribution")} today.` : "Nothing yet today."}
        {streak.best > streak.current
          ? `Your best is ${streak.best}.`
          : streak.current > 1
            ? "Your longest yet."
            : ""}
      </p>
    </div>
  </div>
{/if}

<dl>
  {#if records.best_day}
    <div>
      <dt>Best day</dt>
      <dd>{plural(records.best_day.count, "star")}, {shortDate(records.best_day.date)}</dd>
    </div>
  {/if}
  {#if records.best_week}
    <div>
      <dt>Best week</dt>
      <dd>{plural(records.best_week.count, "star")}, week of {shortDate(records.best_week.date)}</dd>
    </div>
  {/if}
  {#if records.best_star_streak > 1}
    <div>
      <dt>Days in a row with a star</dt>
      <dd>
        {records.star_streak >= records.best_star_streak
          ? `${records.star_streak}, your best`
          : `${records.star_streak || "None"} now (best ${records.best_star_streak})`}
      </dd>
    </div>
  {/if}
  {#if sightings}
    <div>
      <dt>Life list</dt>
      <dd>{sightings[0]} of {sightings[1]} seen</dd>
    </div>
  {/if}
</dl>

<style>
  .streak {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 16px;
  }

  .days {
    font-size: var(--fs-2xl);
    font-weight: 680;
    font-stretch: 116%;
    line-height: 1;
    letter-spacing: -0.02em;
  }

  .lead {
    font-weight: 600;
  }

  .muted {
    font-size: var(--fs-sm);
  }

  dl {
    margin: 0;
    font-size: var(--fs-row);
  }

  dl > div {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 0;
    border-top: 1px solid var(--line);
  }

  dt {
    color: var(--ink-2);
  }

  dd {
    margin: 0;
    font-weight: 600;
    text-align: right;
  }
</style>
