<script lang="ts">
  import { open } from "./api";
  import { bytes, num, shortDate } from "./format";
  import type { ReleaseRow } from "./types";

  let {
    releases,
    limit = 6,
  }: {
    releases: ReleaseRow[];
    /** How many releases to list before the rest fold away. */
    limit?: number;
  } = $props();

  let all = $state(false);

  const scale = $derived(Math.max(1, ...releases.map((r) => r.downloads)));
  const shown = $derived(all ? releases : releases.slice(0, limit));
</script>

<ul class="releases">
  {#each shown as release (release.tag)}
    <li>
      <details>
        <summary>
          <svg class="chevron" viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
            <path d="M4.5 2.5 8 6l-3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
          <span class="tag mono">{release.tag}</span>
          <span class="when muted">
            {release.published_at ? shortDate(release.published_at) : ""}{release.is_prerelease
              ? ", pre-release"
              : ""}
          </span>
          <span class="track" aria-hidden="true">
            <span class="fill" style:width="{(release.downloads / scale) * 100}%"></span>
          </span>
          <span class="count">{num(release.downloads)}</span>
        </summary>
        <div class="assets">
          {#each release.assets as asset (asset.name)}
            <div class={["asset", { ignored: asset.ignored }]}>
              <span class="mono selectable">{asset.name}</span>
              <span class="muted">{bytes(asset.size)}</span>
              <span>{asset.ignored ? "not counted" : num(asset.downloads)}</span>
            </div>
          {:else}
            <p class="muted">This release has no files attached.</p>
          {/each}
          {#if release.url}
            <button class="btn quiet" onclick={() => open(release.url)}>Open release on GitHub</button>
          {/if}
        </div>
      </details>
    </li>
  {/each}
</ul>
{#if releases.length > limit}
  <button class="btn quiet toggle" onclick={() => (all = !all)}>
    {all ? "Show fewer" : `Show all ${releases.length} releases`}
  </button>
{/if}

<style>
  .releases {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  summary {
    display: grid;
    grid-template-columns: 12px minmax(84px, 15ch) minmax(60px, 11ch) 1fr 6ch;
    align-items: baseline;
    gap: 10px;
    padding: 5px 6px;
    margin: 0 -6px;
    border-radius: var(--radius);
    font-size: var(--fs-row);
    cursor: pointer;
    list-style: none;
  }

  summary::-webkit-details-marker {
    display: none;
  }

  @media (hover: hover) {
    summary:hover {
      background: var(--accent-wash);
    }
  }

  .chevron {
    align-self: center;
    color: var(--ink-3);
    transition: transform 0.12s;
  }

  details[open] .chevron {
    transform: rotate(90deg);
  }

  .tag {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .when {
    font-size: var(--fs-sm);
    white-space: nowrap;
  }

  .track {
    align-self: center;
    height: 8px;
  }

  .fill {
    display: block;
    min-width: 2px;
    height: 100%;
    border-radius: 0 4px 4px 0;
    background: var(--k-downloads);
  }

  .count {
    text-align: right;
    font-weight: 600;
  }

  .assets {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
    margin: 4px 0 10px 22px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--surface-2);
    font-size: var(--fs-sm);
  }

  .asset {
    display: grid;
    grid-template-columns: 1fr 8ch 11ch;
    gap: 12px;
    width: 100%;
  }

  .asset > :first-child {
    overflow-wrap: anywhere;
  }

  .asset > :not(:first-child) {
    text-align: right;
    white-space: nowrap;
  }

  .ignored {
    color: var(--ink-3);
  }

  .assets .btn {
    margin: 6px 0 -4px -8px;
  }

  .toggle {
    margin: 4px 0 -6px -8px;
  }

  @media (pointer: coarse) {
    summary {
      padding-top: 10px;
      padding-bottom: 10px;
    }

    summary:active {
      background: var(--accent-wash);
    }
  }

  @media (max-width: 600px) {
    summary {
      grid-template-columns: 12px minmax(0, 1fr) auto 56px 6ch;
    }

    .assets {
      margin-left: 0;
    }

    .asset {
      grid-template-columns: minmax(0, 1fr) auto auto;
    }
  }
</style>
