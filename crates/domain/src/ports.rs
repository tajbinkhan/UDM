//! The ports the application's use cases work through. Adapters implement
//! them: today the daemon binary does, over the state it keeps in memory
//! (see AGENTS.md section 16).

use std::collections::HashSet;
use std::path::PathBuf;

use crate::download::{Download, DownloadEntry, Status};
use crate::queue::{Moment, Queue};

/// The download queues, the main queue first.
pub trait QueueStore: Send + Sync {
    /// Run `f` on the queues while nothing else changes them.
    fn edit<T>(&self, f: impl FnOnce(&mut Vec<Queue>) -> T) -> T;

    /// The queues changed: save them, tell every client, and let the
    /// scheduler look at them again.
    fn queues_changed(&self);
}

/// The downloads in the list.
pub trait DownloadList: Send + Sync {
    /// The state of download `id`, or `None` when there is no such download.
    fn status(&self, id: u64) -> Option<Status>;

    /// Download `id` as the list shows it.
    fn entry(&self, id: u64) -> Option<DownloadEntry>;

    /// The request download `id` was made with, if it was kept, forgetting
    /// it. It may hold the browser's cookies.
    fn take_request(&self, id: u64) -> Option<Download>;

    /// Take download `id`'s row out of the list and tell every client.
    fn drop_row(&self, id: u64);

    /// The ids of the finished downloads.
    fn finished(&self) -> HashSet<u64>;

    /// Make download `id` wait to run again. Returns false when there is no
    /// such download.
    fn set_waiting(&self, id: u64) -> bool;

    /// The ids of the downloads for which `keep(id, status)` holds.
    fn ids_where(&self, keep: impl Fn(u64, Status) -> bool) -> Vec<u64>;

    /// Whether download `id` is running: queued in the engine or downloading.
    fn is_running(&self, id: u64) -> bool {
        matches!(self.status(id), Some(Status::Queued | Status::Downloading))
    }
}

/// Why a download could not be started again. The message is shown to
/// the user or logged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ControlError(pub String);

/// Stops and restarts downloads.
pub trait DownloadControl: Send + Sync {
    /// Stop download `id` so that it waits in its queue again, with its
    /// progress kept.
    fn hold(&self, id: u64);

    /// Stop download `id`, keeping its progress. It shows as canceled.
    fn stop(&self, id: u64);

    /// Start stopped download `id` again where it left off.
    fn resume(&self, id: u64) -> Result<(), ControlError>;

    /// Remove download `id` from the list for good: its row, its kept
    /// request, and any wait for a new address for it.
    fn forget(&self, id: u64);

    /// Start `download` as a new row, resuming into `path` and `partial`
    /// when given. Without `remember_limit` its speed limit lasts only until
    /// it stops. Returns the new row's id and its address.
    fn restart(
        &self,
        download: Download,
        path: Option<PathBuf>,
        partial: Option<PathBuf>,
        remember_limit: bool,
    ) -> (u64, String);
}

/// Why a download's files could not be removed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct FilesError(pub String);

/// The files a download leaves on disk.
pub trait DownloadFiles: Send + Sync {
    /// Send the file at `path` and the partial copy at `partial` (each with
    /// the files the engine keeps beside it) to the Recycle Bin or Trash.
    fn recycle(&self, path: Option<&str>, partial: Option<&str>) -> Result<(), FilesError>;
}

/// What "pause all" stopped, for "resume all" to start again.
pub trait PausedDownloads: Send + Sync {
    /// Add `queues` and `downloads` to what was paused.
    fn remember(&self, queues: &[u64], downloads: &[u64]);

    /// What was paused, forgetting it.
    fn take(&self) -> (HashSet<u64>, HashSet<u64>);
}

/// What the scheduler remembers about each queue's current run.
pub trait QueueRuns: Send + Sync {
    /// Queue `queue` started a run with nothing started in it yet.
    fn begin(&self, queue: u64);

    /// Queue `queue`'s run ended (or the queue is gone).
    fn end(&self, queue: u64);

    /// Whether queue `queue`'s current run started any download.
    fn started_any(&self, queue: u64) -> bool;
}

/// The sounds the user may have chosen to hear.
pub trait QueueSounds: Send + Sync {
    /// A queue that was not running started.
    fn queue_started(&self);
}

/// The computer's local date and time.
pub trait Clock: Send + Sync {
    fn now(&self) -> Moment;
}
