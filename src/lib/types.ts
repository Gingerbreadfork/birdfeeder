export type Phase = "idle" | "syncing" | "error" | "signed_out";
export type TokenSource = "saved" | "gh_cli" | "env";

export interface RateLimit {
  remaining: number;
  limit: number;
  reset_at: number;
}

export interface Status {
  demo: boolean;
  phase: Phase;
  step: string | null;
  last_sync_at: string | null;
  next_sync_at: string | null;
  error: string | null;
  error_kind: "auth" | "account" | "offline" | "rate_limit" | "other" | null;
  warnings: string[];
  login: string | null;
  token_source: TokenSource | null;
  rates: Record<string, RateLimit>;
  unread: number;
}

export type Kind =
  | "star"
  | "unstar"
  | "issue"
  | "pr"
  | "fork"
  | "release"
  | "downloads"
  | "follower"
  | "unfollow"
  | "milestone"
  | "inbox"
  | "traffic"
  | "sighting"
  | "record"
  | "trend"
  | "story";

export interface WatchEvent {
  id: number;
  kind: Kind;
  repo_id: string | null;
  repo: string | null;
  actor: string | null;
  actor_avatar: string | null;
  actor_followers: number | null;
  actor_company: string | null;
  subject: string | null;
  number: number | null;
  count: number;
  url: string | null;
  meta: Record<string, any> | null;
  occurred_at: string;
  read: boolean;
}

export interface EventQuery {
  kinds?: string[];
  repo_id?: string | null;
  unread_only?: boolean;
  day?: string | null;
  /** Only events older than this one, as its time and id. */
  before?: [string, number] | null;
  limit?: number;
}

/** The newest events the window has already celebrated and summarised, by id. */
export interface Marks {
  celebrated: number;
  seen: number;
}

export interface Point {
  date: string;
  value: number;
}

export interface Account {
  login: string;
  name: string | null;
  avatar_url: string;
  html_url: string;
  followers: number;
  following: number;
}

export interface Totals {
  repos: number;
  stars: number;
  forks: number;
  watchers: number;
  open_issues: number;
  open_prs: number;
  downloads: number;
  followers: number;
}

export interface Week {
  stars: number;
  forks: number;
  issues: number;
  prs: number;
  followers: number | null;
  downloads: number | null;
}

export interface DayCount {
  date: string;
  kind: Kind;
  count: number;
}

export interface Overview {
  account: Account | null;
  totals: Totals;
  week: Week;
  star_series: Point[];
  days: DayCount[];
  contributions: Point[];
  goals: Goal[];
  records: Records;
  streak: Streak | null;
}

export interface Goal {
  what: "stars" | "all-stars" | "downloads" | "followers";
  repo_id: string | null;
  repo: string | null;
  current: number;
  from: number;
  target: number;
  eta_days: number | null;
}

export interface Best {
  date: string;
  count: number;
}

export interface Records {
  best_day: Best | null;
  best_week: Best | null;
  star_streak: number;
  best_star_streak: number;
}

export interface Streak {
  current: number;
  best: number;
  today: number;
  year: number;
}

export interface Sighting {
  id: string;
  title: string;
  how: string;
  seen: boolean;
  when: string | null;
  detail: string | null;
  progress: [number, number] | null;
}

export interface Visitor {
  login: string;
  avatar: string | null;
  name: string | null;
  company: string | null;
  followers: number;
  kind: Kind;
  repo: string | null;
  when: string;
}

export interface LifeList {
  sightings: Sighting[];
  visitors: Visitor[];
}

export interface RepoRow {
  id: string;
  name: string;
  url: string;
  description: string | null;
  is_private: boolean;
  is_fork: boolean;
  is_archived: boolean;
  language: string | null;
  stars: number;
  forks: number;
  watchers: number;
  open_issues: number;
  open_prs: number;
  downloads: number;
  release_count: number;
  stars_30d: number;
  views: number[];
}

export interface AssetRow {
  name: string;
  downloads: number;
  size: number;
  ignored: boolean;
}

export interface ReleaseRow {
  tag: string;
  name: string | null;
  url: string | null;
  published_at: string | null;
  is_prerelease: boolean;
  downloads: number;
  assets: AssetRow[];
}

export interface TrafficDay {
  date: string;
  views: number;
  view_uniques: number;
  clones: number;
  clone_uniques: number;
}

export interface Referrer {
  referrer: string;
  count: number;
  uniques: number;
}

export interface RepoDetail {
  repo: RepoRow;
  star_series: Point[];
  releases: ReleaseRow[];
  traffic: TrafficDay[];
  referrers: Referrer[];
}

export interface RepoDownloads {
  id: string;
  name: string;
  url: string;
  total: number;
  releases: ReleaseRow[];
}

export interface Downloads {
  total: number;
  tracking_since: string | null;
  gained: Point[];
  repos: RepoDownloads[];
  ignore_assets: string[];
}

export interface NotifyKinds {
  star: boolean;
  issue: boolean;
  pr: boolean;
  fork: boolean;
  release: boolean;
  downloads: boolean;
  follower: boolean;
  milestone: boolean;
  inbox: boolean;
  traffic: boolean;
}

/** Light, dark, or whichever the computer or phone uses. */
export type Theme = "system" | "light" | "dark";

export interface Settings {
  poll_minutes: number;
  include_forks: boolean;
  include_archived: boolean;
  include_org_repos: boolean;
  track_traffic: boolean;
  watch_inbox: boolean;
  watch_hacker_news: boolean;
  notifications: boolean;
  notify: NotifyKinds;
  celebrate: boolean;
  theme: Theme;
  keep_running: boolean;
  autostart: boolean;
  check_updates: boolean;
  install_updates: boolean;
  ignore_assets: string[];
}

export interface AuthInfo {
  source: TokenSource | null;
  login: string | null;
  has_saved_token: boolean;
  /** Whether the saved token came from signing in through GitHub rather than being pasted. */
  signed_in_here: boolean;
  has_gh_cli: boolean;
}

/** What to enter on GitHub to sign in, and where. */
export interface SignInCode {
  user_code: string;
  verification_uri: string;
  /** Seconds until the code stops working. */
  expires_in: number;
}
