//! The main window's commands: the selection, the tree, the columns and
//! the actions of its toolbar and menus.

use tauri::State;
use udm_domain::id::DownloadId;

use super::{lang, post};
use crate::actor::{CurrentLanguage, Inbox, Outbox};
use crate::contract::{ColumnKey, Id, InvokeError, ListAction, OpenHow};
use crate::errors;
use crate::projection::main::node_of;
use crate::state::UiMessage;

#[tauri::command]
#[specta::specta]
pub async fn select_rows(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    ids: Vec<Id>,
) -> Result<(), InvokeError> {
    let ids = ids.into_iter().take(100_000).map(|id| DownloadId::new(id.0)).collect();
    post(&outbox, &current, Inbox::Ui(UiMessage::Select(ids)))
}

#[tauri::command]
#[specta::specta]
pub async fn select_node(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    key: String,
) -> Result<(), InvokeError> {
    let node = node_of(&key).ok_or_else(|| errors::invalid(lang(&current), "That is not a category."))?;
    post(&outbox, &current, Inbox::Ui(UiMessage::SelectNode(node)))
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_node(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    key: String,
) -> Result<(), InvokeError> {
    let key: String = key.chars().take(200).collect();
    post(&outbox, &current, Inbox::Ui(UiMessage::ToggleNode(key)))
}

#[tauri::command]
#[specta::specta]
pub async fn sort_list(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    column: ColumnKey,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::Sort(column)))
}

#[tauri::command]
#[specta::specta]
pub async fn resize_column(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    column: ColumnKey,
    width: u32,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::Resize(column, width.min(10_000) as f32)))
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_categories(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::ToggleCategories))
}

#[tauri::command]
#[specta::specta]
pub async fn run_list_action(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    action: ListAction,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Act(action))
}

/// Open a finished download's file, or its folder.
#[tauri::command]
#[specta::specta]
pub async fn open_download(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
    how: OpenHow,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Open { id: DownloadId::new(id.0), how })
}

/// Show a download's progress window.
#[tauri::command]
#[specta::specta]
pub async fn show_download(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::ShowProgress(DownloadId::new(id.0))))
}
