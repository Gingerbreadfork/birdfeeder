<script lang="ts">
  import Columns, { type Column } from "./Columns.svelte";
  import Figures, { type Figure } from "./Figures.svelte";
  import LineChart from "./LineChart.svelte";
  import ReleaseList from "./ReleaseList.svelte";
  import { open } from "./api";
  import { num, plural } from "./format";
  import { repoName } from "./kinds";
  import { narrow, watcher } from "./state.svelte";
  import type { RepoDetail } from "./types";

  let { detail }: { detail: RepoDetail } = $props();

  const repo = $derived(detail.repo);
  const name = $derived(repoName(repo.name, watcher.login));

  const figures = $derived.by((): Figure[] => [
    { label: "Stars", value: repo.stars, change: repo.stars_30d, period: "in 30 days", net: true },
    { label: "Forks", value: repo.forks },
    { label: "Watchers", value: repo.watchers },
    { label: "Open issues", value: repo.open_issues },
    { label: "Open PRs", value: repo.open_prs },
    ...(repo.release_count ? [{ label: "Downloads", value: repo.downloads }] : []),
  ]);

  const views = $derived(
    detail.traffic.map(
      (d): Column => ({
        date: d.date,
        value: d.views,
        note: `${plural(d.view_uniques, "visitor")}`,
      }),
    ),
  );
  const clones = $derived(
    detail.traffic.map(
      (d): Column => ({
        date: d.date,
        value: d.clones,
        note: `${plural(d.clone_uniques, "person", "people")}`,
      }),
    ),
  );
  const hasTraffic = $derived(detail.traffic.some((d) => d.views > 0 || d.clones > 0));
  const topReferrer = $derived(Math.max(1, ...detail.referrers.map((r) => r.count)));

  function showActivity() {
    watcher.setRepoFilter({ id: repo.id, name });
    if (narrow.current) watcher.go("activity");
  }
</script>

<nav>
  <button class="btn quiet" onclick={() => watcher.closeRepo()}>
    <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
      <path d="M9.5 3.5 5 8l4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
    All repositories
  </button>
</nav>

<section class="panel">
  <header class="title">
    <div>
      <h1>{name}</h1>
      {#if repo.description}<p class="about selectable">{repo.description}</p>{/if}
    </div>
    <div class="actions">
      <button class="btn" onclick={showActivity}>Show in activity list</button>
      <button class="btn" onclick={() => open(repo.url)}>Open on GitHub</button>
    </div>
  </header>
  <Figures {figures} />
</section>

<section class="panel">
  <header><h2>Stars over time</h2></header>
  {#if repo.stars > 0}
    <LineChart points={detail.star_series} unit={["star", "stars"]} color="var(--k-star)" height={190} />
  {:else}
    <p class="muted">No stars yet. The first one will show up in the activity list.</p>
  {/if}
</section>

{#if repo.release_count}
  <section class="panel">
    <header>
      <h2>Releases</h2>
      <p class="muted">{plural(repo.downloads, "download")} across {plural(detail.releases.length, "release")}</p>
    </header>
    <ReleaseList releases={detail.releases} />
  </section>
{/if}

<section class="panel">
  <header>
    <h2>Visitors</h2>
    <p class="muted">GitHub keeps 14 days of traffic. Birdfeeder keeps every day it sees.</p>
  </header>
  {#if hasTraffic}
    <div class="pair">
      <div>
        <h3>Page views per day</h3>
        <Columns data={views} unit={["view", "views"]} />
      </div>
      <div>
        <h3>Clones per day</h3>
        <Columns
          data={clones}
          unit={["clone", "clones"]}
          color="color-mix(in srgb, var(--ink-3) 55%, var(--cell))"
        />
      </div>
    </div>
    {#if detail.referrers.length}
      <h3 class="spaced">Where visitors came from, last 14 days</h3>
      <ul class="referrers">
        {#each detail.referrers as r (r.referrer)}
          <li>
            <span class="selectable">{r.referrer}</span>
            <span class="track" aria-hidden="true">
              <span class="fill" style:width="{(r.count / topReferrer) * 100}%"></span>
            </span>
            <span>{num(r.count)}</span>
            <span class="muted">{plural(r.uniques, "visitor")}</span>
          </li>
        {/each}
      </ul>
    {/if}
  {:else if repo.views.length}
    <p class="muted">No views or clones in the last 14 days.</p>
  {:else}
    <p class="muted">
      Traffic has not been read for this repository yet. It needs push access, and traffic tracking
      to be on in Settings.
    </p>
  {/if}
</section>

<style>
  nav {
    margin: -6px 0 -6px -8px;
  }

  .panel {
    padding: 18px 20px;
  }

  .title {
    align-items: flex-start;
    margin-bottom: 18px;
  }

  h1 {
    font-size: var(--fs-xl);
    font-weight: 680;
    font-stretch: 118%;
    line-height: 1.2;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  .about {
    margin-top: 4px;
    max-width: 68ch;
    color: var(--ink-2);
  }

  .actions {
    display: flex;
    flex: none;
    gap: 8px;
  }

  .panel :global(.clip) {
    margin: 0 -16px;
  }

  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }

  header p {
    font-size: var(--fs-sm);
  }

  h3 {
    margin-bottom: 8px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-weight: 550;
  }

  h3.spaced {
    margin-top: 18px;
  }

  .pair {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 20px 28px;
  }

  .referrers {
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--fs-row);
  }

  .referrers li {
    display: grid;
    grid-template-columns: minmax(120px, 22ch) 1fr 6ch 11ch;
    align-items: center;
    gap: 12px;
    padding: 4px 0;
  }

  .referrers li > :first-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .referrers li > :nth-child(n + 3) {
    text-align: right;
    white-space: nowrap;
  }

  .track {
    height: 8px;
  }

  .fill {
    display: block;
    min-width: 2px;
    height: 100%;
    border-radius: 0 4px 4px 0;
    background: var(--ink-3);
  }

  @media (max-width: 620px) {
    .title {
      flex-direction: column;
    }
  }

  @media (max-width: 600px) {
    .panel {
      padding: 16px;
    }

    .actions {
      flex-wrap: wrap;
      width: 100%;
    }

    .actions .btn {
      flex: 1;
      justify-content: center;
    }

    header {
      flex-wrap: wrap;
    }

    /* The bar and the count keep their place; unique visitors are left out. */
    .referrers li {
      grid-template-columns: minmax(0, 1fr) 28% 6ch;
    }

    .referrers li > :nth-child(4) {
      display: none;
    }
  }
</style>
