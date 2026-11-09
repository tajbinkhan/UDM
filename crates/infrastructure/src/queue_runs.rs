//! What the daemon remembers between scheduler passes: each started
//! queue's current run and the retries its downloads got (`RunBook`, the
//! adapter for `QueueRuns`), and what "pause all" stopped (`PauseBook`, for
//! `PausedDownloads`). Both live in memory only; a restart starts afresh.

use pullbay_domain::id::{DownloadId, QueueId};
use pullbay_domain::sync::LockExt;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};

use pullbay_domain::ports::{PausedDownloads, QueueRuns};

/// Every change below is a single insert or removal, so a lock poisoned by
/// a panicking thread still holds consistent sets.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock_or_recover()
}

/// Each started queue's run: what it started, and the retries counted.
#[derive(Default)]
pub struct RunBook {
    /// What each started queue has started in its current run.
    runs: Mutex<HashMap<QueueId, HashSet<DownloadId>>>,
    /// Retries each download got from its queue in the current run.
    attempts: Mutex<HashMap<DownloadId, u32>>,
}

impl QueueRuns for RunBook {
    fn begin(&self, queue: QueueId) {
        lock(&self.runs).insert(queue, HashSet::new());
    }

    fn end(&self, queue: QueueId) {
        lock(&self.runs).remove(&queue);
    }

    fn started_in_run(&self, queue: QueueId) -> HashSet<DownloadId> {
        lock(&self.runs).get(&queue).cloned().unwrap_or_default()
    }

    fn note_started(&self, queue: QueueId, id: DownloadId) {
        lock(&self.runs).entry(queue).or_default().insert(id);
    }

    fn count_retry(&self, id: DownloadId, max: u32) -> Option<u32> {
        let mut attempts = lock(&self.attempts);
        let n = attempts.entry(id).or_insert(0);
        (*n < max).then(|| {
            *n += 1;
            *n
        })
    }

    fn forget_retries(&self, ids: &HashSet<DownloadId>) {
        let mut attempts = lock(&self.attempts);
        for id in ids {
            attempts.remove(id);
        }
    }
}

/// The queues and downloads "pause all" stopped, for "resume all".
#[derive(Default)]
pub struct PauseBook {
    queues: Mutex<HashSet<QueueId>>,
    downloads: Mutex<HashSet<DownloadId>>,
}

impl PausedDownloads for PauseBook {
    fn remember(&self, queues: &[QueueId], downloads: &[DownloadId]) {
        lock(&self.queues).extend(queues);
        lock(&self.downloads).extend(downloads);
    }

    fn take(&self) -> (HashSet<QueueId>, HashSet<DownloadId>) {
        (std::mem::take(&mut *lock(&self.queues)), std::mem::take(&mut *lock(&self.downloads)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_remembers_what_it_started_until_it_ends() {
        let book = RunBook::default();
        book.begin(QueueId::new(1));
        assert!(!book.started_any(QueueId::new(1)));
        book.note_started(QueueId::new(1), DownloadId::new(10));
        book.note_started(QueueId::new(1), DownloadId::new(11));
        assert_eq!(book.started_in_run(QueueId::new(1)), HashSet::from([DownloadId::new(10), DownloadId::new(11)]));
        book.begin(QueueId::new(1));
        assert!(!book.started_any(QueueId::new(1)), "a new run starts empty");
        book.note_started(QueueId::new(1), DownloadId::new(12));
        book.end(QueueId::new(1));
        assert!(book.started_in_run(QueueId::new(1)).is_empty());
    }

    #[test]
    fn retries_are_counted_up_to_the_most_allowed() {
        let book = RunBook::default();
        assert_eq!(book.count_retry(DownloadId::new(7), 2), Some(1));
        assert_eq!(book.count_retry(DownloadId::new(7), 2), Some(2));
        assert_eq!(book.count_retry(DownloadId::new(7), 2), None);
        book.forget_retries(&HashSet::from([DownloadId::new(7)]));
        assert_eq!(book.count_retry(DownloadId::new(7), 2), Some(1));
    }

    #[test]
    fn what_was_paused_is_taken_once() {
        let book = PauseBook::default();
        book.remember(&[QueueId::new(1)], &[DownloadId::new(5), DownloadId::new(6)]);
        book.remember(&[QueueId::new(2)], &[DownloadId::new(6)]);
        assert_eq!(book.take(), (HashSet::from([1, 2].map(QueueId::new)), HashSet::from([5, 6].map(DownloadId::new))));
        assert_eq!(book.take(), (HashSet::new(), HashSet::new()));
    }
}
