<script lang="ts">
  import ActivityGrid from "./ActivityGrid.svelte";
  import Bird from "./Bird.svelte";
  import Figures, { type Figure } from "./Figures.svelte";
  import LineChart from "./LineChart.svelte";
  import Recap from "./Recap.svelte";
  import Streaks from "./Streaks.svelte";
  import UpNext from "./UpNext.svelte";
  import { num, shortDate, signed } from "./format";
  import { repoName, type Lane } from "./kinds";
  import { narrow, watcher } from "./state.svelte";

  const overview = $derived(watcher.overview);
  const hasReleases = $derived((watcher.repos ?? []).some((r) => r.release_count > 0));

  /** Narrows the activity list to one kind of event. */
  function show(lane: Lane) {
    watcher.setLane(lane);
    if (narrow.current) watcher.go("activity");
  }

  const figures = $derived.by((): Figure[] => {
    if (!overview) return [];
    const { totals, week } = overview;
    const period = "this week";
    const all: Figure[] = [
      { label: "Stars", value: totals.stars, change: week.stars, period, net: true, onpick: () => show("star") },
      { label: "Followers", value: totals.followers, change: week.followers, period, net: true, onpick: () => show("follower") },
      { label: "Forks", value: totals.forks, change: week.forks, period, onpick: () => show("fork") },
      { label: "Open issues", value: totals.open_issues, change: week.issues, period, onpick: () => show("issue") },
      { label: "Open PRs", value: totals.open_prs, change: week.prs, period, onpick: () => show("pr") },
    ];
    if (hasReleases) {
      all.push({ label: "Downloads", value: totals.downloads, change: week.downloads, period, net: true, onpick: () => watcher.go("downloads") });
    }
    return all;
  });

  const top = $derived((watcher.repos ?? []).filter((r) => r.stars > 0).slice(0, 6));
  const firstStar = $derived(overview?.star_series[0]?.date);
  const sightings = $derived.by((): [number, number] | undefined => {
    const all = watcher.lifeList?.sightings;
    return all && [all.filter((s) => s.seen).length, all.length];
  });
</script>

{#if overview && overview.totals.repos === 0}
  <section class="panel waiting">
    <Bird size={72} />
    {#if watcher.status.last_sync_at}
      <h2>No repositories to watch yet</h2>
      <p class="muted">
        Birdfeeder watches the repositories you own. Forks and organisation repositories can be
        included in Settings.
      </p>
      <button class="btn" onclick={() => watcher.go("settings")}>Open Settings</button>
    {:else}
      <h2>Filling the feeder</h2>
      <p class="muted">
        Birdfeeder is reading your repositories for the first time. It takes about twenty seconds,
        and nothing in this first pass will send a notification.
      </p>
    {/if}
  </section>
{:else if overview}
  <Recap />

  <section class="panel figures">
    <Figures {figures} />
  </section>

  <div class="pair">
    {#if overview.goals.length}
      <section class="panel">
        <header><h2>Up next</h2></header>
        <UpNext goals={overview.goals} />
      </section>
    {/if}
    <section class="panel">
      <header>
        <h2>Streaks and records</h2>
        <button class="btn quiet" onclick={() => watcher.go("lifelist")}>Life list</button>
      </header>
      <Streaks streak={overview.streak} records={overview.records} {sightings} />
    </section>
  </div>

  <section class="panel">
    <header>
      <h2>Activity by day</h2>
      <p class="scale" aria-label="Darker squares mean more events">
        <span class="step s1"></span>1
        <span class="step s2"></span>2 to 4
        <span class="step s3"></span>5 or more
      </p>
    </header>
    <ActivityGrid days={overview.days} contributions={overview.contributions} />
    <p class="note">Select a square or a row name to narrow the activity list.</p>
  </section>

  <section class="panel">
    <header>
      <h2>Stars over time</h2>
      {#if firstStar && overview.totals.stars > 0}<p class="muted">Across {num(overview.totals.repos)} repositories since {shortDate(firstStar)}</p>{/if}
    </header>
    <LineChart points={overview.star_series} unit={["star", "stars"]} color="var(--k-star)" height={210} />
  </section>

  {#if top.length}
    <section class="panel">
      <header>
        <h2>Most starred</h2>
        <button class="btn quiet" onclick={() => watcher.go("repos")}>All repositories</button>
      </header>
      <table>
        <thead>
          <tr>
            <th scope="col">Repository</th>
            <th scope="col" class="n">Stars</th>
            <th scope="col" class="n">Stars, 30 days</th>
            <th scope="col" class="n">Forks</th>
            {#if hasReleases}<th scope="col" class="n">Downloads</th>{/if}
          </tr>
        </thead>
        <tbody>
          {#each top as repo (repo.id)}
            <tr>
              <th scope="row">
                <button class="link" onclick={() => watcher.openRepo(repo.id)}>
                  {repoName(repo.name, watcher.login)}
                </button>
              </th>
              <td class="n">{num(repo.stars)}</td>
              <td class="n">
                {#if repo.stars_30d}
                  <span class={repo.stars_30d > 0 ? "up" : "down"}>{signed(repo.stars_30d)}</span>
                {:else}
                  <span class="muted">0</span>
                {/if}
              </td>
              <td class="n">{num(repo.forks)}</td>
              {#if hasReleases}
                <td class="n">
                  {#if repo.release_count}{num(repo.downloads)}{:else}<span class="muted">No releases</span>{/if}
                </td>
              {/if}
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
{/if}

<style>
  .panel {
    padding: 18px 20px;
  }

  .figures {
    padding: 18px 4px;
  }

  .pair {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 16px;
    align-items: start;
  }

  .waiting {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    max-width: 620px;
    padding: 28px 30px;
  }

  .waiting p {
    max-width: 56ch;
  }

  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 14px;
  }

  header p {
    font-size: var(--fs-sm);
  }

  header .btn {
    margin: -6px -8px;
    color: var(--accent);
  }

  .scale {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--ink-3);
    white-space: nowrap;
  }

  .step {
    width: 11px;
    height: 11px;
    margin-left: 8px;
    border-radius: var(--cell-radius);
  }

  .s1 {
    background: color-mix(in srgb, var(--ink-3) 55%, var(--cell));
  }

  .s2 {
    background: color-mix(in srgb, var(--ink-3) 78%, var(--cell));
  }

  .s3 {
    background: var(--ink-3);
  }

  .note {
    margin-top: 10px;
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13.5px;
  }

  th,
  td {
    padding: 7px 0 7px 16px;
    border-top: 1px solid var(--line);
    text-align: left;
    font-weight: inherit;
  }

  th:first-child {
    padding-left: 0;
  }

  thead th {
    border-top: 0;
    padding-top: 0;
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: 500;
  }

  .n {
    text-align: right;
    white-space: nowrap;
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    font-weight: 600;
    text-align: left;
  }

  .link:hover {
    color: var(--accent);
    text-decoration: underline;
  }

  .up {
    color: var(--up);
    font-weight: 600;
  }

  .down {
    color: var(--down);
    font-weight: 600;
  }
</style>
