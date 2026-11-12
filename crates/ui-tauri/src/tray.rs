//! UDM's tray icon and its menu. Its items act through the state actor,
//! since the main window may be hidden.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;
use udm_application::daemon::Command;

use crate::actor::{Inbox, Outbox};
use crate::contract::OpenWindow;
use crate::i18n::Language;
use crate::windows;

/// The tray's icon: UDM's logo (`assets/icon-256.png`).
const ICON: &[u8] = include_bytes!("../../../assets/icon-256.png");

#[derive(Debug, thiserror::Error)]
pub enum TrayError {
    #[error("the tray icon did not start: {0}")]
    Tauri(#[from] tauri::Error),
}

/// Put UDM in the tray.
pub fn install(app: &AppHandle, outbox: Outbox, lang: &'static Language) -> Result<(), TrayError> {
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, lang.tr(text), true, None::<&str>);
    let menu = Menu::with_items(
        app,
        &[
            &item("open", "Open UDM")?,
            &PredefinedMenuItem::separator(app)?,
            &item("add", "Add URL")?,
            &item("pause-all", "Pause All")?,
            &item("resume-all", "Resume All")?,
            &PredefinedMenuItem::separator(app)?,
            &item("exit", "Exit")?,
        ],
    )?;
    let on_menu = outbox.clone();
    TrayIconBuilder::with_id("udm")
        .icon(Image::from_bytes(ICON)?)
        .tooltip("UDM")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            let send = |command| on_menu.send(Inbox::Send { command: Box::new(command), reply: None });
            let _ = match event.id().as_ref() {
                "open" => {
                    windows::show_main(app, lang);
                    Ok(())
                }
                "add" => on_menu.send(Inbox::OpenWindow(OpenWindow::AddUrl)),
                "pause-all" => send(Command::PauseAll),
                "resume-all" => send(Command::ResumeAll),
                "exit" => {
                    app.exit(0);
                    Ok(())
                }
                _ => Ok(()),
            };
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                windows::show_main(tray.app_handle(), lang);
            }
        })
        .build(app)?;
    Ok(())
}
