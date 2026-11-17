import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
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
  downloads: () => invoke<Downloads>("downloads"),
  lifeList: () => invoke<LifeList>("life_list"),
  settings: () => invoke<Settings>("settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  authInfo: () => invoke<AuthInfo>("auth_info"),
  saveToken: (token: string) => invoke<string>("save_token", { token }),
  clearToken: () => invoke<void>("clear_token"),
  startOver: () => invoke<void>("start_over"),
  windowVisible: () => invoke<boolean>("window_visible"),
  noteWindow: () => invoke<void>("note_window"),
  marks: () => invoke<Marks>("marks"),
  setCelebrated: (id: number) => invoke<void>("set_celebrated", { id }),
  markSeen: () => invoke<number>("mark_seen"),
};

/** Opens a link in the system browser. */
export function open(url: string | null | undefined) {
  if (url && /^https?:\/\//.test(url)) void openUrl(url);
}
