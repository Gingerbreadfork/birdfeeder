import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { api, open } from "./api";
import { LANES, isSpecial, type Lane } from "./kinds";
import { mobile, narrow } from "./platform";
import { applyTheme } from "./theme";
import {
  RELEASE_PAGE,
  asksForPassword,
  checkForUpdate,
  relaunch,
  type UpdateHandle,
} from "./updates";
import type {
  Downloads,
  EventQuery,
  LifeList,
  Marks,
  Overview,
  RepoDetail,
  RepoRow,
  Settings,
  Status,
  WatchEvent,
} from "./types";

export type Page = "activity" | "overview" | "repos" | "downloads" | "lifelist" | "settings";

/** The screen the app opens on, and the last one the back gesture returns to. */
export const START: Page = "overview";

const PAGE_SIZE = 80;
/** The most rows the backend hands over in one answer. */
const MAX_BATCH = 500;
/** A look away shorter than this does not count as having been away. */
const BRIEF_ABSENCE = 60_000;
/** How often to look for a newer release. */
const UPDATE_EVERY = 6 * 60 * 60 * 1000;
export { narrow };

/** A check that takes longer than this is no longer waited on by a pull to refresh. */
const CHECK_PATIENCE = 30_000;
/** Remembers that the phone has asked once for leave to notify. */
const ASKED_TO_NOTIFY = "birdfeeder.asked-to-notify";

/** Days of history the activity grid can show. */
export const GRID_DAYS = 91;

/** What arrived while the window was not being looked at. */
export interface Recap {
  events: WatchEvent[];
  /** When the window was left, or null when the app has only just opened. */
  away: number | null;
}

class Watcher {
  status = $state<Status>({
    demo: false,
    phase: "syncing",
    step: null,
    last_sync_at: null,
    next_sync_at: null,
    error: null,
    error_kind: null,
    warnings: [],
    login: null,
    token_source: null,
    rates: {},
    unread: 0,
  });

  events = $state.raw<WatchEvent[]>([]);
  eventsLoaded = $state(false);
  hasMore = $state(false);
  limit = PAGE_SIZE;
  /** Counts list requests so a slow answer cannot replace a newer one. */
  private asked = 0;

  lane = $state<Lane | null>(null);
  unreadOnly = $state(false);
  day = $state<string | null>(null);
  repoFilter = $state<{ id: string; name: string } | null>(null);

  page = $state<Page>("overview");
  overview = $state.raw<Overview | null>(null);
  repos = $state.raw<RepoRow[] | null>(null);
  downloads = $state.raw<Downloads | null>(null);
  detail = $state.raw<RepoDetail | null>(null);
  lifeList = $state.raw<LifeList | null>(null);

  /** The event being celebrated on screen right now. */
  celebration = $state.raw<WatchEvent | null>(null);
  /** Whether celebrations are shown at all. Follows the setting. */
  celebrates = $state(true);
  recap = $state.raw<Recap | null>(null);
  private recapShownAt = 0;
  private awayAt: number | null = null;
  private marks: Marks = { celebrated: 0, seen: 0 };
  /** Whether a check finished while the window was out of sight. */
  private stale = false;

  /** A newer release the updater found, until it is installed. */
  update = $state.raw<UpdateHandle | null>(null);
  /** Download progress from 0 to 1 while an update installs. */
  updateProgress = $state<number | null>(null);
  updateError = $state("");
  updateCheckedAt = $state<number | null>(null);
  /** Whether the notice about this update was put away. */
  updateDismissed = $state(false);
  private updates = { check: false, install: false };
  private updateTimer: ReturnType<typeof setInterval> | undefined;
  /** Whether installing asks for a password, which should not happen out of sight. */
  private asksForPassword = false;
  /** The version an automatic install was last tried for, so a refusal is not asked again. */
  private triedVersion: string | null = null;

  /** Ticks so relative times stay current. */
  now = $state(Date.now());

  /** Whether the phone lets the app show notifications, or null before it is known. */
  mayNotify = $state<boolean | null>(null);
  /** Whether the phone was asked and said no, after which only its settings can change that. */
  notifyRefused = $state(false);
  private notifying = false;

