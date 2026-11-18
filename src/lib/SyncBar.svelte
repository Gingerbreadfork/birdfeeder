<script lang="ts">
  import { open } from "./api";
  import { ago, clock, until } from "./format";
  import { narrow, watcher } from "./state.svelte";

  const status = $derived(watcher.status);
  const account = $derived(watcher.overview?.account ?? null);
  const syncing = $derived(status.phase === "syncing");

  const line = $derived.by(() => {
    if (status.demo) return "Sample data. Nothing is being checked.";
    switch (status.phase) {
      case "syncing":
        return status.step ? `${status.step}…` : "Checking…";
      case "error":
        return status.error_kind === "offline"
          ? "Can't reach GitHub. Trying again shortly."
          : status.error_kind === "auth"
            ? "GitHub rejected the access token."
            : status.error_kind === "account"
              ? "Signed in to a different account."
            : status.error_kind === "rate_limit"
              ? `GitHub's request limit was reached. ${
                  status.next_sync_at
                    ? `Checking again at ${clock(status.next_sync_at)}.`
                    : "Waiting for it to reset."
                }`
              : (status.error ?? "The last check failed.");
      case "signed_out":
        return "Not connected to GitHub";
      default: {
        const next = until(status.next_sync_at, watcher.now);
        return `Checked ${ago(status.last_sync_at, watcher.now)}${next ? `, next ${next}` : ""}`;
      }
    }
  });
</script>

<header class="sync">
  {#if account}
    <button class="who" onclick={() => open(account.html_url)} title="Open your GitHub profile">
      <img src={account.avatar_url} alt="" width="32" height="32" />
    </button>
  {:else}
    <span class="who blank"></span>
  {/if}
  <div class="text">
    <strong>{account?.login ?? status.login ?? "Birdfeeder"}</strong>
    <span class={["line", status.phase]} title={status.error ?? undefined}>{line}</span>
  </div>
  {#if !status.demo}
    <button
      class="btn quiet icon"
      onclick={() => watcher.syncNow()}
      disabled={syncing}
      aria-label="Check now"
      title="Check now (Ctrl+R)"
    >
      <svg class={{ spin: syncing }} viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
        <path
          d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9M13.5 2.5v2.6h-2.6"
          fill="none"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </button>
  {/if}
  {#if narrow.current}
    <button
      class="btn quiet icon"
      onclick={() => watcher.go("settings")}
      aria-label="Settings"
      aria-current={watcher.page === "settings" ? "page" : undefined}
      title="Settings"
    >
      <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
        <g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
          <path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />
          <circle cx="10.5" cy="4.5" r="1.6" fill="var(--bg)" />
          <circle cx="5.5" cy="8" r="1.6" fill="var(--bg)" />
          <circle cx="9" cy="11.5" r="1.6" fill="var(--bg)" />
        </g>
      </svg>
    </button>
  {/if}
</header>

<style>
  .sync {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 14px 16px 12px;
  }

  .who {
    flex: none;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: 7px;
    background: var(--cell);
    overflow: hidden;
  }

  .who img {
    display: block;
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
    margin-left: 4px;
    line-height: 1.3;
  }

  strong {
    font-weight: 650;
    font-stretch: 112%;
  }

  .line {
    color: var(--ink-3);
    font-size: var(--fs-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .line.error,
  .line.signed_out {
    color: var(--danger);
  }

  .icon {
    flex: none;
    width: var(--tap);
    padding: 0;
    justify-content: center;
  }

  .icon[aria-current="page"] {
    background: var(--accent-wash);
    color: var(--ink);
  }

  .spin {
    animation: turn 0.9s linear infinite;
  }

  @keyframes turn {
    to {
      transform: rotate(360deg);
    }
  }
</style>
