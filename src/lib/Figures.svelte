<script lang="ts">
  import Count from "./Count.svelte";
  import { num, signed } from "./format";

  export interface Figure {
    label: string;
    value: number;
    /** The change over `period`, or null when it is not known yet. */
    change?: number | null;
    period?: string;
    /** Whether the change is a net gain or loss, rather than a count of new items. */
    net?: boolean;
    /** Shows where the number comes from when it is chosen. */
    onpick?: () => void;
  }

  let { figures }: { figures: Figure[] } = $props();
</script>

<div class="clip">
  <dl class="figures">
  {#each figures as f (f.label)}
    <div>
      <dt>{f.label}</dt>
      <dd class="value" title={num(f.value)}>
        {#if f.onpick}
          <button onclick={f.onpick}><Count value={f.value} /></button>
        {:else}
          <Count value={f.value} />
        {/if}
      </dd>
      {#if f.change !== undefined}
        <dd class="change">
          {#if f.change === null}
            <span class="muted">From today</span>
          {:else if f.change === 0}
            <span class="muted">None {f.period}</span>
          {:else if f.net}
            <span class={f.change > 0 ? "up" : "down"}>{signed(f.change)}</span> {f.period}
          {:else}
            {num(f.change)} new {f.period}
          {/if}
        </dd>
      {/if}
    </div>
  {/each}
  </dl>
</div>

<style>
  /* Every cell draws a rule on its left; the clip hides the ones at the row starts. */
  .clip {
    overflow: hidden;
  }

  .figures {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(118px, 1fr));
    gap: 18px 0;
    margin: 0 0 0 -1px;
  }

  .figures > div {
    padding: 0 16px;
    border-left: 1px solid var(--line);
  }

  dt {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    white-space: nowrap;
  }

  dd {
    margin: 0;
  }

  .value {
    font-size: var(--fs-2xl);
    font-weight: 620;
    font-stretch: 112%;
    line-height: 1.15;
    letter-spacing: -0.01em;
  }

  .value button {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    letter-spacing: inherit;
  }

  @media (hover: hover) {
    .value button:hover {
      color: var(--accent);
    }
  }

  .change {
    color: var(--ink-2);
    font-size: var(--fs-sm);
    white-space: nowrap;
  }

  .up {
    color: var(--up);
    font-weight: 600;
  }

  .down {
    color: var(--down);
    font-weight: 600;
  }
</style>
