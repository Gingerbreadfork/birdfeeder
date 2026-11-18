use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyKinds {
    pub star: bool,
    pub issue: bool,
    pub pr: bool,
    pub fork: bool,
    pub release: bool,
    pub downloads: bool,
    pub follower: bool,
    pub milestone: bool,
    pub inbox: bool,
    pub traffic: bool,
}

impl Default for NotifyKinds {
    fn default() -> Self {
        Self {
            star: true,
            issue: true,
            pr: true,
            fork: true,
            release: true,
            downloads: false,
            follower: true,
            milestone: true,
            inbox: true,
            traffic: true,
        }
    }
}

impl NotifyKinds {
    pub fn allows(&self, kind: &str) -> bool {
        match kind {
            "star" => self.star,
            "issue" => self.issue,
            "pr" => self.pr,
            "fork" => self.fork,
            "release" => self.release,
            "downloads" => self.downloads,
            "follower" => self.follower,
            "milestone" | "sighting" | "record" => self.milestone,
            "inbox" => self.inbox,
            "traffic" | "trend" | "story" => self.traffic,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub poll_minutes: u32,
    pub include_forks: bool,
    pub include_archived: bool,
    pub include_org_repos: bool,
    pub track_traffic: bool,
    pub watch_inbox: bool,
    /// Looks on Hacker News for links to the account's repositories. Off unless asked for.
    pub watch_hacker_news: bool,
    pub notifications: bool,
    pub notify: NotifyKinds,
    /// Shows the bird and a scatter of seed in the window for milestones and the like.
    pub celebrate: bool,
    /// "light", "dark", or "system" to follow the computer or phone.
    pub theme: String,
    pub keep_running: bool,
    pub autostart: bool,
    /// Looks for a newer release at launch and every few hours.
    pub check_updates: bool,
    /// Installs a newer release as soon as it is found, then restarts.
    pub install_updates: bool,
    /// Asset name patterns left out of download counts. `*` matches anything.
    pub ignore_assets: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_minutes: 5,
            include_forks: false,
            include_archived: true,
            include_org_repos: false,
            track_traffic: true,
            watch_inbox: true,
            watch_hacker_news: false,
            notifications: true,
            notify: NotifyKinds::default(),
            celebrate: true,
            theme: "system".into(),
            keep_running: true,
            autostart: false,
            check_updates: true,
            install_updates: false,
            ignore_assets: [
                "*.sig",
                "*.asc",
                "*.sha256",
                "SHA256SUMS*",
                "latest.json",
                "latest*.yml",
            ]
            .map(String::from)
            .to_vec(),
        }
    }
}

impl Settings {
    pub fn load(conn: &Connection) -> Self {
        db::meta_get(conn, "settings")
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, conn: &Connection) -> Result<()> {
        db::meta_set(conn, "settings", &serde_json::to_string(self)?)
    }

    pub fn normalized(mut self) -> Self {
        self.poll_minutes = self.poll_minutes.clamp(1, 24 * 60);
        if !["system", "light", "dark"].contains(&self.theme.as_str()) {
            self.theme = "system".into();
        }
        let mut kept: Vec<String> = Vec::new();
        for pattern in self.ignore_assets.iter().map(|p| p.trim()) {
            if !pattern.is_empty() && !kept.iter().any(|k| k == pattern) {
                kept.push(pattern.to_string());
            }
        }
        self.ignore_assets = kept;
        self
    }
}
