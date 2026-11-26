import { invoke } from "@tauri-apps/api/core";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import type {
  AuthInfo,
  Downloads,
  EventQuery,
  LifeList,
  Marks,
  Overview,
  RepoDetail,
  RepoRow,
  Settings,
  SignInCode,
  Status,
  WatchEvent,
} from "./types";

export const api = {
  status: () => invoke<Status>("status"),
  syncNow: () => invoke<void>("sync_now"),
  quit: () => invoke<void>("quit"),
  events: (query: EventQuery) => invoke<WatchEvent[]>("events", { query }),
  markRead: (ids: number[]) => invoke<number>("mark_read", { ids }),
  markAllRead: (query: EventQuery) => invoke<number>("mark_all_read", { query }),
  overview: (days: number) => invoke<Overview>("overview", { days }),
  repos: () => invoke<RepoRow[]>("repos"),
  repoDetail: (id: string) => invoke<RepoDetail | null>("repo_detail", { id }),
  /** Saves a card's PNG picture to the downloads folder, and answers with where it went. */
  saveCard: (repo: string, png: Uint8Array) =>
    invoke<string>("save_card", png, { headers: { "card-name": repo } }),
  downloads: () => invoke<Downloads>("downloads"),
  lifeList: () => invoke<LifeList>("life_list"),
  settings: () => invoke<Settings>("settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  authInfo: () => invoke<AuthInfo>("auth_info"),
  saveToken: (token: string) => invoke<string>("save_token", { token }),
  clearToken: () => invoke<void>("clear_token"),
  signInStart: () => invoke<SignInCode>("sign_in_start"),
  signInWait: () => invoke<string>("sign_in_wait"),
  signInCancel: () => invoke<void>("sign_in_cancel"),
  startOver: () => invoke<void>("start_over"),
  windowVisible: () => invoke<boolean>("window_visible"),
  noteWindow: () => invoke<void>("note_window"),
  marks: () => invoke<Marks>("marks"),
  setCelebrated: (id: number) => invoke<void>("set_celebrated", { id }),
  markSeen: () => invoke<number>("mark_seen"),
  setForeground: (visible: boolean) => invoke<void>("set_foreground", { visible }),
  latestRelease: () => invoke<{ version: string; notes: string } | null>("latest_release"),
};

/** Shows a file in the file manager. */
export function reveal(path: string) {
  void revealItemInDir(path);
}

/** Opens a link in the system browser. */
export function open(url: string | null | undefined) {
  if (url && /^https?:\/\//.test(url)) void openUrl(url);
}
