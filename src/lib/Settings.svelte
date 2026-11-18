<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import Bird from "./Bird.svelte";
  import SignIn from "./SignIn.svelte";
  import TokenForm from "./TokenForm.svelte";
  import { api, open } from "./api";
  import { clock, num } from "./format";
  import { mobile } from "./platform";
  import { watcher } from "./state.svelte";
  import type { AuthInfo, NotifyKinds, Settings, Theme } from "./types";

  const intervals = [1, 2, 5, 10, 15, 30, 60, 120, 180, 360, 720, 1440];

  const themes: { id: Theme; label: string }[] = [
    { id: "system", label: "System" },
    { id: "light", label: "Light" },
    { id: "dark", label: "Dark" },
  ];

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

  const signOut = $derived.by(() => {
    if (mobile) return "Sign out";
    const what = auth?.signed_in_here ? "Sign out" : "Remove saved token";
    return auth?.has_gh_cli ? `${what} and use the GitHub CLI` : what;
  });
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
    {#if watcher.status.demo && mobile}
      <p>You are looking at sample data. This copy was built to show it.</p>
    {:else if watcher.status.demo}
      <p>
        You are looking at sample data. Start Birdfeeder without
        <span class="mono">--demo</span> to watch your own account.
      </p>
    {:else if watcher.status.token_source}
      <p>
        Signed in
        {#if watcher.login}as <strong>{watcher.login}</strong>{/if}
        {watcher.status.token_source === "saved" && auth?.signed_in_here
          ? "through GitHub"
          : `with ${sources[watcher.status.token_source]}`}.
      </p>
    {:else}
      <p>Not connected to GitHub.</p>
    {/if}
    {#if !watcher.status.demo}
      {#if auth?.has_saved_token}
        <button class="btn" onclick={removeToken}>{signOut}</button>
        {#if auth.signed_in_here}
          <p class="hint">
            Signing out forgets the token here. GitHub lists Birdfeeder in your settings under
            Applications, where its access can be revoked.
          </p>
        {/if}
      {:else}
        <SignIn onsigned={loadAuth} primary={!watcher.status.token_source} />
        <h3>Or paste a personal access token</h3>
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
    <h2>Appearance</h2>
    <div class="theme" role="radiogroup" aria-label="Theme">
      {#each themes as theme (theme.id)}
        <label class={{ on: settings.theme === theme.id }}>
          <input type="radio" name="theme" value={theme.id} bind:group={settings.theme} onchange={save} />
          {theme.label}
        </label>
      {/each}
    </div>
    <p class="hint">
      {mobile ? "System follows the phone's dark theme." : "System follows your desktop's light or dark style."}
    </p>
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
      <span>Show a {mobile ? "" : "desktop "}notification when something new happens</span>
    </label>
    {#if mobile && settings.notifications && watcher.mayNotify === false}
      <div class="blocked">
        {#if watcher.notifyRefused}
          <p>
            Android is not letting Birdfeeder show notifications. They can be turned on in the
            phone's settings, under Apps, Birdfeeder, Notifications.
          </p>
        {:else}
          <p>Android is not letting Birdfeeder show notifications yet.</p>
          <button class="btn" onclick={() => watcher.allowNotifications()}>Allow notifications</button>
        {/if}
      </div>
    {/if}
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
    {#if mobile}
      <label class="row">
        <input type="checkbox" class="switch" bind:checked={settings.keep_running} onchange={save} />
        <span>
          Keep checking while Birdfeeder is closed
          <small>
            Android decides when: about every 15 minutes at most, less often when the battery is
            low, and only with a connection.
          </small>
        </span>
      </label>
    {:else}
      <label class="row">
        <input type="checkbox" class="switch" bind:checked={settings.keep_running} onchange={save} />
        <span>
          Keep watching after the window is closed
          <small>Open the app again, or use its tray icon, to bring the window back.</small>
        </span>
      </label>
    {/if}
    {#if !watcher.status.demo && !mobile}
      <label class="row">
        <input type="checkbox" class="switch" bind:checked={settings.autostart} onchange={save} />
        <span>Start when you log in, without opening a window</span>
      </label>
    {/if}
    {#if !mobile}
      <button class="btn" onclick={() => api.quit()}>Quit Birdfeeder</button>
    {/if}
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
      {#if !mobile}
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
              Installs a new release when it is found, then restarts. A .deb or .rpm update asks
              for your password, so it waits until the window is open.
            </small>
          </span>
        </label>
      {/if}
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
            {!watcher.update.install
              ? "Download"
              : watcher.updateProgress === null
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
    font-size: var(--fs-row);
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

  .theme {
    display: inline-flex;
    padding: 3px;
    border: 1px solid var(--line-strong);
    border-radius: calc(var(--radius) + 3px);
    background: var(--bg);
  }

  .theme label {
    position: relative;
    display: inline-flex;
    align-items: center;
    height: calc(var(--tap) - 6px);
    padding: 0 14px;
    border-radius: var(--radius);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    font-weight: 550;
    cursor: pointer;
  }

  .theme label.on {
    background: var(--surface);
    box-shadow: 0 1px 2px #0000002a;
    color: var(--ink);
  }

  .theme input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .theme label:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  @media (max-width: 600px) {
    .theme {
      display: flex;
      width: 100%;
    }

    .theme label {
      flex: 1;
      justify-content: center;
    }
  }

  .blocked {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--surface-2);
  }

  @media (pointer: coarse) {
    .panel {
      gap: 16px;
    }

    .check {
      min-height: 36px;
      gap: 12px;
    }

    .check input {
      width: 20px;
      height: 20px;
    }
  }

  @media (max-width: 600px) {
    .panel {
      padding: 16px 16px 18px;
    }

    .update {
      flex-direction: column;
      align-items: flex-start;
    }

    textarea {
      max-width: none;
    }

    footer {
      padding: 4px 16px 0;
    }
  }

  @media (hover: hover) {
    .link:hover {
      text-decoration: underline;
    }
  }
</style>
