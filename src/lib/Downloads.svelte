<script lang="ts">
  import Bird from "./Bird.svelte";
  import Columns, { type Column } from "./Columns.svelte";
  import ReleaseList from "./ReleaseList.svelte";
  import { addDays, localDay, num, plural, shortDate } from "./format";
  import { repoName } from "./kinds";
  import { watcher } from "./state.svelte";

  const downloads = $derived(watcher.downloads);

  /** Daily gains for the last 30 days, or since tracking began if that is shorter. */
  const daily = $derived.by((): Column[] => {
    if (!downloads?.tracking_since) return [];
    const today = localDay(new Date(watcher.now));
    const first = localDay(downloads.tracking_since);
    const byDay = new Map(downloads.gained.map((p) => [p.date, p.value]));
    const out: Column[] = [];
    for (let day = addDays(today, -29); day <= today; day = addDays(day, 1)) {
      if (day >= first) out.push({ date: day, value: byDay.get(day) ?? 0 });
    }
    return out;
  });
  const gainedTotal = $derived(daily.reduce((sum, d) => sum + d.value, 0));
</script>

{#if downloads && !downloads.repos.length}
  <section class="panel none">
    <Bird size={64} />
    <h2>No releases yet</h2>
    <p class="muted">
      None of your repositories has a release with files. Downloads will be counted here as soon
      as one does.
    </p>
  </section>
{:else if downloads}
  <section class="panel lead">
    <p class="hero" title={num(downloads.total)}>{num(downloads.total)}</p>
    <div>
      <h2>Release downloads</h2>
      <p class="muted">
        Across {plural(downloads.repos.length, "repository", "repositories")}.
        {#if downloads.ignore_assets.length}
          Files matching
          {#each downloads.ignore_assets as pattern, i (i)}{i ? ", " : " "}<span class="mono">{pattern}</span>{/each}
          are not counted.
          <button class="inline" onclick={() => watcher.go("settings")}>Change in Settings</button>
        {/if}
      </p>
    </div>
  </section>

  <section class="panel">
    <header>
      <h2>New downloads per day</h2>
      {#if daily.length > 1}
        <p class="muted">
          {plural(gainedTotal, "download")} since {shortDate(daily[0]?.date ?? downloads.tracking_since)}
        </p>
      {/if}
    </header>
    {#if daily.length > 1}
      <Columns data={daily} unit={["download", "downloads"]} color="var(--k-downloads)" />
    {:else}
      <p class="muted explain">
        GitHub only reports running totals, so daily numbers start from the day Birdfeeder began
        counting{#if gainedTotal > 0}: {plural(gainedTotal, "download")} so far today{/if}. A chart
        appears here from tomorrow.
      </p>
    {/if}
  </section>

  {#each downloads.repos as repo (repo.id)}
    <section class="panel">
      <header>
        <h2>
          <button class="link" onclick={() => watcher.openRepo(repo.id)}>
            {repoName(repo.name, watcher.login)}
          </button>
        </h2>
        <p class="muted">{plural(repo.total, "download")}</p>
      </header>
      <ReleaseList releases={repo.releases} limit={5} />
    </section>
  {/each}
{/if}

<style>
  .panel {
    padding: 18px 20px;
  }

  .lead {
    display: flex;
    align-items: center;
    gap: 22px;
  }

  .none {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    max-width: 620px;
    padding: 28px 30px;
  }

  .none p {
    max-width: 56ch;
  }

  .hero {
    font-size: 52px;
    font-weight: 680;
    font-stretch: 118%;
    line-height: 1;
    letter-spacing: -0.02em;
  }

  .lead .muted {
    margin-top: 4px;
    max-width: 62ch;
    font-size: var(--fs-sm);
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

  .explain {
    max-width: 68ch;
  }

  .link,
  .inline {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
  }

  .link:hover,
  .inline:hover {
    text-decoration: underline;
  }

  .inline {
    color: var(--accent);
  }

  @media (max-width: 560px) {
    .lead {
      flex-direction: column;
      align-items: flex-start;
      gap: 10px;
    }
  }
</style>
