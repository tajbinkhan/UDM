//! The ports the application's use cases work through. Adapters implement
//! them: today the daemon binary does, over the state it keeps in memory
//! (see AGENTS.md section 16).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::download::{Download, DownloadEntry, Status};
use crate::queue::{Moment, Queue, RunEnd};
use crate::quota::QuotaNotice;

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

    /// Change download `id`'s row with `f` and tell every client. `f`
    /// returns false to leave the row as it was. Returns `None` when there
    /// is no such download, otherwise what `f` returned.
    fn update(&self, id: u64, f: impl FnOnce(&mut DownloadEntry) -> bool) -> Option<bool>;

    /// The request download `id` was made with, if it was kept.
    fn request(&self, id: u64) -> Option<Download>;

    /// Keep `download` as the request download `id` starts from.
    fn keep_request(&self, id: u64, download: Download);

    /// Change download `id`'s kept request with `f`, if one was kept.
    fn edit_request(&self, id: u64, f: impl FnOnce(&mut Download));

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

    /// Set running download `id`'s own speed limit, in bytes per second;
    /// `None` lifts it.
    fn set_limit(&self, id: u64, bytes_per_sec: Option<u64>);

    /// Start download `id` again under its own id from `download`.
    fn start_again(&self, id: u64, download: Download);

    /// Hand waiting download `id` to the engine. Returns whether it started.
    fn dispatch(&self, id: u64) -> bool;

    /// Make download `id` the one waiting for the next link the browser
    /// hands over, or none, and tell every client.
    fn wait_for_link(&self, id: Option<u64>);

    /// The download waiting for a link from the browser, if any.
    fn waiting_for_link(&self) -> Option<u64>;

    /// Ask the server at `download`'s address whether it still has the file
    /// the partial copy at `partial` was taken from. Returns why not, or
    /// `None` when it does.
    fn check_partial(&self, download: &Download, partial: &str) -> Result<Option<String>, ControlError>;

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

    /// Delete the resume records kept beside `paths`, so a file left there
    /// counts as a taken name rather than something to resume.
    fn forget_resume(&self, paths: &[&str]);

    /// Move a finished file to `to`, which must not exist yet.
    fn move_finished(&self, from: &Path, to: &Path) -> Result<(), FilesError>;

    /// Move an unfinished download's file and its resume record to `to`.
    /// Either may be missing.
    fn move_partial(&self, from: &Path, to: &Path) -> Result<(), FilesError>;

    /// Whether anything is at `path`.
    fn exists(&self, path: &Path) -> bool;
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
    fn started_any(&self, queue: u64) -> bool {
        !self.started_in_run(queue).is_empty()
    }

    /// The downloads queue `queue`'s current run started (or checked).
    fn started_in_run(&self, queue: u64) -> HashSet<u64>;

    /// Queue `queue`'s run started (or checked) download `id`.
    fn note_started(&self, queue: u64, id: u64);

    /// Count one more retry of download `id`, if it has had fewer than
    /// `max`. Returns its new count, or `None` when it has had them all.
    fn count_retry(&self, id: u64, max: u32) -> Option<u32>;

    /// Forget the retries counted for `ids`.
    fn forget_retries(&self, ids: &HashSet<u64>);
}

/// The background checks a synchronizing queue runs on its finished
/// downloads: has the server's copy changed since?
pub trait SyncChecks: Send + Sync {
    /// The downloads being checked now.
    fn checking(&self) -> HashSet<u64>;

    /// Start checking finished download `id`. When its copy changed, it
    /// waits to be downloaded again; the scheduler hears when the check ends.
    fn check(&self, id: u64);
}

/// What happens when a queue's run ends having started something.
pub trait QueueFinish: Send + Sync {
    /// Queue `queue`'s run ended as `end` asks: play the sound the user
    /// chose, open the file, and sleep, shut down or exit if asked.
    fn run_ended(&self, queue: &str, end: &RunEnd);
}

/// The sounds the user may have chosen to hear.
pub trait QueueSounds: Send + Sync {
    /// A queue that was not running started.
    fn queue_started(&self);
}

/// The download quota's count and what it has stopped.
pub trait QuotaState: Send + Sync {
    /// Bytes received in the `window` minutes up to minute `now`.
    fn used(&self, now: u64, window: u64) -> u64;

    /// The first minute after `now` at which the window holds less than
    /// `limit` bytes again, if no more arrive.
    fn frees_at(&self, now: u64, window: u64, limit: u64) -> u64;

    /// Whether downloads are paused for the quota.
    fn paused(&self) -> bool;

    /// Downloads are paused for the quota: add `queues` and `downloads` to
    /// what it stopped.
    fn hold(&self, queues: &[u64], downloads: &[u64]);

    /// The quota allows downloads again: what it stopped, forgetting it.
    fn release(&self) -> (Vec<u64>, HashSet<u64>);

    /// Whether the nearly-reached warning was given in this window.
    fn warned(&self) -> bool;

    fn set_warned(&self, warned: bool);

    /// Tell every client the quota's state.
    fn announce(&self, notice: QuotaNotice);
}

/// The computer's local date and time.
pub trait Clock: Send + Sync {
    fn now(&self) -> Moment;
}
