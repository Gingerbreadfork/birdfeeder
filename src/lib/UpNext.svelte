<script lang="ts">
  import { num, plural } from "./format";
  import { laneColor, repoName } from "./kinds";
  import { watcher } from "./state.svelte";
  import type { Goal } from "./types";

  let { goals }: { goals: Goal[] } = $props();

  const colors = {
    stars: laneColor("star"),
    "all-stars": laneColor("star"),
    downloads: laneColor("downloads"),
    followers: laneColor("follower"),
  };

  function label(g: Goal): string {
    const repo = repoName(g.repo, watcher.login);
    switch (g.what) {
      case "stars":
        return `${repo} to ${plural(g.target, "star")}`;
      case "downloads":
        return `${repo} to ${plural(g.target, "download")}`;
      case "all-stars":
        return `${plural(g.target, "star")} in total`;
      case "followers":
        return `${plural(g.target, "follower")}`;
    }
  }

  function pace(days: number): string {
    if (days < 2) return "About a day away at this pace";
    if (days < 14) return `About ${days} days away at this pace`;
    if (days < 70) return `About ${Math.round(days / 7)} weeks away at this pace`;
    return `About ${Math.round(days / 30)} months away at this pace`;
  }

  const filled = (g: Goal) => ((g.current - g.from) / Math.max(1, g.target - g.from)) * 100;
</script>

<ul>
  {#each goals as goal (`${goal.what}:${goal.repo_id}`)}
    <li style:--series={colors[goal.what]}>
      <div class="line">
        {#if goal.repo_id}
          <button class="name" onclick={() => watcher.openRepo(goal.repo_id!)}>{label(goal)}</button>
        {:else}
          <span class="name">{label(goal)}</span>
        {/if}
        <strong>{num(goal.target - goal.current)} to go</strong>
      </div>
      <div
        class="track"
        role="progressbar"
        aria-valuemin={goal.from}
        aria-valuemax={goal.target}
        aria-valuenow={goal.current}
        aria-label={label(goal)}
      >
        <span style:width="{filled(goal)}%"></span>
      </div>
      {#if goal.eta_days}<p class="muted">{pace(goal.eta_days)}</p>{/if}
    </li>
  {/each}
</ul>

<style>
  ul {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .line {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 6px;
    font-size: var(--fs-row);
  }

  .name {
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    overflow: hidden;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @media (hover: hover) {
    button.name:hover {
      color: var(--accent);
      text-decoration: underline;
    }
  }

  strong {
    flex: none;
    font-weight: 650;
  }

  .track {
    height: 8px;
    border-radius: 4px;
    background: var(--cell);
    overflow: hidden;
  }

  .track span {
    display: block;
    min-width: 4px;
    height: 100%;
    border-radius: 4px;
    background: var(--series);
  }

  p {
    margin-top: 4px;
    font-size: var(--fs-xs);
  }
</style>