  /** Called when the running check finishes. */
  private checked: (() => void)[] = [];

  /** History entries above the first: one for each level the back gesture can step out of. */
  private entries = 0;
  /** Set while the app itself steps back through history, so that step is not taken twice. */
  private quiet = false;

  get login(): string | null {
    return this.status.login ?? this.overview?.account?.login ?? null;
  }

  get filtered(): boolean {
    return Boolean(this.lane || this.unreadOnly || this.day || this.repoFilter);
  }

  /** Unread rows among those the list is showing. */
  get unreadShown(): number {
    return this.events.filter((e) => !e.read).length;
  }

  /** The filters in force, as the backend takes them. */
  private get query(): EventQuery {
    return {
      kinds: LANES.find((l) => l.id === this.lane)?.kinds ?? [],
      repo_id: this.repoFilter?.id ?? null,
      day: this.day,
    };
  }

  async start() {
    await listen<Status>("status", (e) => {
      const was = this.status.phase;
      this.status = e.payload;
      // Signed out, a check ends without ever being under way.
      const phase = e.payload.phase;
      if (phase !== "syncing" && (was === "syncing" || phase === "signed_out")) this.settle();
      void this.askToNotify();
    });
    addEventListener("popstate", () => this.popped());
    await listen("synced", () => {
      // A hidden window has nobody to redraw for. It catches up when it is shown.
      if (document.visibilityState === "hidden") this.stale = true;
      else void this.refresh();
    });
    setInterval(() => (this.now = Date.now()), 30_000);
    this.status = await api.status();
    this.asksForPassword = await asksForPassword();
    this.applySettings(await api.settings());
    if (mobile) {
      // Android answers null until it has been asked.
      this.mayNotify = (await isPermissionGranted()) === true;
      void this.askToNotify();
    }
    this.marks = await api.marks();
    await this.refresh();
    await this.summarise(null);
  }

  /** Takes on the settings the window itself acts on. */
  applySettings(settings: Settings) {
    applyTheme(settings.theme);
    this.celebrates = settings.celebrate;
    this.notifying = settings.notifications;
    const checkChanged = this.updates.check !== settings.check_updates;
    this.updates = { check: settings.check_updates, install: settings.install_updates };
    if (checkChanged) this.scheduleUpdateChecks();
    else void this.installIfWanted();
  }

  /** Looks for a newer release now and every few hours, while the setting is on. */
  private scheduleUpdateChecks() {
    clearInterval(this.updateTimer);
    this.updateTimer = undefined;
    // Sample data never contacts GitHub.
    if (this.status.demo || !this.updates.check) return;
    void this.checkForUpdate();
    this.updateTimer = setInterval(() => void this.checkForUpdate(), UPDATE_EVERY);
  }

  async checkForUpdate() {
    if (this.status.demo || this.updateProgress !== null) return;
    try {
      const found = await checkForUpdate();
      this.updateCheckedAt = Date.now();
      this.updateError = "";
      if (found?.version !== this.update?.version) this.updateDismissed = false;
      this.update = found;
      await this.installIfWanted();
    } catch (reason) {
      this.updateError = String(reason);
    }
  }

  /** Installs a found release by itself when the setting asks for that. */
  private async installIfWanted() {
    const update = this.update;
    if (!update?.install || !this.updates.install || this.triedVersion === update.version) return;
    // Asking for a password with nobody at the window would only be a surprise.
    if (this.asksForPassword && !(await api.windowVisible())) return;
    this.triedVersion = update.version;
    await this.installUpdate();
  }

  /**
   * Downloads and installs the release found, then restarts. A phone opens the release page
   * instead, since it cannot install the new package by itself.
   */
  async installUpdate() {
    const update = this.update;
    if (!update || this.updateProgress !== null) return;
    if (!update.install) {
      open(`${RELEASE_PAGE}${update.version}`);
      return;
    }
    this.updateProgress = 0;
    this.updateError = "";
    try {
      await update.install((fraction) => (this.updateProgress = fraction));
      await api.noteWindow();
      await relaunch();
    } catch (reason) {
      this.updateError = String(reason);
      this.updateDismissed = false;
    } finally {
      this.updateProgress = null;
    }
  }

