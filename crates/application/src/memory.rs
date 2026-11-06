//! In-memory adapters for the use cases' tests.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use uidm_domain::download::{Download, DownloadEntry, Status};
use uidm_domain::ports::{
    Clock, ControlError, DownloadControl, DownloadFiles, DownloadList, FilesError, PausedDownloads, QueueRuns,
    QueueSounds, QueueStore,
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

/// The download list, with the requests kept for retries.
#[derive(Default)]
pub struct Downloads {
    rows: Mutex<HashMap<u64, DownloadEntry>>,
    requests: Mutex<HashMap<u64, Download>>,
}

/// A row for download `id` in state `status`.
pub fn row(id: u64, status: Status) -> DownloadEntry {
    let json = serde_json::json!({
        "id": id, "url": format!("https://dl.example/{id}.zip"), "status": status,
        "connections": 0, "done": 0, "resumable": false,
    });
    serde_json::from_value(json).expect("a valid row")
}

impl Downloads {
    pub fn with(entries: &[(u64, Status)]) -> Self {
        Self::rows(entries.iter().map(|(id, s)| row(*id, *s)))
    }

    pub fn rows(rows: impl IntoIterator<Item = DownloadEntry>) -> Self {
        Downloads { rows: Mutex::new(rows.into_iter().map(|e| (e.id, e)).collect()), ..Self::default() }
    }

    /// Put download `id` in state `status`. Returns false when there is no
    /// such download.
    pub fn set(&self, id: u64, status: Status) -> bool {
        self.rows.lock().unwrap().get_mut(&id).map(|e| e.status = status).is_some()
    }

    pub fn has_row(&self, id: u64) -> bool {
        self.rows.lock().unwrap().contains_key(&id)
    }
}

impl DownloadList for Downloads {
    fn status(&self, id: u64) -> Option<Status> {
        self.rows.lock().unwrap().get(&id).map(|e| e.status)
    }

    fn entry(&self, id: u64) -> Option<DownloadEntry> {
        self.rows.lock().unwrap().get(&id).cloned()
    }

    fn take_request(&self, id: u64) -> Option<Download> {
        self.requests.lock().unwrap().remove(&id)
    }

    fn drop_row(&self, id: u64) {
        self.rows.lock().unwrap().remove(&id);
    }

    fn update(&self, id: u64, f: impl FnOnce(&mut DownloadEntry) -> bool) -> Option<bool> {
        let mut rows = self.rows.lock().unwrap();
        let row = rows.get_mut(&id)?;
        let mut changed = row.clone();
        let ok = f(&mut changed);
        if ok {
            *row = changed;
        }
        Some(ok)
    }

    fn request(&self, id: u64) -> Option<Download> {
        self.requests.lock().unwrap().get(&id).cloned()
    }

    fn keep_request(&self, id: u64, download: Download) {
        self.requests.lock().unwrap().insert(id, download);
    }

    fn edit_request(&self, id: u64, f: impl FnOnce(&mut Download)) {
        if let Some(d) = self.requests.lock().unwrap().get_mut(&id) {
            f(d);
        }
    }

    fn finished(&self) -> HashSet<u64> {
        self.ids_where(|_, s| s == Status::Done).into_iter().collect()
    }

    fn set_waiting(&self, id: u64) -> bool {
        self.set(id, Status::Waiting)
    }

    fn ids_where(&self, keep: impl Fn(u64, Status) -> bool) -> Vec<u64> {
        let mut ids: Vec<u64> =
            self.rows.lock().unwrap().values().filter(|e| keep(e.id, e.status)).map(|e| e.id).collect();
        ids.sort_unstable();
        ids
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
    forgotten: Mutex<Vec<u64>>,
    restarted: Mutex<Vec<Restart>>,
    limits: Mutex<Vec<(u64, Option<u64>)>>,
    started_again: Mutex<Vec<(u64, Download)>>,
    refuse: HashSet<u64>,
}

/// One call to [`DownloadControl::restart`].
#[derive(Debug, Clone)]
pub struct Restart {
    pub download: Download,
    pub path: Option<PathBuf>,
    pub partial: Option<PathBuf>,
    pub remember_limit: bool,
}

/// The id [`Control`] gives the first restarted download.
pub const RESTARTED_ID: u64 = 100;

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

    pub fn forgotten(&self) -> Vec<u64> {
        self.forgotten.lock().unwrap().clone()
    }

    pub fn restarted(&self) -> Vec<Restart> {
        self.restarted.lock().unwrap().clone()
    }

    pub fn limits(&self) -> Vec<(u64, Option<u64>)> {
        self.limits.lock().unwrap().clone()
    }

    pub fn started_again(&self) -> Vec<(u64, Download)> {
        self.started_again.lock().unwrap().clone()
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

    fn forget(&self, id: u64) {
        self.forgotten.lock().unwrap().push(id);
    }

    fn set_limit(&self, id: u64, bytes_per_sec: Option<u64>) {
        self.limits.lock().unwrap().push((id, bytes_per_sec));
    }

    fn start_again(&self, id: u64, download: Download) {
        self.started_again.lock().unwrap().push((id, download));
    }

    fn restart(
        &self,
        download: Download,
        path: Option<PathBuf>,
        partial: Option<PathBuf>,
        remember_limit: bool,
    ) -> (u64, String) {
        let mut restarted = self.restarted.lock().unwrap();
        let url = download.url.clone();
        restarted.push(Restart { download, path, partial, remember_limit });
        (RESTARTED_ID + restarted.len() as u64 - 1, url)
    }
}

/// Records what it was asked to recycle.
#[derive(Default)]
pub struct Files {
    fail: bool,
    recycled: Mutex<Vec<(Option<String>, Option<String>)>>,
    forgot_resume: Mutex<Vec<String>>,
}

impl Files {
    /// Files that never go: every call fails.
    pub fn failing() -> Self {
        Files { fail: true, ..Files::default() }
    }

    pub fn recycled(&self) -> Vec<(Option<String>, Option<String>)> {
        self.recycled.lock().unwrap().clone()
    }

    pub fn forgot_resume(&self) -> Vec<String> {
        self.forgot_resume.lock().unwrap().clone()
    }
}

impl DownloadFiles for Files {
    fn recycle(&self, path: Option<&str>, partial: Option<&str>) -> Result<(), FilesError> {
        if self.fail {
            return Err(FilesError("the file is open".into()));
        }
        self.recycled.lock().unwrap().push((path.map(String::from), partial.map(String::from)));
        Ok(())
    }

    fn forget_resume(&self, paths: &[&str]) {
        self.forgot_resume.lock().unwrap().extend(paths.iter().map(|p| p.to_string()));
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
