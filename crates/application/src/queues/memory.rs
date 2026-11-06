//! In-memory adapters for the queue use cases' tests.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use uidm_domain::download::Status;
use uidm_domain::ports::{Clock, DownloadControl, DownloadList, QueueRuns, QueueSounds, QueueStore};
use uidm_domain::queue::{Moment, Queue, MAIN_QUEUE, MAIN_QUEUE_NAME};

pub struct Queues {
    list: Mutex<Vec<Queue>>,
    changes: AtomicUsize,
}

impl Queues {
    /// Just the main queue, empty.
    pub fn main() -> Self {
        Queues { list: Mutex::new(vec![Queue::new(MAIN_QUEUE, MAIN_QUEUE_NAME)]), changes: AtomicUsize::new(0) }
    }

    pub fn get(&self, id: u64) -> Queue {
        self.edit(|qs| qs.iter().find(|q| q.id == id).cloned()).expect("the queue exists")
    }

    /// How many times the queues were announced as changed.
    pub fn changes(&self) -> usize {
        self.changes.load(Ordering::SeqCst)
    }
}

impl QueueStore for Queues {
    fn edit<T>(&self, f: impl FnOnce(&mut Vec<Queue>) -> T) -> T {
        f(&mut self.list.lock().unwrap())
    }

    fn queues_changed(&self) {
        self.changes.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub struct Downloads(Mutex<HashMap<u64, Status>>);

impl Downloads {
    pub fn with(entries: &[(u64, Status)]) -> Self {
        Downloads(Mutex::new(entries.iter().copied().collect()))
    }
}

impl DownloadList for Downloads {
    fn status(&self, id: u64) -> Option<Status> {
        self.0.lock().unwrap().get(&id).copied()
    }

    fn finished(&self) -> HashSet<u64> {
        self.0.lock().unwrap().iter().filter(|(_, s)| **s == Status::Done).map(|(id, _)| *id).collect()
    }

    fn set_waiting(&self, id: u64) -> bool {
        self.0.lock().unwrap().get_mut(&id).map(|s| *s = Status::Waiting).is_some()
    }
}

/// The queues that have a run going.
#[derive(Default)]
pub struct Runs(Mutex<HashSet<u64>>);

impl Runs {
    pub fn running(&self, queue: u64) -> bool {
        self.0.lock().unwrap().contains(&queue)
    }
}

impl QueueRuns for Runs {
    fn begin(&self, queue: u64) {
        self.0.lock().unwrap().insert(queue);
    }

    fn end(&self, queue: u64) {
        self.0.lock().unwrap().remove(&queue);
    }
}

/// Counts the sounds played.
#[derive(Default)]
pub struct Sounds(AtomicUsize);

impl Sounds {
    pub fn started(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

impl QueueSounds for Sounds {
    fn queue_started(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Records the downloads held.
#[derive(Default)]
pub struct Control(Mutex<Vec<u64>>);

impl Control {
    pub fn held(&self) -> Vec<u64> {
        self.0.lock().unwrap().clone()
    }
}

impl DownloadControl for Control {
    fn hold(&self, id: u64) {
        self.0.lock().unwrap().push(id);
    }
}

/// A clock stopped at Wed 2026-10-28 09:30.
pub struct FixedClock(pub Moment);

pub const FIXED_NOW: FixedClock = FixedClock(Moment { day: 20754, minute: 9 * 60 + 30 });

impl Clock for FixedClock {
    fn now(&self) -> Moment {
        self.0
    }
}
