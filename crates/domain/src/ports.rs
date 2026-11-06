//! The ports the application's use cases work through. Adapters implement
//! them: today the daemon binary does, over the state it keeps in memory
//! (see AGENTS.md section 16).

use std::collections::HashSet;

use crate::download::Status;
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

    /// The ids of the finished downloads.
    fn finished(&self) -> HashSet<u64>;

    /// Make download `id` wait to run again. Returns false when there is no
    /// such download.
    fn set_waiting(&self, id: u64) -> bool;

    /// Whether download `id` is running: queued in the engine or downloading.
    fn is_running(&self, id: u64) -> bool {
        matches!(self.status(id), Some(Status::Queued | Status::Downloading))
    }
}

/// Stops downloads that are running.
pub trait DownloadControl: Send + Sync {
    /// Stop download `id` so that it waits in its queue again, with its
    /// progress kept.
    fn hold(&self, id: u64);
}

/// What the scheduler remembers about each queue's current run.
pub trait QueueRuns: Send + Sync {
    /// Queue `queue` started a run with nothing started in it yet.
    fn begin(&self, queue: u64);

    /// Queue `queue`'s run ended (or the queue is gone).
    fn end(&self, queue: u64);
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
