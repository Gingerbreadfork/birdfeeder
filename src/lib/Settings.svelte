<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import Bird from "./Bird.svelte";
  import TokenForm from "./TokenForm.svelte";
  import { api, open } from "./api";
  import { clock, num } from "./format";
  import { watcher } from "./state.svelte";
  import type { AuthInfo, NotifyKinds, Settings } from "./types";

  const intervals = [1, 2, 5, 10, 15, 30, 60, 120, 180, 360, 720, 1440];

  function every(minutes: number): string {
    if (minutes === 1) return "minute";
    if (minutes < 60) return `${minutes} minutes`;
    if (minutes === 60) return "hour";
    if (minutes === 1440) return "day";
    return `${minutes / 60} hours`;
  }
  const notifyKinds: { key: keyof NotifyKinds; label: string }[] = [
    { key: "star", label: "New stars" },
    { key: "issue", label: "New issues" },
    { key: "pr", label: "New pull requests" },
    { key: "fork", label: "New forks" },
    { key: "follower", label: "New followers" },
    { key: "inbox", label: "Mentions, replies and review requests" },
    { key: "release", label: "Releases published by someone else" },
    { key: "downloads", label: "Release downloads" },
    { key: "milestone", label: "Milestones, records and the life list" },
    { key: "traffic", label: "Busy weeks, crowds and Hacker News" },
  ];
  const sources = {
    saved: "a token you saved",
    gh_cli: "the GitHub CLI's login",
    env: "a token from the environment",
  };

  let settings = $state<Settings | null>(null);
  let auth = $state<AuthInfo | null>(null);
  let patterns = $state("");
  let error = $state<string | null>(null);
  let version = $state("");
  let saved = $state(false);
  let fade: ReturnType<typeof setTimeout> | undefined;

  void getVersion().then((v) => (version = v));

  void api.settings().then((loaded) => {
    settings = loaded;
    patterns = loaded.ignore_assets.join("\n");
  });
  const loadAuth = () => api.authInfo().then((info) => (auth = info));
  if (!watcher.status.demo) void loadAuth();

  async function save() {
    if (!settings) return;
    error = null;
    try {
      await api.saveSettings($state.snapshot(settings));
      watcher.applySettings($state.snapshot(settings));
      saved = true;
      clearTimeout(fade);
      fade = setTimeout(() => (saved = false), 1600);
    } catch (reason) {
      error = String(reason);
      settings = await api.settings();
    }
  }

  async function savePatterns() {
    if (!settings) return;
    settings.ignore_assets = patterns.split("\n").map((p) => p.trim()).filter(Boolean);
    await save();
    await watcher.refresh();
  }

  async function removeToken() {
    await api.clearToken();
    await loadAuth();
  }

  let checking = $state(false);

  async function checkNow() {
    checking = true;
    await watcher.checkForUpdate();
    checking = false;
  }

  const updateStatus = $derived.by(() => {
    if (watcher.updateProgress !== null) return "Downloading and installing…";
    if (watcher.updateError) return `The last check failed: ${watcher.updateError}`;
    if (watcher.update) return watcher.update.notes.split("\n").find((line) => line.trim()) ?? "";
    if (watcher.updateCheckedAt) {
      return `Up to date, checked at ${clock(new Date(watcher.updateCheckedAt).toISOString())}.`;
    }
    return "";
  });

  const budget = $derived(
    Object.entries(watcher.status.rates)
      .filter(([name]) => name === "core" || name === "graphql")
      .map(([name, r]) => `${num(r.remaining)} of ${num(r.limit)} ${name === "core" ? "REST" : "GraphQL"} requests`)
      .join(", "),
  );
</script>

