//! The site grabber's commands: starting and stopping a crawl, the results,
//! and its projects, templates and filters.

use std::sync::mpsc;
use std::time::Duration;

use tauri::State;
use udm_domain::grab::Filter;
use udm_domain::id::QueueId;

use super::{lang, post};
use crate::actor::{CurrentLanguage, Inbox, Outbox};
use crate::contract::{FilterInput, GrabDraft, GrabOpened, GrabScheduleDraft, Id, InvokeError};
use crate::errors;
use crate::state::UiMessage;

/// Hand the actor a message whose reply is the command's result, and wait
/// for it off the main thread. Saving can take a while on a slow disk.
async fn result<T: Send + 'static>(
    outbox: &State<'_, Outbox>,
    current: &State<'_, CurrentLanguage>,
    make: impl FnOnce(mpsc::Sender<Result<T, InvokeError>>) -> Inbox,
) -> Result<T, InvokeError> {
    let (tx, rx) = mpsc::channel();
    post(outbox, current, make(tx))?;
    let offline = errors::offline(lang(current));
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(30)))
        .await
        .map_err(|_| offline.clone())?
        .map_err(|_| offline)?
}

/// Next on the last step: start exploring the site.
#[tauri::command]
#[specta::specta]
pub async fn grab_start(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    draft: GrabDraft,
) -> Result<(), InvokeError> {
    result(&outbox, &current, |reply| Inbox::GrabStart { draft: Box::new(draft), reply }).await
}

#[tauri::command]
#[specta::specta]
pub async fn grab_stop(outbox: State<'_, Outbox>, current: State<'_, CurrentLanguage>) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::GrabStop)
}

/// Show the results under a node of the results tree.
#[tauri::command]
#[specta::specta]
pub async fn grab_select(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    key: String,
) -> Result<(), InvokeError> {
    let key: String = key.chars().take(4096).collect();
    post(&outbox, &current, Inbox::Ui(UiMessage::GrabNode(key)))
}

/// A new project: back to the wizard, without results.
#[tauri::command]
#[specta::specta]
pub async fn grab_new(outbox: State<'_, Outbox>, current: State<'_, CurrentLanguage>) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::GrabReset))
}

/// Add the ticked results, by their places, to a queue.
#[tauri::command]
#[specta::specta]
pub async fn grab_add(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    indices: Vec<u32>,
    queue: Id,
    start: bool,
) -> Result<(), InvokeError> {
    let queue = QueueId::new(queue.0);
    result(&outbox, &current, |reply| Inbox::GrabAdd { indices, queue, start, reply }).await
}

/// A template's settings, for the wizard's steps.
#[tauri::command]
#[specta::specta]
pub async fn grab_template(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    name: String,
) -> Result<GrabDraft, InvokeError> {
    result(&outbox, &current, |reply| Inbox::GrabTemplate { name, reply }).await
}

#[tauri::command]
#[specta::specta]
pub async fn grab_open(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    name: String,
) -> Result<GrabOpened, InvokeError> {
    result(&outbox, &current, |reply| Inbox::GrabOpen { name, reply }).await
}

/// Save the project, with the results' ticks.
#[tauri::command]
#[specta::specta]
pub async fn grab_save(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    name: String,
    draft: GrabDraft,
    checked: Vec<bool>,
    schedule: GrabScheduleDraft,
) -> Result<(), InvokeError> {
    let name: String = name.chars().take(200).collect();
    let schedule = Box::new(schedule);
    result(&outbox, &current, |reply| Inbox::GrabSave { name, draft: Box::new(draft), checked, schedule, reply }).await
}

#[tauri::command]
#[specta::specta]
pub async fn grab_save_template(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    name: String,
    draft: GrabDraft,
) -> Result<(), InvokeError> {
    let name: String = name.chars().take(200).collect();
    result(&outbox, &current, |reply| Inbox::GrabSaveTemplate { name, draft: Box::new(draft), reply }).await
}

#[tauri::command]
#[specta::specta]
pub async fn grab_save_filters(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    filters: Vec<FilterInput>,
) -> Result<(), InvokeError> {
    let filters: Vec<Filter> = filters
        .into_iter()
        .filter(|f| !f.name.trim().is_empty() && !f.mask.trim().is_empty())
        .map(|f| Filter { name: f.name.trim().to_string(), mask: f.mask.trim().to_string() })
        .collect();
    result(&outbox, &current, |reply| Inbox::GrabSaveFilters { filters, reply }).await
}

/// "Add checked files to the list ... on closing the grabber".
#[tauri::command]
#[specta::specta]
pub async fn grab_add_on_close(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    on: bool,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::GrabAddOnClose(on))
}
