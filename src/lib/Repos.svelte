<script lang="ts">
  import RepoDetail from "./RepoDetail.svelte";
  import Spark from "./Spark.svelte";
  import { MediaQuery } from "svelte/reactivity";
  import { num, plural, signed } from "./format";
  import { repoName } from "./kinds";
  import { watcher } from "./state.svelte";
  import type { RepoRow } from "./types";

  /** Too narrow for a table: each repository becomes a row of its own. */
  const compact = new MediaQuery("max-width: 600px");

  type Key = "name" | "stars" | "stars_30d" | "forks" | "open" | "downloads" | "views";

  const columns: { key: Key; label: string; numeric: boolean; hint?: string; minor?: boolean }[] = [
    { key: "name", label: "Repository", numeric: false },
    { key: "stars", label: "Stars", numeric: true },
    { key: "stars_30d", label: "Stars, 30 days", numeric: true },
    { key: "forks", label: "Forks", numeric: true, minor: true },
    { key: "open", label: "Open", numeric: true, hint: "Open issues and pull requests", minor: true },
    { key: "downloads", label: "Downloads", numeric: true },
    { key: "views", label: "Views, 2 weeks", numeric: true },
  ];

  let sort = $state<{ key: Key; down: boolean }>({ key: "stars", down: true });
  let search = $state("");

  const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);

  function value(repo: RepoRow, key: Key): number | string {
    switch (key) {
      case "name":
        return repo.name.toLowerCase();
      case "open":
        return repo.open_issues + repo.open_prs;
      case "views":
        return sum(repo.views);
      default:
        return repo[key];
    }
  }

  const rows = $derived.by(() => {
    const needle = search.trim().toLowerCase();
    const list = (watcher.repos ?? []).filter((r) => r.name.toLowerCase().includes(needle));
    const { key, down } = sort;
    return list.sort((a, b) => {
      const [x, y] = [value(a, key), value(b, key)];
      const order = x < y ? -1 : x > y ? 1 : 0;
      return down ? -order : order;
    });
  });

  function sortBy(key: Key) {
    sort = sort.key === key ? { key, down: !sort.down } : { key, down: key !== "name" };
  }

  /** What the list is sorted by, said the way the row's figure reads. */
  const units: Record<Exclude<Key, "name">, [string, string]> = {
    stars: ["star", "stars"],
    stars_30d: ["star in 30 days", "stars in 30 days"],
    forks: ["fork", "forks"],
    open: ["open", "open"],
    downloads: ["download", "downloads"],
    views: ["view", "views"],
  };

  /** The figure a row leads with: whatever the list is sorted by, or its stars. */
  function lead(repo: RepoRow): { value: string; unit: string } {
    const key = sort.key === "name" ? "stars" : sort.key;
    const n = value(repo, key) as number;
    const [one, many] = units[key];
    return { value: key === "stars_30d" ? signed(n) : num(n), unit: n === 1 ? one : many };
  }

  /** The other figures worth a glance, in one short line. */
  function facts(repo: RepoRow): string[] {
    const out: string[] = [];
    if (sort.key !== "stars" && sort.key !== "name") out.push(plural(repo.stars, "star"));
    if (repo.stars_30d && sort.key !== "stars_30d") out.push(`${signed(repo.stars_30d)} in 30 days`);
    if (repo.release_count && sort.key !== "downloads") out.push(plural(repo.downloads, "download"));
    const views = sum(repo.views);
    if (views && sort.key !== "views") out.push(plural(views, "view"));
    return out;
  }
</script>

