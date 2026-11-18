<script lang="ts">
  import { signIn } from "./signin.svelte";

  let {
    onsigned,
    primary = true,
  }: {
    onsigned?: (login: string) => void;
    /** Whether this is the main way offered, which makes its button the prominent one. */
    primary?: boolean;
  } = $props();
</script>

{#if signIn.signedIn}
  <p class="done" role="status">Signed in as <strong>{signIn.signedIn}</strong>.</p>
{:else if signIn.code}
  {@const code = signIn.code}
  <div class="code" role="group" aria-label="Signing in through GitHub">
    <p>Enter this code on GitHub, then approve Birdfeeder:</p>
    <p class="digits mono selectable">{code.user_code}</p>
    <div class="actions">
      <button class="btn primary" onclick={() => signIn.copyAndOpen()}>
        {signIn.copied ? "Open GitHub again" : "Copy code and open GitHub"}
      </button>
      <button class="btn quiet" onclick={() => signIn.cancel()}>Cancel</button>
    </div>
    <p class="waiting" role="status">
      <span class="dot" aria-hidden="true"></span>
      Waiting for GitHub. The code works for {Math.round(code.expires_in / 60)} minutes.
    </p>
  </div>
{:else}
  <button
    class={["btn", "start", { primary }]}
    onclick={() => signIn.start(onsigned)}
    disabled={signIn.asking}
  >
    {signIn.asking ? "Asking GitHub…" : "Sign in with GitHub"}
  </button>
  {#if signIn.error}<p class="error" role="alert">{signIn.error}</p>{/if}
{/if}

<style>
  .code {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    max-width: 440px;
    padding: 16px 18px;
    border-radius: 10px;
    background: var(--surface-2);
  }

  .code > p:first-child {
    color: var(--ink-2);
  }

  .digits {
    font-size: 28px;
    font-weight: 500;
    letter-spacing: 0.08em;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .waiting {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: var(--cell-radius);
    background: var(--seed);
    animation: pulse 1.4s ease-in-out infinite;
  }

  .done {
    color: var(--ink-2);
  }

  .done strong {
    color: var(--ink);
    font-weight: 650;
  }

  .error {
    margin-top: 8px;
    color: var(--danger);
    font-size: var(--fs-sm);
  }

  @keyframes pulse {
    50% {
      opacity: 0.25;
    }
  }

  @media (max-width: 600px) {
    .actions {
      width: 100%;
    }

    .start,
    .actions .btn {
      width: 100%;
      justify-content: center;
    }
  }
</style>
