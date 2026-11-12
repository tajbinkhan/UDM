//! The Options window's commands: saving it, and its Browse buttons.

use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use tauri::State;
use udm_application::window::{FileKind, FilePicker};

use super::{lang, post};
use crate::actor::{CurrentLanguage, Inbox, Outbox};
use crate::contract::{InvokeError, OptionsDraft, PickKind};
use crate::errors;

/// The system's file and folder dialogs (a managed type).
pub type Pickers = Arc<dyn FilePicker>;

/// Options' OK: check the settings and send them to the daemon.
#[tauri::command]
#[specta::specta]
pub async fn save_options(
    window: tauri::WebviewWindow,
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    draft: OptionsDraft,
) -> Result<(), InvokeError> {
    let (tx, rx) = mpsc::channel();
    post(&outbox, &current, Inbox::SaveOptions { draft: Box::new(draft), reply: tx })?;
    let offline = errors::offline(lang(&current));
    let result = tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(5)))
        .await
        .map_err(|_| offline.clone())?
        .map_err(|_| offline)?;
    result?;
    let _ = window.destroy();
    Ok(())
}

/// A Browse button: a folder, a sound or a program, starting at `start`.
/// `None` when the user cancels.
#[tauri::command]
#[specta::specta]
pub async fn pick_path(
    pickers: State<'_, Pickers>,
    current: State<'_, CurrentLanguage>,
    kind: PickKind,
    start: String,
) -> Result<Option<String>, InvokeError> {
    let lang = lang(&current);
    let pickers = Arc::clone(&pickers);
    let title = lang.tr(match kind {
        PickKind::Folder => "Choose a folder",
        PickKind::Sound => "Choose a sound",
        PickKind::Program => "Choose a program",
    });
    let title = title.to_string();
    let sounds = FileKind::new(lang.tr("Sounds"), &["wav"]);
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let start = Some(Path::new(start.trim())).filter(|p| p.is_absolute());
        match kind {
            PickKind::Folder => pickers.pick_folder(&title, start),
            PickKind::Sound => pickers.pick_file(&title, &[sounds], start),
            PickKind::Program => pickers.pick_file(&title, &[], start),
        }
    })
    .await
    .map_err(|_| errors::offline(lang))?;
    Ok(picked.map(|p| p.display().to_string()))
}
