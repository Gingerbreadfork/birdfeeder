use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::watch::{self, AppState};

const ICON: &[u8] = include_bytes!("../icons/tray.png");
const ICON_UNREAD: &[u8] = include_bytes!("../icons/tray-unread.png");

/// The menu line that shows the unread count.
struct UnreadItem(MenuItem<Wry>);

fn unread_label(unread: i64) -> String {
    match unread {
        0 => "Nothing new".to_string(),
        1 => "1 unread event".to_string(),
        n => format!("{n} unread events"),
    }
}

pub fn build(app: &AppHandle, unread: i64) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Birdfeeder", true, None::<&str>)?;
    let count = MenuItem::with_id(app, "unread", unread_label(unread), false, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "Check now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &count,
            &PredefinedMenuItem::separator(app)?,
            &sync,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(if unread > 0 {
            ICON_UNREAD
        } else {
            ICON
        })?)
        .tooltip("Birdfeeder")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => watch::show_window(app),
            "sync" => app.state::<AppState>().wake.notify_one(),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                watch::show_window(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(UnreadItem(count));
    Ok(())
}

pub fn update(app: &AppHandle, unread: i64) {
    if let Some(item) = app.try_state::<UnreadItem>() {
        let _ = item.0.set_text(unread_label(unread));
    }
    if let Some(tray) = app.tray_by_id("main") {
        let bytes = if unread > 0 { ICON_UNREAD } else { ICON };
        if let Ok(icon) = Image::from_bytes(bytes) {
            let _ = tray.set_icon(Some(icon));
        }
        let _ = tray.set_tooltip(Some(match unread {
            0 => "Birdfeeder".to_string(),
            n => format!("Birdfeeder: {}", unread_label(n)),
        }));
    }
}
