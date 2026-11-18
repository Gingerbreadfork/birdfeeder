<script lang="ts">
  import Bird from "./Bird.svelte";
  import { clock, dayHeading, localDay, plural, shortDate } from "./format";
  import { LANES, describe, eventColor, hasBirdMarker, laneColor, type Part } from "./kinds";
  import { pullToRefresh } from "./pull.svelte";
  import { watcher } from "./state.svelte";
  import type { WatchEvent } from "./types";
  import type { Attachment } from "svelte/attachments";

  const groups = $derived.by(() => {
    const out: { day: string; events: WatchEvent[] }[] = [];
    for (const event of watcher.events) {
      const day = localDay(event.occurred_at);
      const last = out[out.length - 1];
      if (last?.day === day) last.events.push(event);
      else out.push({ day, events: [event] });
    }
    return out;
  });

  const unread = $derived(watcher.status.unread);
  const firstCheck = $derived(!watcher.status.last_sync_at && watcher.status.phase === "syncing");

  /** Loads older rows as the end of the list comes into view. */
  const more: Attachment<HTMLElement> = (node) => {
    const seen = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) watcher.showMore();
      },
      { root: node.closest(".scroll"), rootMargin: "0px 0px 240px 0px" },
    );
    seen.observe(node);
    return () => seen.disconnect();
  };

  /** Asks GitHub for a small copy of an avatar. Inline images are used as they are. */
  function avatar(url: string): string {
    return url.startsWith("https://") ? `${url}${url.includes("?") ? "&" : "?"}s=40` : url;
  }
</script>

