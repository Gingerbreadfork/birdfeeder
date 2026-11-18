use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::auth::{self, TokenSource};
use crate::db::{self, Event, EventQuery, Marks};
use crate::github::GitHub;
use crate::settings::Settings;
use crate::watch::{self, AppState, Status};
use crate::{apply, signin, views};

type CmdResult<T> = Result<T, String>;

fn fail(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Runs database work off the window's thread, so a sync that holds the
/// database cannot freeze the window.
async fn with_db<T, F>(app: &AppHandle, work: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Connection) -> anyhow::Result<T> + Send + 'static,
{
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut conn = state.db.lock().unwrap();
        work(&mut conn)
    })
    .await
    .map_err(fail)?
    .map_err(fail)
}

/// Recounts unread events for the status and the tray.
async fn unread(app: &AppHandle) -> CmdResult<i64> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || watch::refresh_unread(&app))
        .await
        .map_err(fail)
}

/// The current status, with the unread count and last check read afresh, since a
/// phone's background check may have changed both while the app was away.
#[tauri::command]
pub async fn status(app: AppHandle) -> CmdResult<Status> {
    let (unread, last) = with_db(&app, |conn| {
        Ok((db::unread_count(conn)?, db::meta_get(conn, "last_sync_at")?))
    })
    .await?;
    let state = app.state::<AppState>();
    let mut status = state.status.lock().unwrap();
    status.unread = unread;
    if last.is_some() {
        status.last_sync_at = last;
    }
    Ok(status.clone())
}

/// A check this recent is not repeated when the app comes back on screen.
const STILL_FRESH: chrono::Duration = chrono::Duration::minutes(2);

/// A phone tells the app when it comes on screen and goes away. Coming back checks at once,
/// unless the last check, by the app or its background job, was only a moment ago.
#[tauri::command]
pub async fn set_foreground(app: AppHandle, visible: bool) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let was = state.in_foreground();
    state.set_foreground(visible);
    if !visible || was {
        return Ok(());
    }
    state.shown.notify_one();
    let last = with_db(&app, |conn| db::meta_get(conn, "last_sync_at")).await?;
    if crate::sync::older_than(last.as_deref(), STILL_FRESH) {
        state.wake.notify_one();
    }
    Ok(())
}

#[tauri::command]
pub fn sync_now(state: State<'_, AppState>) {
    state.wake.notify_one();
}

