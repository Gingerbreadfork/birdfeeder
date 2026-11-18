<script lang="ts">
  import { api, open } from "./api";
  import { mobile } from "./platform";

  /** GitHub's form for a new classic token, with the scope and a name filled in. */
  const NEW_TOKEN = "https://github.com/settings/tokens/new?scopes=repo&description=Birdfeeder";

  let { onsaved }: { onsaved?: (login: string) => void } = $props();

  let token = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      const login = await api.saveToken(token);
      token = "";
      onsaved?.(login);
    } catch (reason) {
      error = String(reason);
    } finally {
      busy = false;
    }
  }
</script>

<form onsubmit={save}>
  <!-- On a phone the token is made first, in the browser, then pasted below. -->
  {#if mobile}
    <button type="button" class="btn create" onclick={() => open(NEW_TOKEN)}>
      Create a token on GitHub
    </button>
  {/if}
  <div class="entry">
    <input
      class="field mono"
      type="password"
      placeholder="ghp_…"
      autocomplete="off"
      autocapitalize="off"
      enterkeyhint="done"
      spellcheck="false"
      aria-label="Personal access token"
      bind:value={token}
    />
    <button class="btn primary" disabled={busy || !token.trim()}>
      {busy ? "Checking…" : "Save token"}
    </button>
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <p class="hint">
    A classic token with the <span class="mono">repo</span> scope covers private repositories,
    traffic and the inbox.
    {mobile
      ? "It stays in Birdfeeder's private storage on this phone."
      : "It is kept in your system keyring."}
  </p>
  {#if !mobile}
    <button type="button" class="btn" onclick={() => open(NEW_TOKEN)}>Create a token on GitHub</button>
  {/if}
</form>

<style>
  .entry {
    display: flex;
    gap: 8px;
  }

  .field {
    flex: 1;
    min-width: 0;
    max-width: 360px;
  }

  .hint,
  .error {
    margin-top: 8px;
    font-size: var(--fs-sm);
  }

  .hint {
    max-width: 60ch;
    color: var(--ink-3);
  }

  .error {
    color: var(--danger);
  }

  @media (max-width: 600px) {
    .entry {
      flex-direction: column;
    }

    .field {
      flex: none;
      max-width: none;
    }

    .entry .btn {
      justify-content: center;
    }
  }

  form > .btn {
    margin-top: 10px;
  }

  form > .create {
    width: 100%;
    margin: 0 0 12px;
    justify-content: center;
  }
</style>
