//! In-memory adapters for the use cases' tests.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use uidm_domain::download::Status;
use uidm_domain::ports::{
    Clock, ControlError, DownloadControl, DownloadList, PausedDownloads, QueueRuns, QueueSounds, QueueStore,
};
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
        self.set(id, Status::Waiting)
    }

    fn ids_where(&self, keep: impl Fn(u64, Status) -> bool) -> Vec<u64> {
        let mut ids: Vec<u64> =
            self.0.lock().unwrap().iter().filter(|(id, s)| keep(**id, **s)).map(|(id, _)| *id).collect();
        ids.sort_unstable();
        ids
    }
}

impl Downloads {
    /// Put download `id` in state `status`. Returns false when there is no
    /// such download.
    pub fn set(&self, id: u64, status: Status) -> bool {
        self.0.lock().unwrap().get_mut(&id).map(|s| *s = status).is_some()
    }
}

/// The queues that have a run going, with what each run started.
#[derive(Default)]
pub struct Runs(Mutex<HashMap<u64, HashSet<u64>>>);

impl Runs {
    pub fn running(&self, queue: u64) -> bool {
        self.0.lock().unwrap().contains_key(&queue)
    }

    /// Queue `queue`'s run started download `id`.
    pub fn started(&self, queue: u64, id: u64) {
        self.0.lock().unwrap().entry(queue).or_default().insert(id);
    }
}

impl QueueRuns for Runs {
    fn begin(&self, queue: u64) {
        self.0.lock().unwrap().insert(queue, HashSet::new());
    }

    fn end(&self, queue: u64) {
        self.0.lock().unwrap().remove(&queue);
    }

    fn started_any(&self, queue: u64) -> bool {
        self.0.lock().unwrap().get(&queue).is_some_and(|r| !r.is_empty())
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

/// Records the downloads held, stopped and resumed. Downloads in
/// `refuse` cannot be resumed.
#[derive(Default)]
pub struct Control {
    held: Mutex<Vec<u64>>,
    stopped: Mutex<Vec<u64>>,
    resumed: Mutex<Vec<u64>>,
    refuse: HashSet<u64>,
}

impl Control {
    /// A control that cannot resume the downloads in `refuse`.
    pub fn refusing(refuse: &[u64]) -> Self {
        Control { refuse: refuse.iter().copied().collect(), ..Control::default() }
    }

    pub fn held(&self) -> Vec<u64> {
        self.held.lock().unwrap().clone()
    }

    pub fn stopped(&self) -> Vec<u64> {
        self.stopped.lock().unwrap().clone()
    }

    pub fn resumed(&self) -> Vec<u64> {
        self.resumed.lock().unwrap().clone()
    }
}

impl DownloadControl for Control {
    fn hold(&self, id: u64) {
        self.held.lock().unwrap().push(id);
    }

    fn stop(&self, id: u64) {
        self.stopped.lock().unwrap().push(id);
    }

    fn resume(&self, id: u64) -> Result<(), ControlError> {
        if self.refuse.contains(&id) {
            return Err(ControlError(format!("download {id} cannot resume")));
        }
        self.resumed.lock().unwrap().push(id);
        Ok(())
    }
}

/// What was paused.
#[derive(Default)]
pub struct Paused(Mutex<(HashSet<u64>, HashSet<u64>)>);

impl PausedDownloads for Paused {
    fn remember(&self, queues: &[u64], downloads: &[u64]) {
        let mut p = self.0.lock().unwrap();
        p.0.extend(queues);
        p.1.extend(downloads);
    }

    fn take(&self) -> (HashSet<u64>, HashSet<u64>) {
        std::mem::take(&mut *self.0.lock().unwrap())
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
