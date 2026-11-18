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
#[cfg(desktop)]
mod tray;
mod views;
mod watch;

use tauri::Manager;

use watch::AppState;

/// Passed by the login item so the app starts without opening its window.
#[cfg(desktop)]
const HIDDEN_FLAG: &str = "--hidden";
/// Starts the app on sample data, without contacting GitHub.
const DEMO_FLAG: &str = "--demo";

/// A phone has no command line, so a build can ask for sample data instead.
fn wants_demo() -> bool {
    option_env!("BIRDFEEDER_DEMO").is_some() || std::env::args().any(|arg| arg == DEMO_FLAG)
}

/// The single window, the tray, the updater and the login item belong to a computer.
#[cfg(desktop)]
fn with_desktop_parts(
    builder: tauri::Builder<tauri::Wry>,
    demo: bool,
) -> tauri::Builder<tauri::Wry> {
    let builder = if demo {
        builder
    } else {
        // Sample data never contacts GitHub, so it does not look for updates either.
        builder
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                watch::show_window(app);
            }))
            .plugin(tauri_plugin_updater::Builder::new().build())
    };
    builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_FLAG]),
        ))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if !state.demo && state.keeps_running() {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let demo = wants_demo();
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = with_desktop_parts(builder, demo);
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let conn = if demo {
                let conn = db::open_in_memory()?;
                demo::fill(&conn)?;
                conn
            } else {
                let dir = app.path().app_data_dir()?;
                std::fs::create_dir_all(&dir)?;
                auth::keep_in(&dir);
                db::open(&dir.join(db::FILE_NAME))?
            };
            // A restart for an update comes back the way the window was left.
            // Otherwise a start at login stays out of sight.
            let note = db::take_window_note(&conn).ok().flatten();
            let state = AppState::new(conn, demo);
            #[cfg(desktop)]
            {
                let hidden = match note {
                    Some(was_showing) => !was_showing,
                    None => std::env::args().any(|arg| arg == HIDDEN_FLAG),
                };
                let unread = state.status.lock().unwrap().unread;
                app.manage(state);
                tray::build(app.handle(), unread)?;
                if !hidden {
                    watch::show_window(app.handle());
                }
            }
            #[cfg(mobile)]
            {
                let _ = note;
                app.manage(state);
            }
            if !demo {
                tauri::async_runtime::spawn(watch::run(app.handle().clone()));
            }
            Ok(())
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
            commands::window_visible,
            commands::note_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Birdfeeder");
}
