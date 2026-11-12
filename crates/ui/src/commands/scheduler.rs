//! The Scheduler window's commands: saving, creating, deleting, starting and
//! stopping queues, ordering their downloads, and the download limits.

use std::sync::mpsc;
use std::time::Duration;

use tauri::State;
use udm_application::daemon::Command;
use udm_domain::id::{DownloadId, QueueId};

use super::{lang, post};
use crate::actor::{CurrentLanguage, Inbox, Outbox};
use crate::contract::{Id, InvokeError, QueueDraft, QuotaDraft};
use crate::errors;

/// Hand the actor a message whose reply is the command's result, and wait
/// for it off the main thread.
async fn result(
    outbox: &State<'_, Outbox>,
    current: &State<'_, CurrentLanguage>,
    make: impl FnOnce(mpsc::Sender<Result<(), InvokeError>>) -> Inbox,
) -> Result<(), InvokeError> {
    let (tx, rx) = mpsc::channel();
    post(outbox, current, make(tx))?;
    let offline = errors::offline(lang(current));
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(5)))
        .await
        .map_err(|_| offline.clone())?
        .map_err(|_| offline)?
}

fn send(outbox: &State<'_, Outbox>, current: &State<'_, CurrentLanguage>, command: Command) -> Result<(), InvokeError> {
    post(outbox, current, Inbox::Send { command: Box::new(command), reply: None })
}

/// Apply: save a queue's schedule and download count.
#[tauri::command]
#[specta::specta]
pub async fn save_queue(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    draft: QueueDraft,
) -> Result<(), InvokeError> {
    result(&outbox, &current, |reply| Inbox::SaveQueue { draft: Box::new(draft), reply }).await
}

/// New queue, named `name`.
#[tauri::command]
#[specta::specta]
pub async fn create_queue(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    name: String,
) -> Result<(), InvokeError> {
    let name: String = name.chars().take(200).collect();
    result(&outbox, &current, |reply| Inbox::CreateQueue { name, reply }).await
}

/// Delete a queue; its downloads move to the main queue.
#[tauri::command]
#[specta::specta]
pub async fn delete_queue(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
) -> Result<(), InvokeError> {
    if QueueId::new(id.0) == QueueId::MAIN {
        return Err(errors::invalid(lang(&current), "The main queue cannot be deleted."));
    }
    send(&outbox, &current, Command::DeleteQueue { id: QueueId::new(id.0) })
}

/// Start now or Stop.
#[tauri::command]
#[specta::specta]
pub async fn control_queue(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
    start: bool,
) -> Result<(), InvokeError> {
    let id = QueueId::new(id.0);
    send(&outbox, &current, if start { Command::StartQueue { id } } else { Command::StopQueue { id } })
}

/// Move a download `offset` places along its queue (negative is earlier).
#[tauri::command]
#[specta::specta]
pub async fn move_queued(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
    offset: i32,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::MoveQueued { id: DownloadId::new(id.0), offset: offset.clamp(-1000, 1000) })
}

/// Take a download out of its queue; it stays in the list.
#[tauri::command]
#[specta::specta]
pub async fn remove_queued(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
) -> Result<(), InvokeError> {
    send(&outbox, &current, Command::RemoveFromQueue { id: DownloadId::new(id.0) })
}

/// Apply on the Download limits page.
#[tauri::command]
#[specta::specta]
pub async fn save_quota(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    quota: QuotaDraft,
) -> Result<(), InvokeError> {
    result(&outbox, &current, |reply| Inbox::SaveQuota { quota, reply }).await
}
