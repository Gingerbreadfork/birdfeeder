<script lang="ts">
  import Bird from "./Bird.svelte";
  import { describe, isRare, type Part } from "./kinds";
  import { watcher } from "./state.svelte";

  const SEEDS = 26;
  const COLORS = ["--k-star", "--k-follower", "--k-issue", "--k-pr", "--k-fork", "--k-downloads", "--k-release", "--seed"];
  /** How long the celebration stays, and how long it lingers after the pointer leaves it. */
  const STAY = 7000;
  const LINGER = 2500;

  const event = $derived(watcher.celebration);
  const said = $derived(event ? describe(event, watcher.login) : null);

  /** Where each seed is thrown. Worked out once so they do not jump in flight. */
  const seeds = Array.from({ length: SEEDS }, (_, i) => {
    const angle = Math.PI + (i / (SEEDS - 1)) * Math.PI;
    const reach = 70 + Math.random() * 150;
    return {
      x: Math.cos(angle) * reach,
      y: Math.sin(angle) * reach,
      spin: Math.round(Math.random() * 300 - 150),
      delay: Math.round(Math.random() * 120),
      color: COLORS[i % COLORS.length],
    };
  });

  let timer: ReturnType<typeof setTimeout> | undefined;

  function dismiss() {
    clearTimeout(timer);
    watcher.celebration = null;
  }

  function wait(ms: number) {
    clearTimeout(timer);
    timer = setTimeout(dismiss, ms);
  }

  /** Starts the clock when a celebration appears. */
  function hold() {
    wait(STAY);
    return () => clearTimeout(timer);
  }

  /** Goes to where the celebrated thing can be seen in full. */
  function follow() {
    const e = event;
    dismiss();
    if (!e) return;
    if (e.kind === "sighting" || e.meta?.sighting || isRare(e)) watcher.go("lifelist");
    else if (e.kind === "milestone" && e.repo_id) void watcher.openRepo(e.repo_id);
    else watcher.go("overview");
  }
</script>

{#snippet text(parts: Part[])}
  {#each parts as part, i (i)}
    {#if typeof part === "string"}{part}{:else if "strong" in part}<strong>{part.strong}</strong
      >{:else if "tag" in part}{part.tag}{:else}{part.code}{/if}
  {/each}
{/snippet}

{#if event && said}
  {#key event.id}
    <div class="stage">
      <div class="burst" aria-hidden="true">
        {#each seeds as seed, i (i)}
          <span
            style:--x="{seed.x}px"
            style:--y="{seed.y}px"
            style:--spin="{seed.spin}deg"
            style:--delay="{seed.delay}ms"
            style:background="var({seed.color})"
          ></span>
        {/each}
      </div>
      <div
        class="toast"
        role="status"
        onpointerenter={() => clearTimeout(timer)}
        onpointerleave={() => wait(LINGER)}
        {@attach hold}
      >
        <button class="main" onclick={follow}>
          <span class="bird"><Bird size={52} /></span>
          <span class="words">
            <span class="lead">{@render text(said.parts)}</span>
            {#if said.note}<span class="note">{said.note}</span>{/if}
            {#if said.detail}<span class="detail">{@render text(said.detail)}</span>{/if}
          </span>
        </button>
        <button class="close" onclick={dismiss} aria-label="Dismiss">
          <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
            <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
          </svg>
        </button>
      </div>
    </div>
  {/key}
{/if}

<style>
  .stage {
    position: fixed;
    right: calc(20px + var(--safe-right));
    bottom: calc(20px + var(--dock));
    z-index: 50;
    max-width: calc(100vw - 40px);
  }

  /* A phone gives the whole width to it, just above the tab bar. */
  @media (max-width: 560px) {
    .stage {
      right: 12px;
      bottom: calc(10px + var(--dock));
      left: 12px;
      max-width: none;
    }

    .main {
      flex: 1;
      max-width: none;
    }
  }

  .toast {
    position: relative;
    display: flex;
    align-items: flex-start;
    border: 1px solid color-mix(in srgb, var(--seed) 70%, var(--line));
    border-radius: 14px;
    background: var(--surface);
    box-shadow: 0 14px 40px #00000038;
    animation: rise 0.45s cubic-bezier(0.2, 1.3, 0.4, 1) both;
  }

  .main {
    display: flex;
    align-items: center;
    gap: 14px;
    max-width: 440px;
    padding: 12px 6px 12px 14px;
    border: 0;
    border-radius: 14px;
    background: none;
    text-align: left;
  }

  .close {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    margin: 6px 6px 0 0;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--ink-3);
  }

  @media (hover: hover) {
    .close:hover {
      background: var(--accent-wash);
      color: var(--ink);
    }
  }

  .bird {
    animation: hop 0.5s 0.35s ease-out 2;
  }

  .words {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .lead {
    font-size: var(--fs-lg);
    font-weight: 620;
    font-stretch: 108%;
    line-height: 1.25;
  }

  .note {
    color: var(--seed-ink);
    font-size: var(--fs-sm);
    font-weight: 650;
  }

  .detail {
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }

  .burst {
    position: absolute;
    top: 30px;
    left: 44px;
  }

  .burst span {
    position: absolute;
    width: 9px;
    height: 9px;
    border-radius: var(--cell-radius);
    opacity: 0;
    animation: scatter 1.5s var(--delay) cubic-bezier(0.1, 0.7, 0.3, 1) both;
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(20px) scale(0.96);
    }
  }

  @keyframes hop {
    50% {
      transform: translateY(-9px);
    }
  }

  @keyframes scatter {
    0% {
      opacity: 1;
      transform: translate(0, 0) rotate(0);
    }
    70% {
      opacity: 1;
    }
    100% {
      opacity: 0;
      transform: translate(var(--x), calc(var(--y) + 40px)) rotate(var(--spin));
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .burst {
      display: none;
    }
  }
</style>