  /** Reloads everything on screen after a sync or a filter change. */
  async refresh() {
    await Promise.all([
      this.loadEvents(),
      api.overview(GRID_DAYS).then((o) => (this.overview = o)),
      api.repos().then((r) => (this.repos = r)),
      api.downloads().then((d) => (this.downloads = d)),
      api.lifeList().then((l) => (this.lifeList = l)),
      this.celebrate(),
      this.detail ? this.loadDetail(this.detail.repo.id) : null,
    ]);
  }

  /** Loads as many rows as the list wants, asking for one more to learn whether older ones exist. */
  async loadEvents() {
    const request = ++this.asked;
    const query = { ...this.query, unread_only: this.unreadOnly };
    const rows: WatchEvent[] = [];
    while (rows.length <= this.limit) {
      const oldest = rows[rows.length - 1];
      const wanted = Math.min(MAX_BATCH, this.limit + 1 - rows.length);
      const batch = await api.events({
        ...query,
        before: oldest ? [oldest.occurred_at, oldest.id] : null,
        limit: wanted,
      });
      if (request !== this.asked) return;
      rows.push(...batch);
      if (batch.length < wanted) break;
    }
    this.hasMore = rows.length > this.limit;
    this.events = rows.slice(0, this.limit);
    this.eventsLoaded = true;
  }

  private refilter() {
    this.limit = PAGE_SIZE;
    void this.loadEvents();
  }

  setLane(lane: Lane | null) {
    this.lane = lane;
    this.refilter();
  }

  setUnreadOnly(on: boolean) {
    this.unreadOnly = on;
    this.refilter();
  }

  /** Narrows the feed to one grid cell: a kind of event on one day. */
  setCell(lane: Lane, day: string) {
    this.lane = lane;
    this.day = day;
    this.refilter();
  }

  setDay(day: string | null) {
    this.day = day;
    this.refilter();
  }

  setRepoFilter(repo: { id: string; name: string } | null) {
    this.repoFilter = repo;
    this.refilter();
  }

  clearFilters() {
    this.lane = null;
    this.unreadOnly = false;
    this.day = null;
    this.repoFilter = null;
    this.refilter();
  }

  showMore() {
    this.limit += PAGE_SIZE;
    void this.loadEvents();
  }

  async openEvent(event: WatchEvent) {
    open(event.url);
    if (!event.read) await this.markRead([event.id]);
  }

  async markRead(ids: number[]) {
    const marked = new Set(ids);
    this.events = this.events.map((e) => (marked.has(e.id) ? { ...e, read: true } : e));
    if (this.recap) {
      const left = this.recap.events.filter((e) => !marked.has(e.id));
      this.recap = left.length ? { ...this.recap, events: left } : null;
    }
    this.status.unread = await api.markRead(ids);
  }

  /** Marks as read everything the list's filters match, not only the rows loaded. */
  async markAllRead() {
    this.status.unread = await api.markAllRead(this.query);
    this.recap = null;
    await this.loadEvents();
  }

  /** Puts the newest unread special event on screen, once. */
  private async celebrate() {
    const unread = await api.events({ unread_only: true, limit: 40 });
    const event = unread.find(isSpecial);
    if (!event || event.id <= this.marks.celebrated) return;
    this.marks.celebrated = event.id;
    void api.setCelebrated(event.id);
    if (this.celebrates) this.celebration = event;
  }

  /** The window was left: what is on screen now counts as looked at. */
  leave() {
    if (this.awayAt !== null) return;
    this.awayAt = Date.now();
    if (this.recap && Date.now() - this.recapShownAt > 3000) this.recap = null;
    void api.markSeen().then((id) => (this.marks.seen = id));
  }

  /**
   * A phone's app checks only while it is on screen. Out of sight, Android's background job
   * takes over, so whatever it found is read back on return.
   */
  shown(visible: boolean) {
    if (!mobile) return;
    void api.setForeground(visible);
    if (!visible) this.stale = true;
  }

