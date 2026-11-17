mod apply;
mod auth;
mod commands;
mod db;
mod demo;
mod github;
mod hackernews;
mod highlights;
mod settings;
mod sync;
#[cfg(test)]
mod tests;
mod tray;
mod views;
mod watch;

use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

use watch::AppState;

/// Passed by the login item so the app starts without opening its window.
const HIDDEN_FLAG: &str = "--hidden";
/// Starts the app on sample data, without contacting GitHub.
const DEMO_FLAG: &str = "--demo";

pub fn run() {
    let demo = std::env::args().any(|arg| arg == DEMO_FLAG);
    let mut builder = tauri::Builder::default();
    if !demo {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            watch::show_window(app);
        }));
    }
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_FLAG]),
        ))
        .setup(move |app| {
            let conn = if demo {
                let conn = db::open_in_memory()?;
                demo::fill(&conn)?;
                conn
            } else {
                let dir = app.path().app_data_dir()?;
                std::fs::create_dir_all(&dir)?;
                db::open(&dir.join("birdfeeder.db"))?
            };
            let state = AppState::new(conn, demo);
            let unread = state.status.lock().unwrap().unread;
            app.manage(state);

            tray::build(app.handle(), unread)?;
            if !std::env::args().any(|arg| arg == HIDDEN_FLAG) {
                watch::show_window(app.handle());
            }
            if !demo {
                tauri::async_runtime::spawn(watch::run(app.handle().clone()));
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if !state.demo && state.keeps_running() {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::sync_now,
            commands::quit,
            commands::events,
            commands::mark_read,
            commands::mark_all_read,
            commands::overview,
            commands::repos,
            commands::repo_detail,
            commands::downloads,
            commands::life_list,
            commands::marks,
            commands::set_celebrated,
            commands::mark_seen,
            commands::settings,
            commands::save_settings,
            commands::auth_info,
            commands::save_token,
            commands::clear_token,
            commands::start_over,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Birdfeeder");
}