/// Quits from the window, which counts everything so far as looked at.
#[tauri::command]
pub async fn quit(app: AppHandle) -> CmdResult<()> {
    let _ = with_db(&app, |conn| db::mark_seen(conn)).await;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn events(app: AppHandle, query: EventQuery) -> CmdResult<Vec<Event>> {
    with_db(&app, move |conn| db::events(conn, &query)).await
}

#[tauri::command]
pub async fn mark_read(app: AppHandle, ids: Vec<i64>) -> CmdResult<i64> {
    with_db(&app, move |conn| db::mark_read(conn, &ids)).await?;
    unread(&app).await
}

/// Marks as read what the activity list is showing under the given filters.
#[tauri::command]
pub async fn mark_all_read(app: AppHandle, query: EventQuery) -> CmdResult<i64> {
    with_db(&app, move |conn| db::mark_matching_read(conn, &query)).await?;
    unread(&app).await
}

#[tauri::command]
pub async fn overview(app: AppHandle, days: Option<i64>) -> CmdResult<views::Overview> {
    let days = days.unwrap_or(91).clamp(7, 371);
    with_db(&app, move |conn| views::overview(conn, days)).await
}

#[tauri::command]
pub async fn repos(app: AppHandle) -> CmdResult<Vec<views::RepoRow>> {
    with_db(&app, |conn| views::repos(conn)).await
}

#[tauri::command]
pub async fn repo_detail(app: AppHandle, id: String) -> CmdResult<Option<views::RepoDetail>> {
    with_db(&app, move |conn| {
        let ignore = Settings::load(conn).ignore_assets;
        views::repo_detail(conn, &id, &ignore)
    })
    .await
}

#[tauri::command]
pub async fn downloads(app: AppHandle) -> CmdResult<views::Downloads> {
    with_db(&app, |conn| {
        let ignore = Settings::load(conn).ignore_assets;
        views::downloads(conn, &ignore)
    })
    .await
}

#[tauri::command]
pub async fn life_list(app: AppHandle) -> CmdResult<views::LifeList> {
    with_db(&app, |conn| views::life_list(conn)).await
}

#[tauri::command]
pub async fn marks(app: AppHandle) -> CmdResult<Marks> {
    with_db(&app, |conn| db::marks(conn)).await
}

#[tauri::command]
pub async fn set_celebrated(app: AppHandle, id: i64) -> CmdResult<()> {
    with_db(&app, move |conn| db::set_celebrated(conn, id)).await
}

#[tauri::command]
pub async fn mark_seen(app: AppHandle) -> CmdResult<i64> {
    with_db(&app, |conn| db::mark_seen(conn)).await
}

#[tauri::command]
pub async fn settings(app: AppHandle) -> CmdResult<Settings> {
    with_db(&app, |conn| Ok(Settings::load(conn))).await
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> CmdResult<Settings> {
    let handle = app.clone();
    let (settings, rescan) = with_db(&app, move |conn| {
        let mut settings = settings.normalized();
        let before = Settings::load(conn);
        let state = handle.state::<AppState>();

        if state.demo || cfg!(mobile) {
            // Sample data must not leave a login item behind, and a phone has none.
            settings.autostart = before.autostart;
        } else if settings.autostart != before.autostart {
            set_autostart(&handle, settings.autostart)?;
        }

        settings.save(conn)?;
        state.set_keeps_running(settings.keep_running);
        if settings.ignore_assets != before.ignore_assets {
            apply::recount_downloads(conn, &settings.ignore_assets)?;
        }
        let rescan = settings.include_forks != before.include_forks
            || settings.include_archived != before.include_archived
            || settings.include_org_repos != before.include_org_repos
            || settings.track_traffic != before.track_traffic
            || settings.watch_inbox != before.watch_inbox
            || settings.watch_hacker_news != before.watch_hacker_news
            || settings.poll_minutes != before.poll_minutes;
        Ok((settings, rescan))
    })
    .await?;
    if rescan {
        app.state::<AppState>().wake.notify_one();
    }
    Ok(settings)
}

#[cfg(desktop)]
fn set_autostart(app: &AppHandle, on: bool) -> anyhow::Result<()> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let result = if on {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|e| anyhow::anyhow!("Could not change start at login: {e}"))
}

#[cfg(mobile)]
fn set_autostart(_app: &AppHandle, _on: bool) -> anyhow::Result<()> {
    Ok(())
}

#[derive(Debug, Default, Serialize)]
pub struct AuthInfo {
    /// Where the token in use comes from, if there is one.
    pub source: Option<TokenSource>,
    pub login: Option<String>,
    pub has_saved_token: bool,
    /// Whether the saved token came from signing in through GitHub rather than being pasted.
    pub signed_in_here: bool,
    pub has_gh_cli: bool,
}

#[tauri::command]
pub async fn auth_info(state: State<'_, AppState>) -> CmdResult<AuthInfo> {
    if state.demo {
        // Sample data has no business with the keyring or the GitHub CLI.
        return Ok(AuthInfo::default());
    }
    let (has_saved_token, has_gh_cli) = tokio::join!(auth::saved_token(), auth::gh_token());
    let (source, login) = {
        let status = state.status.lock().unwrap();
        (status.token_source, status.login.clone())
    };
    Ok(AuthInfo {
        source,
        login,
        signed_in_here: has_saved_token.as_deref().is_some_and(signin::from_app),
        has_saved_token: has_saved_token.is_some(),
        has_gh_cli: has_gh_cli.is_some(),
    })
}

const NOT_IN_DEMO: &str = "Start Birdfeeder without --demo to connect an account.";

/// Checks a pasted token against GitHub, then keeps it in the system keyring.
#[tauri::command]
pub async fn save_token(state: State<'_, AppState>, token: String) -> CmdResult<String> {
    if state.demo {
        return Err(NOT_IN_DEMO.into());
    }
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("Paste a token first.".into());
    }
    keep_token(&state, token).await
}

