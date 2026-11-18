<script lang="ts">
  import type { Page } from "./state.svelte";

  let {
    tabs,
    current,
    unread,
    onpick,
  }: {
    tabs: { id: Page; label: string }[];
    current: Page;
    unread: number;
    onpick: (page: Page) => void;
  } = $props();
</script>

{#snippet icon(id: Page)}
  <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
    <g fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
      {#if id === "overview"}
        <rect x="4" y="4" width="7" height="7" rx="1.6" />
        <rect x="13" y="4" width="7" height="7" rx="1.6" class="fill" />
        <rect x="4" y="13" width="7" height="7" rx="1.6" class="fill" />
        <rect x="13" y="13" width="7" height="7" rx="1.6" />
      {:else if id === "activity"}
        <rect x="4" y="5" width="4" height="4" rx="1" class="fill" />
        <rect x="4" y="15" width="4" height="4" rx="1" class="fill" />
        <path d="M11 6h9M11 9h5M11 16h9M11 19h5" />
      {:else if id === "repos"}
        <path d="M6.5 3.5H19v14H6.5A2.5 2.5 0 0 0 4 20V6a2.5 2.5 0 0 1 2.5-2.5Z" class="fill" />
        <path d="M4 20a2.5 2.5 0 0 1 2.5-2.5H19V21H6.5" />
      {:else if id === "downloads"}
        <path d="M12 3.5v11M7.5 10l4.5 4.5 4.5-4.5" />
        <path d="M4.5 15.5v2.5a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2v-2.5" />
      {:else if id === "lifelist"}
        <path d="M4 7.5 6 9.5l3.5-4M4 16.5l2 2 3.5-4" />
        <path d="M13 8h7M13 17h7" />
      {/if}
    </g>
  </svg>
{/snippet}

<nav class="bar" aria-label="Sections">
  {#each tabs as tab (tab.id)}
    <button
      aria-current={current === tab.id ? "page" : undefined}
      onclick={() => onpick(tab.id)}
    >
      <span class="pill">
        {@render icon(tab.id)}
        {#if tab.id === "activity" && unread > 0}
          <span class="badge" aria-label="{unread} unread">{unread > 99 ? "99+" : unread}</span>
        {/if}
      </span>
      <span class="label">{tab.label}</span>
    </button>
  {/each}
</nav>

<style>
  .bar {
    display: grid;
    grid-auto-columns: 1fr;
    grid-auto-flow: column;
    flex: none;
    padding: 6px max(4px, var(--safe-right)) calc(6px + var(--safe-bottom))
      max(4px, var(--safe-left));
    border-top: 1px solid var(--line);
    background: var(--surface);
  }

  button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    min-width: 0;
    padding: 4px 0 2px;
    border: 0;
    background: none;
    color: var(--ink-2);
  }

  .pill {
    position: relative;
    display: grid;
    place-items: center;
    width: 56px;
    height: 30px;
    border-radius: 15px;
    transition: background 0.18s;
  }

  .label {
    max-width: 100%;
    overflow: hidden;
    font-size: 12px;
    font-weight: 550;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  [aria-current="page"] {
    color: var(--ink);
  }

  [aria-current="page"] .pill {
    background: var(--accent-wash);
  }

  [aria-current="page"] .label {
    font-weight: 700;
  }

  /* The current tab fills in part of its icon. */
  [aria-current="page"] .fill {
    fill: currentColor;
  }

  button:active .pill {
    background: var(--accent-wash);
  }

  .badge {
    position: absolute;
    top: -2px;
    left: 32px;
    min-width: 18px;
    padding: 0 5px;
    border: 2px solid var(--surface);
    border-radius: 10px;
    background: var(--seed);
    color: #17211b;
    font-size: 10.5px;
    font-weight: 700;
    line-height: 14px;
    text-align: center;
  }
</style>
