<script lang="ts">
  import Connect from "./lib/Connect.svelte";
  import Downloads from "./lib/Downloads.svelte";
  import Celebration from "./lib/Celebration.svelte";
  import Feed from "./lib/Feed.svelte";
  import LifeList from "./lib/LifeList.svelte";
  import Overview from "./lib/Overview.svelte";
  import PullIndicator from "./lib/PullIndicator.svelte";
  import Repos from "./lib/Repos.svelte";
  import Settings from "./lib/Settings.svelte";
  import SyncBar from "./lib/SyncBar.svelte";
  import TabBar from "./lib/TabBar.svelte";
  import UpdateNotice from "./lib/UpdateNotice.svelte";
  import { api } from "./lib/api";
  import { handheld } from "./lib/platform";
  import { pullToRefresh } from "./lib/pull.svelte";
  import { narrow, watcher, type Page } from "./lib/state.svelte";

  const tabs: { id: Page; label: string; short?: string }[] = [
    { id: "activity", label: "Activity" },
    { id: "overview", label: "Overview" },
    { id: "repos", label: "Repositories", short: "Repos" },
    { id: "downloads", label: "Downloads" },
    { id: "lifelist", label: "Life list" },
    { id: "settings", label: "Settings" },
  ];

  /** On a phone the first tab is the one the app opens on. */
  const thumbTabs = [tabs[1], tabs[0], ...tabs.slice(2, -1)];

  /** The activity list has its own tab only when there is no room for the side rail. */
  const page = $derived(!narrow.current && watcher.page === "activity" ? "overview" : watcher.page);
  /** A narrow window reaches Settings from the bar above the tabs instead. */
  const shownTabs = $derived(
    handheld.current ? thumbTabs : narrow.current ? tabs.slice(0, -1) : tabs.slice(1),
  );
  const disconnected = $derived(
    watcher.status.phase === "signed_out" ||
      watcher.status.error_kind === "auth" ||
      watcher.status.error_kind === "account",
  );

  /** Steps back one level: out of a celebration, a repository page, then the filters. */
  function back(): boolean {
    if (watcher.celebration) watcher.celebration = null;
    else if (watcher.detail) watcher.closeRepo();
    else if (watcher.filtered) watcher.clearFilters();
    else return false;
    return true;
  }

  let content = $state<HTMLElement>();

  /** The window's full height at its current width. */
  let tall = { width: innerWidth, height: innerHeight };
  /** Whether a phone's keyboard is up, which it shows by making the window shorter. */
  let keyboard = $state(false);

  /** Hides the tab bar while the keyboard is up, and keeps the field being typed in in view. */
  function resized() {
    if (innerWidth !== tall.width || innerHeight > tall.height) {
      tall = { width: innerWidth, height: innerHeight };
    }
    keyboard = handheld.current && innerHeight < tall.height - 120;
    const field = document.activeElement;
    if (keyboard && field?.matches("input, textarea")) {
      requestAnimationFrame(() => field.scrollIntoView({ block: "center" }));
    }
  }

  /** Choosing the tab already open goes back to the top of it. */
  function pick(id: Page) {
    if (id === page && !watcher.detail) {
      content?.querySelector(".scroll")?.scrollTo({ top: 0, behavior: "smooth" });
    } else {
      watcher.go(id);
    }
  }

  // The back gesture follows the screen.
  $effect(() => {
    void [watcher.page, watcher.detail];
    watcher.keepBackInStep();
  });

  function shortcut(e: KeyboardEvent) {
    if (e.key === "Escape" && !e.defaultPrevented) {
      if (back()) e.preventDefault();
      return;
    }
    if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
    const tab = shownTabs[Number(e.key) - 1];
    if (e.key === "q") {
      e.preventDefault();
      void api.quit();
    } else if (e.key === "r") {
      e.preventDefault();
      watcher.syncNow();
    } else if (tab) {
      e.preventDefault();
      watcher.go(tab.id);
    }
  }

  function seen() {
    const visible = document.visibilityState === "visible";
    watcher.shown(visible);
    if (visible) watcher.arrive();
    else watcher.leave();
  }
