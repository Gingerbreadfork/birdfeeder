<script lang="ts">
  import Bird from "./Bird.svelte";
  import { READY, pull } from "./pull.svelte";

  const R = 17;
  const LENGTH = 2 * Math.PI * R;

  const shown = $derived(pull.distance > 0 || pull.busy);
  const progress = $derived(Math.min(1, pull.distance / READY));
</script>

<div
  class={["pull", { shown, holding: pull.holding }]}
  style:--d="{pull.distance}px"
  role="status"
  aria-live="polite"
>
  <span class={["disc", { ready: pull.ready, busy: pull.busy }]}>
    <svg viewBox="0 0 40 40" width="40" height="40" aria-hidden="true">
      <circle class="ring" cx="20" cy="20" r={R} stroke-dasharray="{progress * LENGTH} {LENGTH}" />
    </svg>
    <span class="bird"><Bird size={20} perch={false} /></span>
  </span>
  {#if pull.busy}<span class="label">Checking GitHub</span>{/if}
</div>

<style>
  .pull {
    position: absolute;
    top: 0;
    left: 50%;
    z-index: 5;
    pointer-events: none;
    opacity: 0;
    transform: translate(-50%, calc(var(--d) - 48px));
    transition:
      transform 0.25s cubic-bezier(0.2, 0.9, 0.3, 1),
      opacity 0.2s;
  }

  .shown {
    opacity: 1;
  }

  .holding {
    transition: opacity 0.2s;
  }

  .disc {
    position: relative;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: var(--surface);
    box-shadow:
      0 0 0 1px var(--line),
      0 4px 14px #0000002a;
  }

  svg {
    position: absolute;
    inset: 0;
    transform: rotate(-90deg);
  }

  .ring {
    fill: none;
    stroke: var(--ink-3);
    stroke-width: 2.5;
    stroke-linecap: round;
    transition: stroke 0.15s;
  }

  .ready .ring {
    stroke: var(--seed);
  }

  .busy svg {
    animation: turn 0.9s linear infinite;
  }

  .busy .ring {
    stroke-dasharray: 30 200 !important;
  }

  .busy .bird {
    animation: peck 0.6s ease-in-out infinite;
  }

  .bird {
    display: grid;
    margin-left: -1px;
  }

  .label {
    position: absolute;
    top: 100%;
    left: 50%;
    margin-top: 6px;
    padding: 2px 9px;
    border-radius: 10px;
    background: var(--surface);
    box-shadow: 0 0 0 1px var(--line);
    transform: translateX(-50%);
    color: var(--ink-2);
    font-size: var(--fs-xs);
    white-space: nowrap;
  }

  @keyframes turn {
    to {
      transform: rotate(270deg);
    }
  }

  @keyframes peck {
    50% {
      transform: translateY(-3px);
    }
  }
</style>
