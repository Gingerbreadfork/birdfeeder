<script lang="ts">
  import { open } from "./api";
  import { watcher } from "./state.svelte";
  import { RELEASE_PAGE } from "./updates";

  const update = $derived(watcher.update);
  const progress = $derived(watcher.updateProgress);
  const installing = $derived(progress !== null);
</script>

{#if update && !watcher.updateDismissed}
  <aside class="notice" role="status">
    <p>
      <strong>Birdfeeder {update.version}</strong> is available.
      {#if watcher.updateError && !installing}
        <span class="error">It could not be installed: {watcher.updateError}</span>
      {/if}
    </p>
    {#if update.install}
      <button class="btn quiet" onclick={() => open(`${RELEASE_PAGE}${update.version}`)}>What's new</button>
    {/if}
    <button class="btn primary" disabled={installing} onclick={() => watcher.installUpdate()}>
      {#if !update.install}
        Download
      {:else if progress === null}
        Install and restart
      {:else if progress > 0}
        Downloading {Math.round(progress * 100)}%
      {:else}
        Installing…
      {/if}
    </button>
    <button class="btn quiet" disabled={installing} onclick={() => (watcher.updateDismissed = true)}>
      Not now
    </button>
  </aside>
{/if}

<style>
  .notice {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
    padding: 8px calc(24px + var(--safe-right)) 8px 24px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
    font-size: var(--fs-sm);
  }

  p {
    flex: 1;
    min-width: 200px;
    color: var(--ink-2);
  }

  strong {
    color: var(--ink);
    font-weight: 650;
  }

  .error {
    color: var(--danger);
  }

  @media (max-width: 900px) {
    .notice {
      padding: 8px 12px;
    }
  }
</style>
