//! The Scheduler window's contract: each queue as plain fields to edit, its
//! downloads, and the download limits.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{FileIcon, Id, Then};

/// One queue, as the Schedule tab and the queue's download count edit it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QueueDraft {
    pub id: Id,
    pub name: String,
    /// The main queue, which cannot be deleted or synchronize.
    pub main: bool,
    pub running: bool,
    /// Periodic synchronization rather than one-time downloading.
    pub sync: bool,
    pub start_on_launch: bool,
    pub start_on: bool,
    /// `HH:MM`.
    pub start: String,
    /// Once, on `date`, rather than on `days`.
    pub once: bool,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// 0 is Monday, 6 Sunday.
    pub days: Vec<u8>,
    pub repeat_on: bool,
    pub repeat_hours: u32,
    pub repeat_minutes: u32,
    pub stop_on: bool,
    pub stop: String,
    pub retries_on: bool,
    pub retries: u32,
    pub open_on: bool,
    pub open_file: String,
    pub then: Then,
    pub force_close: bool,
    pub max_concurrent: u32,
}

/// A download waiting in a queue, in its order.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QueueRow {
    pub id: Id,
    pub name: String,
    pub icon: FileIcon,
    pub size: String,
    pub status: String,
    pub time_left: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QueueFiles {
    pub queue: Id,
    pub rows: Vec<QueueRow>,
}

/// The download limits: no more than `mb` megabytes every `hours` hours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QuotaDraft {
    pub enabled: bool,
    pub mb: u32,
    pub hours: u32,
    pub warn: bool,
}

/// The Scheduler window.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerView {
    pub queues: Vec<QueueDraft>,
    pub files: Vec<QueueFiles>,
    pub quota: QuotaDraft,
    /// The name New queue proposes.
    pub new_name: String,
    pub max_at_once: u32,
    pub max_retries: u32,
    pub connected: bool,
}
