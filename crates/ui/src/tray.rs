//! UDM's tray icon and its menu. Its items act through the state actor,
//! since the main window may be hidden.

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
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

/// The tray's id, for its tooltip.
const ID: &str = "udm";

/// The tray menu's check items, kept so the state can tick them (a managed
/// type).
pub struct Checks {
    limiter: CheckMenuItem<Wry>,
    drop_target: CheckMenuItem<Wry>,
}

/// Tick the speed limiter and the drop target as the settings have them,
/// and say what downloads in the tooltip.
pub fn show(app: &AppHandle, limiter: bool, drop_target: bool, tooltip: &str) {
    if let Some(checks) = app.try_state::<Checks>() {
        let _ = checks.limiter.set_checked(limiter);
        let _ = checks.drop_target.set_checked(drop_target);
    }
    if let Some(tray) = app.tray_by_id(ID) {
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

/// Put UDM in the tray.
pub fn install(app: &AppHandle, outbox: Outbox, lang: &'static Language) -> Result<(), TrayError> {
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, lang.tr(text), true, None::<&str>);
    let check = |id: &str, text: &str| CheckMenuItem::with_id(app, id, lang.tr(text), true, false, None::<&str>);
    let checks =
        Checks { limiter: check("limiter", "Speed Limiter")?, drop_target: check("drop-target", "Show drop target")? };
    let menu = Menu::with_items(
        app,
        &[
            &item("open", "Open UDM")?,
            &PredefinedMenuItem::separator(app)?,
            &item("add", "Add URL")?,
            &item("clipboard-batch", "Add batch download from clipboard")?,
            &item("pause-all", "Pause All")?,
            &item("resume-all", "Resume All")?,
            &PredefinedMenuItem::separator(app)?,
            &checks.limiter,
            &checks.drop_target,
            &PredefinedMenuItem::separator(app)?,
            &item("exit", "Exit")?,
        ],
    )?;
    app.manage(checks);
    let on_menu = outbox.clone();
    TrayIconBuilder::with_id(ID)
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
                "clipboard-batch" => on_menu.send(Inbox::ClipboardBatch),
                "limiter" => on_menu.send(Inbox::ToggleLimiter),
                "drop-target" => on_menu.send(Inbox::ToggleDropTarget),
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
