import { listen } from "@tauri-apps/api/event";
import { MediaQuery } from "svelte/reactivity";
import { api, open } from "./api";
import { LANES, isSpecial, type Lane } from "./kinds";
import { asksForPassword, checkForUpdate, relaunch, type UpdateHandle } from "./updates";
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

const PAGE_SIZE = 80;
/** The most rows the backend hands over in one answer. */
const MAX_BATCH = 500;
/** A look away shorter than this does not count as having been away. */
const BRIEF_ABSENCE = 60_000;
/** How often to look for a newer release. */
const UPDATE_EVERY = 6 * 60 * 60 * 1000;
/** Below this width the activity list becomes a tab instead of a side rail. */
export const narrow = new MediaQuery("max-width: 900px");

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
    await listen<Status>("status", (e) => (this.status = e.payload));
    await listen("synced", () => {
      // A hidden window has nobody to redraw for. It catches up when it is shown.
      if (document.visibilityState === "hidden") this.stale = true;
      else void this.refresh();
    });
    setInterval(() => (this.now = Date.now()), 30_000);
    this.status = await api.status();
    this.asksForPassword = await asksForPassword();
    this.applySettings(await api.settings());
    this.marks = await api.marks();
    await this.refresh();
    await this.summarise(null);
  }

  /** Takes on the settings the window itself acts on. */
  applySettings(settings: Settings) {
    this.celebrates = settings.celebrate;
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
    if (!update || !this.updates.install || this.triedVersion === update.version) return;
    // Asking for a password with nobody at the window would only be a surprise.
    if (this.asksForPassword && !(await api.windowVisible())) return;
    this.triedVersion = update.version;
    await this.installUpdate();
  }

  /** Downloads and installs the release found, then restarts. */
  async installUpdate() {
    const update = this.update;
    if (!update || this.updateProgress !== null) return;
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

  /** The window is being looked at again. */
  async arrive() {
    const away = this.awayAt;
    this.awayAt = null;
    if (this.stale) {
      this.stale = false;
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
