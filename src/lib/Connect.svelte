<script lang="ts">
  import Bird from "./Bird.svelte";
  import SignIn from "./SignIn.svelte";
  import TokenForm from "./TokenForm.svelte";
  import { api } from "./api";
  import { mobile } from "./platform";
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
  {:else if mobile}
    <h1>{rejected ? "GitHub rejected the access token" : "Connect a GitHub account"}</h1>
    <p>
      {#if rejected}
        GitHub no longer accepts the token Birdfeeder was using. Sign in again to carry on.
      {:else}
        Sign in through GitHub in your browser. Birdfeeder only reads, and what it learns stays on
        this phone.
      {/if}
    </p>
    <div class="first"><SignIn /></div>

    <details>
      <summary>Use a personal access token instead</summary>
      <ol class="steps">
        <li>Create a token on GitHub. The link fills in the name and the one scope it needs.</li>
        <li>Copy the token GitHub shows you. It starts with <span class="mono">ghp_</span>.</li>
        <li>Come back and paste it here.</li>
      </ol>
      <TokenForm />
    </details>
  {:else}
    <h1>{rejected ? "GitHub rejected the access token" : "Connect a GitHub account"}</h1>
    <p>
      {#if rejected && status.token_source === "saved"}
        GitHub no longer accepts the token Birdfeeder saved. Sign in again, paste a new token, or
        remove the saved one in Settings to use the GitHub CLI's login.
      {:else if rejected}
        The token Birdfeeder was using has expired or been revoked. Sign in again below, sign in
        again with the GitHub CLI, or paste a new token.
      {:else}
        Sign in through GitHub in your browser. Birdfeeder also uses the GitHub CLI's login when it
        finds one, and there is none on this computer yet.
      {/if}
    </p>

    <h2>With your browser</h2>
    <SignIn />

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
    margin: 24px 0 8px;
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

  .first {
    margin: 18px 0 8px;
  }

  details {
    margin-top: 20px;
    border-top: 1px solid var(--line);
  }

  summary {
    display: flex;
    align-items: center;
    min-height: var(--tap);
    padding-top: 12px;
    color: var(--ink-2);
    font-weight: 550;
    cursor: pointer;
  }

  details[open] summary {
    color: var(--ink);
  }

  .steps {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 12px 0 18px;
    padding-left: 22px;
    color: var(--ink-2);
  }

  .steps li::marker {
    color: var(--ink);
    font-weight: 650;
  }

  @media (max-width: 600px) {
    .panel {
      padding: 22px 18px;
    }
  }
</style>
