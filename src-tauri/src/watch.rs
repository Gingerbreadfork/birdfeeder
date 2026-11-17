//! The background loop that keeps syncing, and the state it shares with the UI.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::apply::Notice;
use crate::auth::{self, TokenSource};
use crate::github::{self, ApiError, GitHub, RateLimit};
use crate::settings::Settings;
use crate::{db, sync, tray};
use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Syncing,
    Error,
    /// No token could be found.
    SignedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Auth,
    /// The token belongs to a different account than the stored data.
    Account,
    Offline,
    RateLimit,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    /// The app is showing sample data and is not talking to GitHub.
    pub demo: bool,
    pub phase: Phase,
    /// What the running sync is doing right now.
    pub step: Option<String>,
    pub last_sync_at: Option<String>,
    pub next_sync_at: Option<String>,
    pub error: Option<String>,
    pub error_kind: Option<ErrorKind>,
    pub warnings: Vec<String>,
    pub login: Option<String>,
    pub token_source: Option<TokenSource>,
    pub rates: BTreeMap<String, RateLimit>,
    pub unread: i64,
}

pub struct AppState {
    pub demo: bool,
    pub db: Mutex<Connection>,
    pub http: reqwest::Client,
    pub status: Mutex<Status>,
    /// Wakes the loop for an immediate sync.
    pub wake: tokio::sync::Notify,
    pub token: tokio::sync::Mutex<Option<(String, TokenSource)>>,
    /// Whether closing the window hides it. Kept here so the window's thread never waits on the database.
    keep_running: AtomicBool,
    running: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn new(conn: Connection, demo: bool) -> Self {
        let login = db::meta_get(&conn, "account")
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str::<github::User>(&raw).ok())
            .map(|account| account.login);
        let status = Status {
            demo,
            phase: Phase::Idle,
            step: None,
            last_sync_at: db::meta_get(&conn, "last_sync_at").ok().flatten(),
            next_sync_at: None,
            error: None,
            error_kind: None,
            warnings: Vec::new(),
            login,
            token_source: None,
            rates: BTreeMap::new(),
            unread: db::unread_count(&conn).unwrap_or(0),
        };
        Self {
            keep_running: AtomicBool::new(Settings::load(&conn).keep_running),
            demo,
            db: Mutex::new(conn),
            http: github::http_client(),
            status: Mutex::new(status),
            wake: tokio::sync::Notify::new(),
            token: tokio::sync::Mutex::new(None),
            running: tokio::sync::Mutex::new(()),
        }
    }

    pub fn settings(&self) -> Settings {
        Settings::load(&self.db.lock().unwrap())
    }

    pub fn keeps_running(&self) -> bool {
        self.keep_running.load(Ordering::Relaxed)
    }

    pub fn set_keeps_running(&self, on: bool) {
        self.keep_running.store(on, Ordering::Relaxed);
    }

    /// Waits for a running sync to finish and holds the next one off.
    pub async fn pause(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.running.lock().await
    }
}

/// Applies a change to the shared status and tells the UI about it.
pub fn set_status(app: &AppHandle, change: impl FnOnce(&mut Status)) {
    let state = app.state::<AppState>();
    let status = {
        let mut status = state.status.lock().unwrap();
        change(&mut status);
        status.clone()
    };
    let _ = app.emit("status", &status);
}

/// Recounts unread events and reflects the number in the status and the tray.
pub fn refresh_unread(app: &AppHandle) -> i64 {
    let state = app.state::<AppState>();
    let unread = db::unread_count(&state.db.lock().unwrap()).unwrap_or(0);
    set_status(app, |s| s.unread = unread);
    tray::update(app, unread);
    unread
}

pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub async fn run(app: AppHandle) {
    loop {
        let wait = sync_once(&app).await;
        let next = chrono::Utc::now() + chrono::Duration::from_std(wait).unwrap_or_default();
        set_status(&app, |s| {
            s.next_sync_at = Some(next.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
        });
        // Waits in short steps against the wall clock, so time asleep counts too.
        let state = app.state::<AppState>();
        loop {
            let left = (next - chrono::Utc::now()).to_std().unwrap_or_default();
            if left.is_zero() {
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep(left.min(Duration::from_secs(30))) => {}
                _ = state.wake.notified() => break,
            }
        }
    }
}

/// Runs one sync and returns how long to wait before the next.
async fn sync_once(app: &AppHandle) -> Duration {
    let state = app.state::<AppState>();
    let _running = state.running.lock().await;
    let settings = state.settings();
    let interval = Duration::from_secs(u64::from(settings.poll_minutes) * 60);

    set_status(app, |s| {
        s.phase = Phase::Syncing;
        s.step = Some("Starting".into());
    });

    let mut retried = false;
    let outcome = loop {
        let cached = state.token.lock().await.clone();
        let fresh = cached.is_none();
        let Some((token, source)) = (match cached {
            Some(found) => Some(found),
            None => auth::resolve().await,
        }) else {
            set_status(app, |s| {
                s.phase = Phase::SignedOut;
                s.step = None;
                s.error = None;
                s.error_kind = None;
                s.token_source = None;
            });
            return interval;
        };
        *state.token.lock().await = Some((token.clone(), source));
        set_status(app, |s| s.token_source = Some(source));

        let gh = GitHub::new(state.http.clone(), token);
        let report = |step: &str| set_status(app, |s| s.step = Some(step.to_string()));
        let result = sync::run(&state.db, &gh, &settings, &report).await;

        // A cached token may have been rotated, so look it up again once.
        let rejected = matches!(
            result
                .as_ref()
                .err()
                .and_then(|e| e.downcast_ref::<ApiError>()),
            Some(ApiError::Unauthorized)
        );
        if rejected {
            *state.token.lock().await = None;
            if !fresh && !retried {
                retried = true;
                continue;
            }
        }
        break result;
    };

    // A sync that stored its main findings and then failed still reports them.
    let (report, failure) = match outcome {
        Ok(mut report) => {
            let failure = report.cut_short.take();
            (Some(report), failure)
        }
        Err(e) => (None, Some(e)),
    };
    if let Some(report) = report {
        for warning in &report.warnings {
            eprintln!("sync warning: {warning}");
        }
        let finished = failure.is_none().then(sync::now_utc);
        if let Some(now) = &finished {
            let _ = db::meta_set(&state.db.lock().unwrap(), "last_sync_at", now);
        }
        set_status(app, |s| {
            s.phase = Phase::Idle;
            s.step = None;
            s.error = None;
            s.error_kind = None;
            if finished.is_some() {
                s.last_sync_at = finished;
            }
            s.login = Some(report.viewer.login.clone());
            s.rates = report.rates;
            s.warnings = report.warnings;
        });
        refresh_unread(app);
        let _ = app.emit("synced", report.notices.len());
        notify(app, &settings, &report.notices);
    }
    let Some(e) = failure else {
        return interval;
    };

    eprintln!("sync failed: {e:#}");
    let (kind, wait) = match e.downcast_ref::<ApiError>() {
        _ if e.is::<sync::OtherAccount>() => (ErrorKind::Account, interval),
        Some(ApiError::Unauthorized) => (ErrorKind::Auth, interval),
        Some(ApiError::Offline(_)) => (ErrorKind::Offline, interval.min(Duration::from_secs(60))),
        Some(ApiError::RateLimited { reset_at }) => {
            let until_reset = reset_at
                .map(|at| at - chrono::Utc::now().timestamp() + 5)
                .filter(|secs| *secs > 0)
                .map_or(interval, |secs| Duration::from_secs(secs as u64));
            (ErrorKind::RateLimit, until_reset.max(interval))
        }
        _ => (ErrorKind::Other, interval.min(Duration::from_secs(300))),
    };
    set_status(app, |s| {
        s.phase = Phase::Error;
        s.step = None;
        s.error = Some(e.to_string());
        s.error_kind = Some(kind);
    });
    wait
}

fn notify(app: &AppHandle, settings: &Settings, notices: &[Notice]) {
    if !settings.notifications {
        return;
    }
    let wanted: Vec<&Notice> = notices
        .iter()
        .filter(|n| settings.notify.allows(n.kind))
        .collect();
    if wanted.len() <= 3 {
        for notice in wanted {
            show_notification(app, &notice.title, &notice.body);
        }
        return;
    }
    // A burst is folded into one notification, led by its first few titles.
    let lead: Vec<&str> = wanted.iter().take(3).map(|n| n.title.as_str()).collect();
    show_notification(
        app,
        &format!("{} new things on GitHub", wanted.len()),
        &format!("{}, and {} more", lead.join("; "), wanted.len() - 3),
    );
}

/// Sent directly on Linux so the notification carries the app's name, and so the
/// desktop can tie it to the installed app and focus it when clicked.
#[cfg(target_os = "linux")]
fn show_notification(app: &AppHandle, title: &str, body: &str) {
    let config = app.config();
    let name = config.product_name.as_deref().unwrap_or("Birdfeeder");
    let result = notify_rust::Notification::new()
        .appname(name)
        .summary(title)
        .body(body)
        .icon(env!("CARGO_PKG_NAME"))
        .hint(notify_rust::Hint::DesktopEntry(name.to_string()))
        .show();
    if let Err(e) = result {
        eprintln!("notification failed: {e}");
    }
}

#[cfg(not(target_os = "linux"))]
fn show_notification(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}