{#if watcher.detail}
  <RepoDetail detail={watcher.detail} />
{:else if watcher.repos}
  <section class="panel">
    <header>
      <h2>{num(watcher.repos.length)} repositories</h2>
      <input class="field" type="search" placeholder="Filter by name" bind:value={search} />
    </header>
    {#if compact.current}
      <label class="order">
        <span>Sort by</span>
        <select
          class="field"
          value={sort.key}
          onchange={(e) => {
            const key = e.currentTarget.value as Key;
            sort = { key, down: key !== "name" };
          }}
        >
          {#each columns as col (col.key)}
            <option value={col.key}>{col.key === "name" ? "Name" : (col.hint ?? col.label)}</option>
          {/each}
        </select>
      </label>
      <ul class="cards">
        {#each rows as repo (repo.id)}
          {@const top = lead(repo)}
          {@const more = facts(repo)}
          <li>
            <button onclick={() => watcher.openRepo(repo.id)}>
              <span class="what">
                <span class="title">
                  <strong>{repoName(repo.name, watcher.login)}</strong>
                  {#if repo.is_private}<span class="badge">Private</span>{/if}
                  {#if repo.is_archived}<span class="badge">Archived</span>{/if}
                  {#if repo.is_fork}<span class="badge">Fork</span>{/if}
                </span>
                {#if repo.description}<span class="about">{repo.description}</span>{/if}
                {#if more.length}<span class="facts">{more.join(" · ")}</span>{/if}
              </span>
              <span class="figure">
                <strong class={{ up: sort.key === "stars_30d" && repo.stars_30d > 0 }}>{top.value}</strong>
                <span>{top.unit}</span>
              </span>
            </button>
          </li>
        {:else}
          <li class="none">No repository matches “{search}”.</li>
        {/each}
      </ul>
    {:else}
      <div class="wide">
        <table>
          <thead>
            <tr>
              {#each columns as col (col.key)}
                <th
                  scope="col"
                  class={{ n: col.numeric, minor: col.minor }}
                  aria-sort={sort.key === col.key ? (sort.down ? "descending" : "ascending") : undefined}
                >
                  <button onclick={() => sortBy(col.key)} title={col.hint}>
                    {col.label}{#if sort.key === col.key}<span aria-hidden="true">{sort.down ? " ↓" : " ↑"}</span>{/if}
                  </button>
                </th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each rows as repo (repo.id)}
              <tr onclick={() => watcher.openRepo(repo.id)}>
                <th scope="row">
                  <button class="name" title={repo.description ?? undefined}>
                    {repoName(repo.name, watcher.login)}
                  </button>
                  {#if repo.is_private}<span class="badge">Private</span>{/if}
                  {#if repo.is_archived}<span class="badge">Archived</span>{/if}
                  {#if repo.is_fork}<span class="badge">Fork</span>{/if}
                </th>
                <td class="n">{num(repo.stars)}</td>
                <td class="n">
                  {#if repo.stars_30d}
                    <span class={repo.stars_30d > 0 ? "up" : "down"}>{signed(repo.stars_30d)}</span>
                  {/if}
                </td>
                <td class="n minor">{num(repo.forks)}</td>
                <td class="n minor" title="{repo.open_issues} issues, {repo.open_prs} pull requests">
                  {repo.open_issues + repo.open_prs || ""}
                </td>
                <td class="n">{repo.release_count ? num(repo.downloads) : ""}</td>
                <td class="n">
                  {#if repo.views.length && sum(repo.views) > 0}
                    <span class="views"><Spark values={repo.views} /><span>{num(sum(repo.views))}</span></span>
                  {/if}
                </td>
              </tr>
            {:else}
              <tr class="none"><td colspan={columns.length}>No repository matches “{search}”.</td></tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>
{/if}

<style>
  .panel {
    padding: 18px 20px;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }

  .field {
    width: 200px;
  }

  .wide {
    padding-bottom: 8px;
    overflow-x: auto;
  }

  table {
    width: 100%;
    min-width: 600px;
    border-collapse: collapse;
    font-size: var(--fs-row);
  }

  th,
  td {
    padding: 7px 0 7px 14px;
    border-top: 1px solid var(--line);
    text-align: left;
    font-weight: inherit;
    white-space: nowrap;
  }

  /* The name column takes the room that is left and truncates instead of pushing the table wide. */
  th:first-child {
    width: 100%;
    max-width: 0;
    padding-left: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  thead th {
    border-top: 0;
    padding-top: 0;
  }

  thead button {
    padding: 0;
    border: 0;
    background: none;
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: 500;
  }

  th[aria-sort] button {
    color: var(--ink);
  }

  @media (hover: hover) {
    thead button:hover {
      color: var(--ink);
    }
  }

  .n {
    text-align: right;
  }

  tbody tr {
    cursor: pointer;
  }

  @media (hover: hover) {
    tbody tr:hover {
      background: var(--accent-wash);
    }
  }

  tbody tr.none {
    cursor: default;
    background: none;
    color: var(--ink-3);
  }

  .name {
    margin-right: 6px;
    padding: 0;
    border: 0;
    background: none;
    font-weight: 600;
  }

  .badge {
    display: inline-block;
    padding: 1px 6px;
    border: 1px solid var(--line-strong);
    border-radius: 9px;
    color: var(--ink-3);
    font-size: 11px;
  }

  .views {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .views span {
    min-width: 5ch;
    text-align: right;
  }

  /* A narrow window keeps the columns that matter most and drops the rest. */
  @media (max-width: 600px) {
    header .field {
      flex: 1;
      min-width: 0;
      max-width: 220px;
    }
  }


  .order {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 6px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }

  .order .field {
    flex: 1;
    width: auto;
  }

  .cards {
    margin: 0 -12px;
    padding: 0;
    list-style: none;
  }

  .cards button {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    padding: 12px;
    border: 0;
    border-top: 1px solid var(--line);
    background: none;
    text-align: left;
    font-size: var(--fs-row);
  }

  .cards li:first-child button {
    border-top: 0;
  }

  .cards button:active {
    background: var(--accent-wash);
  }

  .what {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }

  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
  }

  .title strong {
    font-weight: 650;
    overflow-wrap: anywhere;
  }

  .about,
  .facts {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }

  .about {
    display: -webkit-box;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    color: var(--ink-2);
  }

  .figure {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    flex: none;
    max-width: 40%;
    text-align: right;
  }

  .figure strong {
    font-size: var(--fs-lg);
    font-weight: 680;
    font-stretch: 112%;
    line-height: 1.2;
  }

  .figure span {
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }

  .cards .none {
    padding: 12px;
    color: var(--ink-3);
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