{#snippet text(parts: Part[])}
  {#each parts as part, i (i)}
    {#if typeof part === "string"}{part}{:else if "strong" in part}<strong>{part.strong}</strong
      >{:else if "tag" in part}<span class="tag">{part.tag}</span>{:else}<span class="mono"
        >{part.code}</span
      >{/if}
  {/each}
{/snippet}

<section class="feed">
  <div class="head">
    <h2>Activity</h2>
    {#if watcher.filtered}
      {#if watcher.unreadShown > 0}
        <span class="unread">{watcher.unreadShown}{watcher.hasMore ? "+" : ""} unread here</span>
        <button class="btn quiet" onclick={() => watcher.markAllRead()}>Mark these read</button>
      {/if}
    {:else if unread > 0}
      <span class="unread">{plural(unread, "unread", "unread")}</span>
      <button class="btn quiet" onclick={() => watcher.markAllRead()}>Mark all read</button>
    {/if}
  </div>

  <div class="filters" role="group" aria-label="Filter activity">
    {#each LANES as lane (lane.id)}
      <button
        class="chip"
        aria-pressed={watcher.lane === lane.id}
        onclick={() => watcher.setLane(watcher.lane === lane.id ? null : lane.id)}
      >
        <span class="cell" style:--kind={laneColor(lane.id)}></span>{lane.chip ?? lane.label}
      </button>
    {/each}
    <label class="only">
      <input
        type="checkbox"
        checked={watcher.unreadOnly}
        onchange={(e) => watcher.setUnreadOnly(e.currentTarget.checked)}
      />
      Unread only
    </label>
    {#if watcher.day}
      <button class="chip on" onclick={() => watcher.setDay(null)} title="Show every day">
        {shortDate(watcher.day)} <span aria-hidden="true">×</span>
      </button>
    {/if}
    {#if watcher.repoFilter}
      <button class="chip on" onclick={() => watcher.setRepoFilter(null)} title="Show every repository">
        {watcher.repoFilter.name} <span aria-hidden="true">×</span>
      </button>
    {/if}
  </div>

  <div class="list scroll" {@attach pullToRefresh}>
    {#each groups as group (group.day)}
      {@const fresh = group.events.filter((e) => !e.read).map((e) => e.id)}
      <h3>
        {dayHeading(group.day, new Date(watcher.now))}
        {#if fresh.length}
          <button class="btn quiet" onclick={() => watcher.markRead(fresh)}>Mark read</button>
        {/if}
      </h3>
      <ul>
        {#each group.events as event (event.id)}
          {@const d = describe(event, watcher.login)}
          <li>
            <button class={["row", { read: event.read }]} onclick={() => watcher.openEvent(event)}>
              {#if hasBirdMarker(event)}
                <span class="bird" title={event.read ? undefined : "Unread"}>
                  <Bird size={15} perch={false} />
                </span>
              {:else}
                <span
                  class={["cell", { hollow: event.read }]}
                  style:--kind={eventColor(event)}
                  title={event.read ? undefined : "Unread"}
                ></span>
              {/if}
              <span class="body">
                <span class="lead">
                  {#if d.byActor && event.actor_avatar}
                    <img src={avatar(event.actor_avatar)} alt="" width="16" height="16" loading="lazy" />
                  {/if}
                  {@render text(d.parts)}
                </span>
                {#if d.note}<span class="note">{d.note}</span>{/if}
                {#if d.detail}<span class="detail">{@render text(d.detail)}</span>{/if}
                {#if d.quote}<span class="quote">{d.quote}</span>{/if}
              </span>
              <time datetime={event.occurred_at}>{clock(event.occurred_at)}</time>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      {#if watcher.eventsLoaded}
        <div class="empty">
          {#if watcher.filtered}
            <p>Nothing matches these filters.</p>
            <button class="btn" onclick={() => watcher.clearFilters()}>Clear filters</button>
          {:else if firstCheck}
            <p>Recent stars, issues and releases will show up here in a moment.</p>
          {:else}
            <Bird size={64} />
            <p>Quiet at the feeder. New stars, issues, downloads and followers will land here.</p>
          {/if}
        </div>
      {/if}
    {/each}
    {#if watcher.hasMore}
      <button class="btn more" onclick={() => watcher.showMore()} {@attach more}>
        Show older activity
      </button>
    {/if}
  </div>
</section>

<style>
  .feed {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 30px;
    padding: 0 10px 0 16px;
  }

  .unread {
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }

  .head .btn {
    margin-left: auto;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    padding: 8px 12px 10px;
    border-bottom: 1px solid var(--line);
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: 12px;
    background: none;
    color: var(--ink-2);
    font-size: var(--fs-sm);
    white-space: nowrap;
  }

  @media (hover: hover) {
    .chip:hover {
      background: var(--accent-wash);
      color: var(--ink);
    }

    .row:hover {
      background: var(--accent-wash);
    }
  }

  .chip:active,
  .row:active {
    background: var(--accent-wash);
  }

  .chip[aria-pressed="true"],
  .chip.on {
    border-color: var(--line-strong);
    background: var(--surface);
    color: var(--ink);
    font-weight: 600;
  }

  .only {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    margin-left: auto;
    padding: 0 4px 0 8px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
    cursor: pointer;
  }

  .only input {
    margin: 0;
    accent-color: var(--accent);
  }

  .list {
    flex: 1;
    min-height: 0;
    padding: 0 6px calc(16px + var(--safe-bottom));
  }

  :global(.handheld) .list {
    padding-bottom: 16px;
  }

  h3 {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 37px;
    margin: 0;
    padding: 8px 4px 0 10px;
    background: var(--bg);
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: 550;
  }

  h3 .btn {
    height: 24px;
    font-weight: 500;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .row {
    display: grid;
    grid-template-columns: 10px 1fr auto;
    align-items: start;
    gap: 10px;
    width: 100%;
    padding: 7px 10px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    text-align: left;
    font-size: var(--fs-row);
    line-height: 1.4;
  }


  /* A label inside a sentence, such as "Rare bird". */
  .tag,
  .note {
    color: var(--seed-ink);
    font-weight: 650;
  }

  .note {
    font-size: var(--fs-sm);
  }

  .row .cell {
    margin-top: 4.5px;
  }

  .bird {
    margin: 4px 0 0 -3px;
  }

  .read .bird {
    opacity: 0.45;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .lead {
    color: var(--ink-2);
    overflow-wrap: anywhere;
  }

  .lead strong {
    color: var(--ink);
    font-weight: 620;
  }

  .lead img {
    width: 16px;
    height: 16px;
    margin-right: 5px;
    border-radius: 4px;
    vertical-align: -3px;
    background: var(--cell);
  }

  /* Read rows step back through weight and ink alone. */
  .read .lead {
    color: var(--ink-3);
  }

  .read .lead strong {
    color: var(--ink-2);
    font-weight: 540;
  }

  .read .detail {
    color: var(--ink-3);
  }

  .detail,
  .quote {
    color: var(--ink-3);
    font-size: var(--fs-sm);
    overflow: hidden;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .detail {
    color: var(--ink-2);
    text-wrap: pretty;
  }

  .quote {
    margin-top: 2px;
    padding-left: 8px;
    border-left: 2px solid var(--line-strong);
  }

  time {
    color: var(--ink-3);
    font-size: var(--fs-xs);
    padding-top: 2px;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    padding: 28px 10px;
    color: var(--ink-2);
    max-width: 34ch;
  }

  .more {
    margin: 12px 10px 0;
  }

  /* A finger gets one row of chips to swipe along, and taller rows to tap. */
  @media (pointer: coarse) {
    .head {
      min-height: 40px;
    }

    .filters {
      flex-wrap: nowrap;
      gap: 6px;
      overflow-x: auto;
      scrollbar-width: none;
    }

    .filters::-webkit-scrollbar {
      display: none;
    }

    .chip,
    .only {
      flex: none;
      height: 34px;
      padding: 0 12px;
      border-radius: 17px;
    }

    .chip:not([aria-pressed="true"]):not(.on),
    .only {
      border-color: var(--line);
    }

    .only {
      margin-left: 0;
      gap: 8px;
      border: 1px solid var(--line);
    }

    .only:has(input:checked) {
      border-color: var(--line-strong);
      background: var(--surface);
      color: var(--ink);
      font-weight: 600;
    }

    h3 {
      min-height: 44px;
    }

    h3 .btn {
      height: 34px;
    }

    .row {
      gap: 12px;
      padding: 11px 10px;
    }

    .row .cell {
      margin-top: 6px;
    }

    .bird {
      margin-top: 5px;
    }

    .lead img {
      width: 20px;
      height: 20px;
      vertical-align: -4px;
    }

    time {
      padding-top: 3px;
    }
  }
</style>