/// Starts signing in through GitHub, and returns the code to enter there.
#[tauri::command]
pub async fn sign_in_start(
    state: State<'_, AppState>,
    flow: State<'_, signin::Flow>,
) -> CmdResult<signin::Code> {
    if state.demo {
        return Err(NOT_IN_DEMO.into());
    }
    flow.start(&state.http).await.map_err(fail)
}

/// Waits for the code to be approved on GitHub, then keeps the token like a pasted one.
#[tauri::command]
pub async fn sign_in_wait(
    state: State<'_, AppState>,
    flow: State<'_, signin::Flow>,
) -> CmdResult<String> {
    let token = flow.wait(&state.http).await.map_err(fail)?;
    keep_token(&state, token).await
}

#[tauri::command]
pub fn sign_in_cancel(flow: State<'_, signin::Flow>) {
    flow.cancel();
}

/// Checks a token against GitHub, keeps it, and checks the account at once.
async fn keep_token(state: &AppState, token: String) -> CmdResult<String> {
    let viewer = GitHub::new(state.http.clone(), token.clone())
        .viewer()
        .await
        .map_err(fail)?;
    auth::save_token(token.clone()).await.map_err(fail)?;
    *state.token.lock().await = Some((token, TokenSource::Saved));
    state.wake.notify_one();
    Ok(viewer.login)
}

#[tauri::command]
pub async fn clear_token(state: State<'_, AppState>) -> CmdResult<()> {
    if state.demo {
        return Ok(());
    }
    auth::clear_token().await.map_err(fail)?;
    *state.token.lock().await = None;
    state.wake.notify_one();
    Ok(())
}

/// Whether the window is on screen, as far as the desktop says.
fn window_showing(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(true)
}

#[tauri::command]
pub fn window_visible(app: AppHandle) -> bool {
    window_showing(&app)
}

/// Called just before a restart for an update, so the app comes back the way it was left.
#[tauri::command]
pub async fn note_window(app: AppHandle) -> CmdResult<()> {
    let visible = window_showing(&app);
    with_db(&app, move |conn| db::note_window(conn, visible)).await
}

/// Clears what was collected for one account so another can be watched, then checks at once.
#[tauri::command]
pub async fn start_over(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    if state.demo {
        return Ok(());
    }
    {
        let _paused = state.pause().await;
        with_db(&app, db::forget_account).await?;
        watch::set_status(&app, |s| {
            s.login = None;
            s.last_sync_at = None;
            s.warnings.clear();
        });
        unread(&app).await?;
        let _ = app.emit("synced", 0);
    }
    state.wake.notify_one();
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Release {
    pub version: String,
    pub notes: String,
}

/// The newest release, when it is newer than this copy. A phone cannot install
/// it by itself, so it only learns that one exists.
#[tauri::command]
pub async fn latest_release(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<Release>> {
    #[derive(serde::Deserialize)]
    struct Manifest {
        version: String,
        #[serde(default)]
        notes: String,
    }
    if state.demo {
        return Ok(None);
    }
    let manifest: Manifest = state
        .http
        .get(LATEST_MANIFEST)
        .send()
        .await
        .and_then(|resp| resp.error_for_status())
        .map_err(fail)?
        .json()
        .await
        .map_err(fail)?;
    let current = app.package_info().version.to_string();
    Ok(newer(&manifest.version, &current).then_some(Release {
        version: manifest.version,
        notes: manifest.notes,
    }))
}

/// The manifest the desktop updater reads, which names the newest version.
const LATEST_MANIFEST: &str =
    "https://github.com/Gingerbreadfork/birdfeeder/releases/latest/download/latest.json";

/// Compares dotted version numbers. A pre-release suffix is ignored.
pub fn newer(candidate: &str, current: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .split(['-', '+'])
            .next()
            .unwrap_or_default()
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parts(candidate) > parts(current)
}
