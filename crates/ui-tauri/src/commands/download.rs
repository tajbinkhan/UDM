//! A download's progress window: Pause, Resume and Cancel, its speed
//! limit and what happens when it is done.

use tauri::State;
use udm_application::daemon::Command;
use udm_domain::download::OnDone;

use super::{label_of, lang, post};
use crate::actor::{CurrentLanguage, Inbox, Outbox};
use crate::contract::{DownloadControl, InvokeError, LimitInput, OnDoneInput};
use crate::errors;
use crate::projection::dialogs::when_done;
use crate::windows::Label;

/// The highest limit the window takes, in KB/s (about 4 GB/s).
const MAX_KB_PER_SEC: u32 = 4_000_000;

fn download_of(
    window: &tauri::WebviewWindow,
    current: &State<'_, CurrentLanguage>,
) -> Result<udm_domain::id::DownloadId, InvokeError> {
    match label_of(window, current)? {
        Label::Download(id) => Ok(id),
        _ => Err(errors::gone(lang(current))),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn control_download(
    window: tauri::WebviewWindow,
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    action: DownloadControl,
) -> Result<(), InvokeError> {
    let id = download_of(&window, &current)?;
    let command = match action {
        DownloadControl::Pause | DownloadControl::Cancel => Command::Cancel { id },
        DownloadControl::Resume => Command::Retry { id },
    };
    post(&outbox, &current, Inbox::Send { command: Box::new(command), reply: None })?;
    if action == DownloadControl::Cancel {
        let _ = window.destroy();
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_download_limit(
    window: tauri::WebviewWindow,
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    limit: LimitInput,
) -> Result<(), InvokeError> {
    let id = download_of(&window, &current)?;
    if limit.on && !(1..=MAX_KB_PER_SEC).contains(&limit.kb_per_sec) {
        return Err(errors::invalid(lang(&current), "Type a speed from 1 KB/s up."));
    }
    let bytes_per_sec = limit.on.then(|| u64::from(limit.kb_per_sec) * 1024);
    post(&outbox, &current, Inbox::Limit { id, bytes_per_sec, remember: limit.remember })
}

#[tauri::command]
#[specta::specta]
pub async fn set_download_on_done(
    window: tauri::WebviewWindow,
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    on_done: OnDoneInput,
) -> Result<(), InvokeError> {
    let id = download_of(&window, &current)?;
    let on_done = OnDone { dialog: on_done.dialog, then: when_done(on_done.then), force_close: on_done.force_close };
    let on_done = (on_done != OnDone::default()).then_some(on_done);
    post(&outbox, &current, Inbox::OnDone { id, on_done })
}
