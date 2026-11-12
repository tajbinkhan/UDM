//! The contract of the smaller tools: batch downloads, a list of links to
//! pick from (the clipboard's or an imported file's), export, find, a
//! download's properties, the speed limiter, the columns, categories and
//! the drop target.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{ColumnKey, FileIcon, Id, QueueItem};

/// Batch download's pattern: an address with one `*`.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchInput {
    pub address: String,
    pub letters: bool,
    pub from: String,
    pub to: String,
    pub size: u32,
}

/// The first, second and last addresses a pattern makes, and how many.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchPreview {
    pub first: String,
    pub second: String,
    pub last: String,
    pub count: u32,
}

/// Batch download.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchView {
    pub connected: bool,
}

/// One link in the list to pick from.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LinkRow {
    pub index: u32,
    pub name: String,
    pub icon: FileIcon,
    pub kind: String,
    pub url: String,
    pub save_to: String,
    pub text: String,
}

/// The links of the clipboard or of an imported list, to tick and add.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LinksView {
    pub title: String,
    pub note: String,
    pub rows: Vec<LinkRow>,
    pub queues: Vec<QueueItem>,
    pub connected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExportScope {
    Queue,
    Selected,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    /// IDM's export file.
    Ef2,
    Text,
    Json,
}

/// Find file's choices.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FindInput {
    pub text: String,
    pub name: bool,
    pub description: bool,
    pub sites: bool,
    pub match_case: bool,
    pub whole: bool,
}

/// A download's properties.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PropertiesView {
    pub id: Id,
    pub name: String,
    pub icon: FileIcon,
    pub kind: String,
    pub status: String,
    pub size: String,
    pub save_to: String,
    pub address: String,
    pub description: String,
    pub page: String,
    pub referrer: String,
    pub login_user: String,
    /// The address and login can change: the download is not finished.
    pub editable: bool,
    pub can_open: bool,
    /// Moving needs it stopped or finished.
    pub can_move: bool,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PropertiesInput {
    pub address: String,
    pub description: String,
    pub referrer: String,
    pub login_user: String,
    /// Empty keeps the stored password.
    pub login_password: String,
}

/// The speed limiter's settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LimiterDraft {
    pub on: bool,
    pub kb_per_sec: u32,
    pub on_startup: bool,
}

/// One column in the column chooser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ColumnChoice {
    pub key: ColumnKey,
    pub title: String,
    pub shown: bool,
    pub width: u32,
}

/// A category as its properties sheet edits it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInput {
    pub name: String,
    pub extensions: String,
    pub sites_on: bool,
    pub sites: String,
    pub folder: String,
    pub remember_folder: bool,
}

/// The drop target.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DropView {
    pub connected: bool,
}

/// View > Font's choice.
#[derive(Debug, Clone, Copy, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListFontInput {
    pub mono: bool,
    pub size: u32,
}
