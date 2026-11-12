//! Commands every window calls.

use tauri::ipc::Channel;
use tauri::State;

use super::{label_of, post};
use crate::actor::{CurrentLanguage, Inbox, Outbox};
use crate::contract::{InvokeError, OpenWindow, Update};
use crate::state::UiMessage;
use crate::windows::Label;

/// Subscribe the calling window to its view: a snapshot, then patches.
/// `languages` are the system's, best first, which the main window passes
/// for an empty Language setting.
#[tauri::command]
#[specta::specta]
pub async fn subscribe(
    window: tauri::WebviewWindow,
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    channel: Channel<Update>,
    languages: Vec<String>,
) -> Result<(), InvokeError> {
    let label = label_of(&window, &current)?;
    if label == Label::Main {
        let tags = languages.into_iter().filter(|t| t.len() <= 35 && t.is_ascii()).take(8).collect();
        post(&outbox, &current, Inbox::Ui(UiMessage::SystemLanguages(tags)))?;
    }
    post(&outbox, &current, Inbox::Subscribe { label, channel })
}

/// An unexpected error in a window, for udm-gui.log (AGENTS.md section 11).
#[tauri::command]
#[specta::specta]
pub async fn report_frontend_error(window: tauri::WebviewWindow, message: String) -> Result<(), InvokeError> {
    let message: String = message.chars().take(4000).collect();
    tracing::error!(window = window.label(), message, "frontend error");
    Ok(())
}

/// Close the calling window, as its title bar's close button does.
#[tauri::command]
#[specta::specta]
pub async fn close_window(window: tauri::WebviewWindow) -> Result<(), InvokeError> {
    let _ = window.destroy();
    Ok(())
}

/// Open one of the windows the main window's buttons and menus open.
#[tauri::command]
#[specta::specta]
pub async fn open_window(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    window: OpenWindow,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::OpenWindow(window))
}

/// Tasks > Exit: close every window and quit, leaving downloads to the
/// daemon.
#[tauri::command]
#[specta::specta]
pub async fn quit_app(app: tauri::AppHandle) -> Result<(), InvokeError> {
    app.exit(0);
    Ok(())
}

/// What the windows tell the watchdog (a managed type).
pub type Heart = std::sync::Arc<crate::watchdog::Beats>;

/// The calling window answers, and is `visible` or not (AGENTS.md section
/// 7.3). It reaches the watchdog directly, not through the state thread,
/// so a stuck state thread does not make every window look stuck.
#[tauri::command]
#[specta::specta]
pub async fn heartbeat(
    window: tauri::WebviewWindow,
    heart: State<'_, Heart>,
    visible: bool,
) -> Result<(), InvokeError> {
    heart.beat(window.label(), visible);
    Ok(())
}

/// A button of the report strip, or Help's Report a problem.
#[tauri::command]
#[specta::specta]
pub async fn report_action(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    action: crate::contract::ReportAction,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Report(action))
}
