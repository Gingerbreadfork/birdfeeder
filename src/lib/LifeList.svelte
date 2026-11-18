<script lang="ts">
  import Bird from "./Bird.svelte";
  import { open } from "./api";
  import { ago, num, shortDate } from "./format";
  import { repoName } from "./kinds";
  import { watcher } from "./state.svelte";
  import type { Sighting, Visitor } from "./types";

  const list = $derived(watcher.lifeList);

  const share = (s: Sighting) => (s.progress ? s.progress[0] / s.progress[1] : -1);
  /** What is left to see, nearest first. Entries that cannot be counted come last. */
  const open_ = $derived(
    (list?.sightings ?? []).filter((s) => !s.seen).sort((a, b) => share(b) - share(a)),
  );
  /** What has been seen, newest first. Entries with no date come last. */
  const seen = $derived(
    (list?.sightings ?? [])
      .filter((s) => s.seen)
      .sort((a, b) => (b.when ?? "").localeCompare(a.when ?? "")),
  );

  const did: Record<string, string> = { star: "starred", fork: "forked", follower: "followed you" };

  function action(v: Visitor): string {
    const repo = v.kind === "follower" ? "" : ` ${repoName(v.repo, watcher.login)}`;
    return `${did[v.kind] ?? v.kind}${repo}`;
  }

  function avatar(url: string): string {
    return url.startsWith("https://") ? `${url}${url.includes("?") ? "&" : "?"}s=64` : url;
  }
</script>

{#snippet entry(s: Sighting)}
  <li class={{ seen: s.seen }}>
    <span class={["cell", { hollow: !s.seen }]} aria-hidden="true"></span>
    <div>
      <strong>{s.title}</strong>
      <span class="how">{s.how}</span>
      {#if s.seen}
        <span class="note">
          {s.when ? shortDate(s.when) : "Seen"}{s.detail ? `, ${s.detail}` : ""}
        </span>
      {:else if s.progress}
        <span class="note">{num(s.progress[0])} of {num(s.progress[1])}</span>
        <span class="track" aria-hidden="true">
          <span style:width="{Math.min(100, (s.progress[0] / s.progress[1]) * 100)}%"></span>
        </span>
      {/if}
    </div>
  </li>
{/snippet}

{#if list}
  <section class="panel">
    <header>
      <h2>Life list</h2>
      <p class="muted">{seen.length} of {list.sightings.length} seen</p>
    </header>
    <div class="track whole" aria-hidden="true">
      <span style:width="{(seen.length / Math.max(1, list.sightings.length)) * 100}%"></span>
    </div>
    <p class="about">
      Birders keep a list of every species they have seen. This is yours: the things that can
      happen to a repository, ticked off as they do.
    </p>

    {#if open_.length}
      <h3>Still to see</h3>
      <ul class="sightings">
        {#each open_ as s (s.id)}{@render entry(s)}{/each}
      </ul>
    {/if}
    {#if seen.length}
      <h3>Seen</h3>
      <ul class="sightings">
        {#each seen as s (s.id)}{@render entry(s)}{/each}
      </ul>
    {/if}
  </section>

  {#if list.visitors.length}
    <section class="panel">
      <header>
        <h2>Most-followed visitors</h2>
        <p class="muted key">
          <Bird size={18} perch={false} /> marks a rare bird, with 1,000 followers or more
        </p>
      </header>
      <ul class="visitors">
        {#each list.visitors as v (v.login)}
          <li>
            <button onclick={() => open(`https://github.com/${v.login}`)}>
              {#if v.avatar}<img src={avatar(v.avatar)} alt="" width="32" height="32" loading="lazy" />{/if}
              <span class="who">
                <strong>{v.name || v.login}</strong>
                <span class="muted">
                  {v.company ? `${v.company.trim().replace(/^@/, "")}, ` : ""}{action(v)}
                  {ago(v.when, watcher.now)}
                </span>
              </span>
              <span class="rare">
                {#if v.followers >= 1000}
                  <span role="img" aria-label="Rare bird" title="A rare bird: 1,000 followers or more">
                    <Bird size={24} perch={false} />
                  </span>
                {/if}
              </span>
              <span class="followers">
                <strong>{num(v.followers)}</strong>
                <span class="muted">followers</span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
{/if}

<style>
  .panel {
    padding: 18px 20px;
  }

  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }

  header p {
    font-size: var(--fs-sm);
  }

  .about {
    max-width: 66ch;
    margin: 12px 0 4px;
    color: var(--ink-2);
    font-size: var(--fs-row);
  }

  h3 {
    margin: 18px 0 6px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-weight: 600;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .sightings {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 0 28px;
  }

  .sightings li {
    display: flex;
    gap: 10px;
    padding: 9px 0;
    border-top: 1px solid var(--line);
    font-size: var(--fs-row);
  }

  .sightings .cell {
    --kind: var(--seed);
    margin-top: 4px;
  }

  .sightings .cell.hollow {
    --kind: var(--ink-3);
  }

  .sightings div {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }

  .how {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }

  .note {
    margin-top: 3px;
    font-size: var(--fs-sm);
    font-weight: 600;
  }

  .seen .note {
    color: var(--ink-2);
    font-weight: 500;
  }

  .track {
    height: 4px;
    margin-top: 4px;
    border-radius: 2px;
    background: var(--cell);
    overflow: hidden;
  }

  .track span {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: var(--seed);
  }

  .track.whole {
    margin-top: 0;
  }

  .key {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .visitors button {
    display: flex;
    align-items: center;
    gap: 12px;
    width: calc(100% + 12px);
    padding: 8px 6px;
    margin: 0 -6px;
    border: 0;
    border-top: 1px solid var(--line);
    background: none;
    text-align: left;
    font-size: var(--fs-row);
  }

  .visitors button:active {
    background: var(--accent-wash);
  }

  @media (hover: hover) {
    .visitors button:hover {
      background: var(--accent-wash);
    }
  }

  .visitors img {
    flex: none;
    border-radius: 7px;
    background: var(--cell);
  }

  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }

  .who .muted {
    font-size: var(--fs-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* A fixed slot, so the birds line up whatever the follower count beside them. */
  .rare {
    flex: none;
    width: 24px;
  }

  .followers {
    display: flex;
    align-items: baseline;
    justify-content: flex-end;
    gap: 6px;
    flex: none;
    min-width: 13ch;
    white-space: nowrap;
  }

  .followers .muted {
    font-size: var(--fs-sm);
  }

  @media (max-width: 600px) {
    .panel {
      padding: 16px;
    }
  }
</style>
