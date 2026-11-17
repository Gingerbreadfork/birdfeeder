use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::auth::{self, TokenSource};
use crate::db::{self, Event, EventQuery, Marks};
use crate::github::GitHub;
use crate::settings::Settings;
use crate::watch::{self, AppState, Status};
use crate::{apply, views};

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

#[tauri::command]
pub fn status(state: State<'_, AppState>) -> Status {
    state.status.lock().unwrap().clone()
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

        if state.demo {
            // Sample data must not leave a login item behind.
            settings.autostart = before.autostart;
        } else if settings.autostart != before.autostart {
            let launcher = handle.autolaunch();
            let result = if settings.autostart {
                launcher.enable()
            } else {
                launcher.disable()
            };
            if let Err(e) = result {
                anyhow::bail!("Could not change start at login: {e}");
            }
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

#[derive(Debug, Default, Serialize)]
pub struct AuthInfo {
    /// Where the token in use comes from, if there is one.
    pub source: Option<TokenSource>,
    pub login: Option<String>,
    pub has_saved_token: bool,
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
        has_saved_token: has_saved_token.is_some(),
        has_gh_cli: has_gh_cli.is_some(),
    })
}

/// Checks a pasted token against GitHub, then keeps it in the system keyring.
#[tauri::command]
pub async fn save_token(state: State<'_, AppState>, token: String) -> CmdResult<String> {
    if state.demo {
        return Err("Start Birdfeeder without --demo to connect an account.".into());
    }
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("Paste a token first.".into());
    }
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