{#if settings}
  <p class="saved" role="status">{#if saved}<span>Saved</span>{/if}</p>

  <section class="panel">
    <h2>Account</h2>
    {#if watcher.status.demo}
      <p>
        You are looking at sample data. Start Birdfeeder without
        <span class="mono">--demo</span> to watch your own account.
      </p>
    {:else if watcher.status.token_source}
      <p>
        Signed in
        {#if watcher.login}as <strong>{watcher.login}</strong>{/if}
        with {sources[watcher.status.token_source]}.
      </p>
    {:else}
      <p>Not connected to GitHub.</p>
    {/if}
    {#if !watcher.status.demo}
      {#if auth?.has_saved_token}
        <button class="btn" onclick={removeToken}>
          Remove saved token{auth.has_gh_cli ? " and use the GitHub CLI" : ""}
        </button>
      {:else}
        <h3>Use a personal access token instead</h3>
        <TokenForm onsaved={loadAuth} />
      {/if}
    {/if}
    {#if budget}<p class="hint">Left this hour: {budget}.</p>{/if}
    {#if watcher.status.warnings.length}
      <h3>The last check could not read everything</h3>
      <ul class="warnings selectable">
        {#each watcher.status.warnings as warning, i (i)}<li>{warning}</li>{/each}
      </ul>
    {/if}
  </section>

  <section class="panel">
    <h2>What to watch</h2>
    <label class="row">
      <span>Check GitHub every</span>
      <select class="field" bind:value={settings.poll_minutes} onchange={save}>
        {#each intervals as minutes (minutes)}
          <option value={minutes}>{every(minutes)}</option>
        {/each}
      </select>
    </label>
    <label class="row">
      <input type="checkbox" class="switch" bind:checked={settings.include_archived} onchange={save} />
      <span>Archived repositories</span>
    </label>
    <label class="row">
      <input type="checkbox" class="switch" bind:checked={settings.include_forks} onchange={save} />
      <span>Your forks of other people's repositories</span>
    </label>
    <label class="row">
      <input type="checkbox" class="switch" bind:checked={settings.include_org_repos} onchange={save} />
      <span>Repositories of organisations you belong to</span>
    </label>
    <label class="row">
      <input type="checkbox" class="switch" bind:checked={settings.watch_inbox} onchange={save} />
      <span>
        Your GitHub inbox
        <small>Mentions, replies, review requests and alerts, from any repository.</small>
      </span>
    </label>
    <label class="row">
      <input type="checkbox" class="switch" bind:checked={settings.watch_hacker_news} onchange={save} />
      <span>
        Hacker News
        <small>
          Looks for stories that link to your repositories. This sends your username to Hacker
          News' search, the only time Birdfeeder talks to anything but GitHub.
        </small>
      </span>
    </label>
    <label class="row">
      <input type="checkbox" class="switch" bind:checked={settings.track_traffic} onchange={save} />
      <span>
        Visitor traffic
        <small>Views, clones and referrers, read at most every three hours.</small>
      </span>
    </label>
  </section>

  <section class="panel">
    <h2>Notifications</h2>
    <label class="row">
      <input type="checkbox" class="switch" bind:checked={settings.notifications} onchange={save} />
      <span>Show a desktop notification when something new happens</span>
    </label>
    <fieldset disabled={!settings.notifications}>
      <legend>Notify about</legend>
      {#each notifyKinds as kind (kind.key)}
        <label class="check">
          <input type="checkbox" bind:checked={settings.notify[kind.key]} onchange={save} />
          {kind.label}
        </label>
      {/each}
    </fieldset>
    <label class="row">
      <input type="checkbox" class="switch" bind:checked={settings.celebrate} onchange={save} />
      <span>
        Celebrate milestones and rare birds in the window
        <small>The bird and a scatter of seed, once, when something special lands.</small>
      </span>
    </label>
  </section>

  <section class="panel">
    <h2>Download counts</h2>
    <label class="stack">
      <span>
        Leave out files whose name matches
        <small>
          One pattern per line, where <span class="mono">*</span> matches anything. Signature
          files and updater manifests are fetched by machines, so they inflate the totals.
        </small>
      </span>
      <textarea
        class="field mono"
        rows="6"
        spellcheck="false"
        bind:value={patterns}
        onchange={savePatterns}
      ></textarea>
    </label>
  </section>

  <section class="panel">
    <h2>Running in the background</h2>
    <label class="row">
      <input type="checkbox" class="switch" bind:checked={settings.keep_running} onchange={save} />
      <span>
        Keep watching after the window is closed
        <small>Open the app again, or use its tray icon, to bring the window back.</small>
      </span>
    </label>
    {#if !watcher.status.demo}
      <label class="row">
        <input type="checkbox" class="switch" bind:checked={settings.autostart} onchange={save} />
        <span>Start when you log in, without opening a window</span>
      </label>
    {/if}
    <button class="btn" onclick={() => api.quit()}>Quit Birdfeeder</button>
  </section>

  <section class="panel">
    <h2>Updates</h2>
    {#if watcher.status.demo}
      <p>Birdfeeder does not look for updates while it is showing sample data.</p>
    {:else}
      <label class="row">
        <input type="checkbox" class="switch" bind:checked={settings.check_updates} onchange={save} />
        <span>
          Check for updates
          <small>At launch and every few hours, from this project's releases on GitHub.</small>
        </span>
      </label>
      <label class="row">
        <input
          type="checkbox"
          class="switch"
          bind:checked={settings.install_updates}
          onchange={save}
          disabled={!settings.check_updates}
        />
        <span>
          Install updates automatically
          <small>
            Installs a new release when it is found, then restarts. A .deb or .rpm update asks for
            your password, so it waits until the window is open.
          </small>
        </span>
      </label>
      <div class="update">
        <p>
          {#if watcher.update}
            <strong>Birdfeeder {watcher.update.version} is available.</strong>
          {:else}
            You have Birdfeeder {version}.
          {/if}
          {#if updateStatus}<span class="hint">{updateStatus}</span>{/if}
        </p>
        {#if watcher.update}
          <button
            class="btn primary"
            disabled={watcher.updateProgress !== null}
            onclick={() => watcher.installUpdate()}
          >
            {watcher.updateProgress === null
              ? "Install and restart"
              : watcher.updateProgress > 0
                ? `Downloading ${Math.round(watcher.updateProgress * 100)}%`
                : "Installing…"}
          </button>
        {:else}
          <button class="btn" onclick={checkNow} disabled={checking}>
            {checking ? "Checking…" : "Check now"}
          </button>
        {/if}
      </div>
    {/if}
  </section>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  <footer>
    <Bird size={44} />
    <p>
      <strong>Birdfeeder</strong> {version}<br />
      <button class="link" onclick={() => open("https://github.com/Gingerbreadfork/birdfeeder")}>
        Source and releases on GitHub
      </button>
    </p>
  </footer>
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    max-width: 680px;
    padding: 18px 20px 20px;
  }

  h3 {
    margin-top: 4px;
    font-size: var(--fs-sm);
    font-weight: 600;
  }

  /* Takes no room of its own, and stays in view while the page scrolls. */
  .saved {
    position: sticky;
    top: 0;
    z-index: 2;
    align-self: flex-end;
    height: 0;
    margin-bottom: -16px;
  }

  .saved span {
    display: inline-block;
    padding: 3px 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }

  .update {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    width: 100%;
  }

  .update p {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .update .hint {
    overflow-wrap: anywhere;
  }

  .warnings {
    margin: 0;
    padding-left: 18px;
    color: var(--ink-2);
    font-size: var(--fs-sm);
  }

  p {
    color: var(--ink-2);
  }

  p strong {
    color: var(--ink);
    font-weight: 650;
  }

  .hint {
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }

  .row {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    cursor: pointer;
  }

  .row:has(select) {
    align-items: center;
  }

  .row > span,
  .stack > span {
    display: flex;
    flex-direction: column;
  }

  small {
    max-width: 58ch;
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }

  fieldset {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
    gap: 8px 20px;
    width: 100%;
    margin: 0;
    padding: 0;
    border: 0;
  }

  fieldset:disabled {
    opacity: 0.5;
  }

  legend {
    padding: 0 0 8px;
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
    cursor: pointer;
  }

  .check input {
    margin: 0;
    accent-color: var(--accent);
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
  }

  textarea {
    width: 100%;
    max-width: 360px;
  }

  .error {
    color: var(--danger);
  }

  footer {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 4px 20px 0;
    font-size: var(--fs-sm);
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
  }

  .link:hover {
    text-decoration: underline;
  }
</style>