  /** The window is being looked at again. */
  async arrive() {
    const away = this.awayAt;
    this.awayAt = null;
    if (this.stale) {
      this.stale = false;
      if (mobile) {
        this.status = await api.status();
        // Notifications may have been turned on in the phone's settings meanwhile.
        this.mayNotify = (await isPermissionGranted()) === true;
      }
      await this.refresh();
    }
    void this.installIfWanted();
    if (away !== null && Date.now() - away >= BRIEF_ABSENCE) await this.summarise(away);
  }

  /** Gathers what has arrived since the window was last looked at. */
  private async summarise(away: number | null) {
    const unread = await api.events({ unread_only: true, limit: 200 });
    const fresh = unread.filter((e) => e.id > this.marks.seen);
    this.recap = fresh.length ? { events: fresh, away } : null;
    this.recapShownAt = Date.now();
  }

  /** Shows the unread rows the recap spoke of. */
  showUnread() {
    this.recap = null;
    this.lane = null;
    this.day = null;
    this.repoFilter = null;
    this.setUnreadOnly(true);
    if (narrow.current) this.go("activity");
  }

  go(page: Page) {
    this.page = page;
    this.closeRepo();
  }

  /** How many steps back lead to the first tab: one out of a repository page, one out of a tab. */
  private get depth(): number {
    return (this.page !== START ? 1 : 0) + (this.detail || this.wanted ? 1 : 0);
  }

  /**
   * Keeps one history entry for each step back, so Android's back gesture comes to the app
   * first and only leaves it from the first tab. Entries are added only on the way in, from a
   * tap, since the web view skips entries a page adds without one.
   */
  keepBackInStep() {
    const want = this.depth;
    while (this.entries < want) {
      this.entries += 1;
      history.pushState({ depth: this.entries }, "");
    }
    if (this.entries > want && !this.quiet) {
      this.quiet = true;
      history.go(want - this.entries);
    }
  }

  private popped() {
    const state: unknown = history.state;
    this.entries =
      state && typeof state === "object" && "depth" in state && typeof state.depth === "number"
        ? state.depth
        : 0;
    if (this.quiet) {
      this.quiet = false;
    } else {
      this.celebration = null;
      while (this.depth > this.entries) {
        if (this.detail || this.wanted) this.closeRepo();
        else this.go(START);
      }
    }
    this.keepBackInStep();
  }

  /** Checks GitHub now and waits for the check to finish. */
  async checkNow(): Promise<void> {
    if (this.status.demo) {
      await Promise.all([this.refresh(), new Promise((r) => setTimeout(r, 700))]);
      return;
    }
    const done = new Promise<void>((resolve) => this.checked.push(resolve));
    void api.syncNow();
    await Promise.race([done, new Promise((r) => setTimeout(r, CHECK_PATIENCE))]);
  }

  private settle() {
    for (const resolve of this.checked.splice(0)) resolve();
  }

  /** Asks once, after an account is connected, whether the phone may show notifications. */
  private async askToNotify() {
    if (!mobile || this.status.demo || !this.status.login) return;
    if (this.mayNotify !== false || !this.notifying) return;
    try {
      if (localStorage.getItem(ASKED_TO_NOTIFY)) return;
      localStorage.setItem(ASKED_TO_NOTIFY, "1");
    } catch {
      return;
    }
    await this.allowNotifications();
  }

  /** Shows the phone's own question about notifications. */
  async allowNotifications() {
    this.mayNotify = (await requestPermission()) === "granted";
    this.notifyRefused = !this.mayNotify;
  }

  async openRepo(id: string) {
    this.page = "repos";
    await this.loadDetail(id);
  }

  /** The repository page that is wanted, so a late answer cannot reopen a closed one. */
  private wanted: string | null = null;

  private async loadDetail(id: string) {
    this.wanted = id;
    const detail = await api.repoDetail(id);
    if (this.wanted === id) this.detail = detail;
  }

  closeRepo() {
    this.wanted = null;
    this.detail = null;
  }

  syncNow() {
    void api.syncNow();
  }
}

export const watcher = new Watcher();
