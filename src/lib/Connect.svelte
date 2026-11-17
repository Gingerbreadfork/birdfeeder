<script lang="ts">
  import Bird from "./Bird.svelte";
  import TokenForm from "./TokenForm.svelte";
  import { api } from "./api";
  import { watcher } from "./state.svelte";

  const status = $derived(watcher.status);
  const rejected = $derived(status.error_kind === "auth");
  const otherAccount = $derived(status.error_kind === "account");
  const checking = $derived(status.phase === "syncing");
</script>

<section class="panel">
  <Bird size={72} />
  {#if otherAccount}
    <h1>This is a different GitHub account</h1>
    <p>{status.error}</p>
    <p>
      Starting over clears the activity, traffic history and life list collected so far, and
      keeps your settings. To carry on with the first account instead, sign in to it again.
    </p>
    <button class="btn" onclick={() => api.startOver()} disabled={checking}>
      {checking ? "Checking…" : "Start over with this account"}
    </button>
  {:else}
    <h1>{rejected ? "GitHub rejected the access token" : "Connect a GitHub account"}</h1>
    <p>
      {#if rejected && status.token_source === "saved"}
        The token you saved has expired or been revoked. Paste a new one below, or remove it in
        Settings to use the GitHub CLI's login.
      {:else if rejected}
        The token Birdfeeder was using has expired or been revoked. Sign in again with the GitHub
        CLI, or paste a new token.
      {:else}
        Birdfeeder reads your account with the GitHub CLI's login when it finds one. There is none
        on this computer yet.
      {/if}
    </p>

    <h2>With the GitHub CLI</h2>
    <p>
      Run <span class="mono selectable">gh auth login</span> in a terminal, then check again.
    </p>
    <button class="btn" onclick={() => watcher.syncNow()} disabled={checking}>
      {checking ? "Checking…" : "Check again"}
    </button>

    <h2>With a personal access token</h2>
    <TokenForm />
  {/if}
</section>

<style>
  .panel {
    max-width: 620px;
    padding: 28px 30px;
  }

  h1 {
    margin: 16px 0 8px;
    font-size: var(--fs-xl);
    font-weight: 680;
    font-stretch: 118%;
    line-height: 1.2;
  }

  h2 {
    margin: 24px 0 6px;
  }

  p {
    max-width: 60ch;
    color: var(--ink-2);
  }

  p + p {
    margin-top: 8px;
  }

  .btn {
    margin-top: 10px;
  }
</style>
