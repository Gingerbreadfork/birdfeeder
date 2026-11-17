<script lang="ts">
  import Bird from "./Bird.svelte";
  import { clock } from "./format";
  import { describe, isSpecial, type Part } from "./kinds";
  import { watcher } from "./state.svelte";

  const HOUR = 3_600_000;

  const recap = $derived(watcher.recap);

  function plain(parts: Part[]): string {
    return parts
      .map((p) => (typeof p === "string" ? p : "strong" in p ? p.strong : "tag" in p ? p.tag : p.code))
      .join("");
  }

  /** The one or two most notable arrivals by name, and a count of the rest. */
  const sentence = $derived.by(() => {
    if (!recap) return "";
    const notable = [...recap.events].sort((a, b) => Number(isSpecial(b)) - Number(isSpecial(a)));
    const named = notable.slice(0, 2).map((e) => plain(describe(e, watcher.login).parts));
    const rest = recap.events.length - named.length;
    if (rest > 0) return `${named.join(", ")} and ${rest} more.`;
    return `${named.join(" and ")}.`;
  });

  const title = $derived(
    recap?.away && watcher.now - recap.away < HOUR
      ? `Since ${clock(new Date(recap.away).toISOString())}`
      : "While you were away",
  );
</script>

{#if recap}
  <section class="recap">
    <span class="bird"><Bird size={52} /></span>
    <div>
      <h2>{title}</h2>
      <p>{sentence[0].toUpperCase()}{sentence.slice(1)}</p>
    </div>
    <button class="btn" onclick={() => watcher.showUnread()}>Show unread</button>
  </section>
{/if}

<style>
  .recap {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 14px 20px;
    border: 1px solid color-mix(in srgb, var(--seed) 60%, var(--line));
    border-radius: 10px;
    background: var(--surface);
  }

  div {
    flex: 1;
    min-width: 0;
  }

  p {
    color: var(--ink-2);
  }

  @media (max-width: 560px) {
    .recap {
      flex-direction: column;
      align-items: flex-start;
      gap: 10px;
      padding: 14px 16px;
    }

    .bird {
      display: none;
    }
  }
</style>
