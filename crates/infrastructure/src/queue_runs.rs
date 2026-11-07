//! What the daemon remembers between scheduler passes: each started
//! queue's current run and the retries its downloads got (`RunBook`, the
//! adapter for `QueueRuns`), and what "pause all" stopped (`PauseBook`, for
//! `PausedDownloads`). Both live in memory only; a restart starts afresh.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use uidm_domain::ports::{PausedDownloads, QueueRuns};

/// Every change below is a single insert or removal, so a lock poisoned by
/// a panicking thread still holds consistent sets.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Each started queue's run: what it started, and the retries counted.
#[derive(Default)]
pub struct RunBook {
    /// What each started queue has started in its current run.
    runs: Mutex<HashMap<u64, HashSet<u64>>>,
    /// Retries each download got from its queue in the current run.
    attempts: Mutex<HashMap<u64, u32>>,
}

impl QueueRuns for RunBook {
    fn begin(&self, queue: u64) {
        lock(&self.runs).insert(queue, HashSet::new());
    }

    fn end(&self, queue: u64) {
        lock(&self.runs).remove(&queue);
    }

    fn started_in_run(&self, queue: u64) -> HashSet<u64> {
        lock(&self.runs).get(&queue).cloned().unwrap_or_default()
    }

    fn note_started(&self, queue: u64, id: u64) {
        lock(&self.runs).entry(queue).or_default().insert(id);
    }

    fn count_retry(&self, id: u64, max: u32) -> Option<u32> {
        let mut attempts = lock(&self.attempts);
        let n = attempts.entry(id).or_insert(0);
        (*n < max).then(|| {
            *n += 1;
            *n
        })
    }

    fn forget_retries(&self, ids: &HashSet<u64>) {
        let mut attempts = lock(&self.attempts);
        for id in ids {
            attempts.remove(id);
        }
    }
}

/// The queues and downloads "pause all" stopped, for "resume all".
#[derive(Default)]
pub struct PauseBook {
    queues: Mutex<HashSet<u64>>,
    downloads: Mutex<HashSet<u64>>,
}

impl PausedDownloads for PauseBook {
    fn remember(&self, queues: &[u64], downloads: &[u64]) {
        lock(&self.queues).extend(queues);
        lock(&self.downloads).extend(downloads);
    }

    fn take(&self) -> (HashSet<u64>, HashSet<u64>) {
        (std::mem::take(&mut *lock(&self.queues)), std::mem::take(&mut *lock(&self.downloads)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_remembers_what_it_started_until_it_ends() {
        let book = RunBook::default();
        book.begin(1);
        assert!(!book.started_any(1));
        book.note_started(1, 10);
        book.note_started(1, 11);
        assert_eq!(book.started_in_run(1), HashSet::from([10, 11]));
        book.begin(1);
        assert!(!book.started_any(1), "a new run starts empty");
        book.note_started(1, 12);
        book.end(1);
        assert!(book.started_in_run(1).is_empty());
    }

    #[test]
    fn retries_are_counted_up_to_the_most_allowed() {
        let book = RunBook::default();
        assert_eq!(book.count_retry(7, 2), Some(1));
        assert_eq!(book.count_retry(7, 2), Some(2));
        assert_eq!(book.count_retry(7, 2), None);
        book.forget_retries(&HashSet::from([7]));
        assert_eq!(book.count_retry(7, 2), Some(1));
    }

    #[test]
    fn what_was_paused_is_taken_once() {
        let book = PauseBook::default();
        book.remember(&[1], &[5, 6]);
        book.remember(&[2], &[6]);
        assert_eq!(book.take(), (HashSet::from([1, 2]), HashSet::from([5, 6])));
        assert_eq!(book.take(), (HashSet::new(), HashSet::new()));
    }
}