</script>

<svelte:window onkeydown={shortcut} onresize={resized} onblur={() => watcher.leave()} onfocus={() => watcher.arrive()} />
<svelte:document onvisibilitychange={seen} />

<Celebration />

<div class={["shell", { narrow: narrow.current, handheld: handheld.current }]}>
  {#if !narrow.current}
    <aside>
      <SyncBar />
      <Feed />
    </aside>
  {/if}

  <main>
    {#if narrow.current}<SyncBar />{/if}
    {#if !handheld.current}
      <nav aria-label="Sections">
        {#each shownTabs as tab (tab.id)}
          <button aria-current={page === tab.id ? "page" : undefined} onclick={() => watcher.go(tab.id)}>
            {narrow.current ? (tab.short ?? tab.label) : tab.label}
            {#if tab.id === "activity" && watcher.status.unread > 0}
              <span class="count">{watcher.status.unread}</span>
            {/if}
          </button>
        {/each}
      </nav>
    {/if}
    <UpdateNotice />

    <div class="content" bind:this={content}>
      <PullIndicator />
      {#if page === "activity" && !disconnected}
        <div class="rail"><Feed /></div>
      {:else}
        <!-- Keyed so that each screen starts scrolled to its top. -->
        {#key `${page}:${watcher.detail?.repo.id ?? ""}`}
          <div class="page scroll" {@attach pullToRefresh}>
            {#if disconnected && page !== "settings"}
              <Connect />
            {:else if page === "overview"}
              <Overview />
            {:else if page === "repos"}
              <Repos />
            {:else if page === "downloads"}
              <Downloads />
            {:else if page === "lifelist"}
              <LifeList />
            {:else if page === "settings"}
              <Settings />
            {/if}
          </div>
        {/key}
      {/if}
    </div>

    {#if handheld.current && !keyboard}
      <TabBar tabs={shownTabs.map((t) => ({ id: t.id, label: t.short ?? t.label }))} current={page} unread={watcher.status.unread} onpick={pick} />
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: minmax(340px, 400px) 1fr;
    height: 100%;
    padding: var(--safe-top) var(--safe-right) 0 var(--safe-left);
  }

  /* The tab bar reaches the screen's edges and pads itself. */
  .shell.handheld {
    padding-right: 0;
    padding-left: 0;
  }

  .handheld main > :not(:global(.bar)) {
    margin-right: var(--safe-right);
    margin-left: var(--safe-left);
  }

  .shell.narrow {
    grid-template-columns: 1fr;
  }

  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--line);
  }

  main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  nav {
    display: flex;
    gap: 2px;
    padding: 12px 24px 0;
    border-bottom: 1px solid var(--line);
    overflow-x: auto;
    scrollbar-width: none;
  }

  .narrow nav {
    padding: 0 8px;
  }

  .narrow nav button {
    padding: 8px 8px 10px;
  }

  nav button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-bottom: -1px;
    padding: 8px 12px 10px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--ink-2);
    font-weight: 550;
    font-stretch: 108%;
    white-space: nowrap;
  }

  @media (hover: hover) {
    nav button:hover {
      color: var(--ink);
    }
  }

  nav button[aria-current="page"] {
    border-bottom-color: var(--ink);
    color: var(--ink);
    font-weight: 680;
  }

  /* Settings sits apart from the screens that show data. */
  .shell:not(.narrow) nav button:last-child {
    margin-left: auto;
  }

  .count {
    padding: 0 6px;
    border-radius: 9px;
    background: var(--seed);
    color: #17211b;
    font-size: 11px;
    font-weight: 650;
    line-height: 17px;
  }

  .content {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    flex: 1;
    min-height: 0;
    padding: 20px 24px calc(32px + var(--safe-bottom));
  }

  .narrow .page {
    padding: 14px 12px calc(24px + var(--safe-bottom));
  }

  .handheld .page {
    padding-bottom: 24px;
  }

  .rail {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    padding-top: 12px;
  }

  /* The tab already names this screen. */
  .rail :global(h2) {
    display: none;
  }

  .rail :global(.head .unread) {
    margin-right: auto;
  }
</style>
