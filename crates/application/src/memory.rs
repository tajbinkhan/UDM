//! In-memory adapters for the use cases' tests. Their own helpers take and
//! return ids as plain numbers, so tests stay short; the ports they
//! implement take the id types.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use uidm_domain::address::Link;
use uidm_domain::bytes::ByteCount;
use uidm_domain::download::{Download, DownloadEntry, Status};
use uidm_domain::filename::FileName;
use uidm_domain::id::{DownloadId, QueueId};
use uidm_domain::media::Conversion;
use uidm_domain::ports::{
    Answers, Attention, Clock, ControlError, Converter, DownloadControl, DownloadFiles, DownloadIntake, DownloadList,
    EarlyStarts, FilesError, PausedDownloads, PowerControl, QueueFinish, QueueRuns, QueueSounds, QueueStore,
    QuotaState, ReleaseSource, SettingsError, SettingsStore, SyncChecks, UpdateChecks,
};
use uidm_domain::queue::{Moment, PowerRequest, Queue, RunEnd, WhenDone, MAIN_QUEUE, MAIN_QUEUE_NAME};
use uidm_domain::quota::QuotaNotice;
use uidm_domain::settings::Settings;
use uidm_domain::update::{Release, SetupAsset, UpdateError};

pub struct Queues {
    list: Mutex<Vec<Queue>>,
    changes: AtomicUsize,
}

impl Queues {
    /// Just the main queue, empty.
    pub fn main() -> Self {
        Queues { list: Mutex::new(vec![Queue::new(MAIN_QUEUE, MAIN_QUEUE_NAME)]), changes: AtomicUsize::new(0) }
    }

    pub fn get(&self, id: impl Into<QueueId>) -> Queue {
        let id = id.into();
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
    rows: Mutex<HashMap<DownloadId, DownloadEntry>>,
    requests: Mutex<HashMap<DownloadId, Download>>,
}

/// A row for download `id` in state `status`.
pub fn row(id: impl Into<DownloadId>, status: Status) -> DownloadEntry {
    let id = id.into();
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
    pub fn set(&self, id: impl Into<DownloadId>, status: Status) -> bool {
        self.rows.lock().unwrap().get_mut(&id.into()).map(|e| e.status = status).is_some()
    }

    pub fn has_row(&self, id: impl Into<DownloadId>) -> bool {
        self.rows.lock().unwrap().contains_key(&id.into())
    }
}

impl DownloadList for Downloads {
    fn status(&self, id: DownloadId) -> Option<Status> {
        self.rows.lock().unwrap().get(&id).map(|e| e.status)
    }

    fn entry(&self, id: DownloadId) -> Option<DownloadEntry> {
        self.rows.lock().unwrap().get(&id).cloned()
    }

    fn take_request(&self, id: DownloadId) -> Option<Download> {
        self.requests.lock().unwrap().remove(&id)
    }

    fn drop_row(&self, id: DownloadId) {
        self.rows.lock().unwrap().remove(&id);
    }

    fn update(&self, id: DownloadId, f: impl FnOnce(&mut DownloadEntry) -> bool) -> Option<bool> {
        let mut rows = self.rows.lock().unwrap();
        let row = rows.get_mut(&id)?;
        let mut changed = row.clone();
        let ok = f(&mut changed);
        if ok {
            *row = changed;
        }
        Some(ok)
    }

    fn request(&self, id: DownloadId) -> Option<Download> {
        self.requests.lock().unwrap().get(&id).cloned()
    }

    fn keep_request(&self, id: DownloadId, download: Download) {
        self.requests.lock().unwrap().insert(id, download);
    }

    fn edit_request(&self, id: DownloadId, f: impl FnOnce(&mut Download)) {
        if let Some(d) = self.requests.lock().unwrap().get_mut(&id) {
            f(d);
        }
    }

    fn finished(&self) -> HashSet<DownloadId> {
        self.ids_where(|_, s| s == Status::Done).into_iter().collect()
    }

    fn set_waiting(&self, id: DownloadId) -> bool {
        self.set(id, Status::Waiting)
    }

    fn find_last(&self, keep: impl Fn(&DownloadEntry) -> bool) -> Option<DownloadEntry> {
        let rows = self.rows.lock().unwrap();
        let mut found: Vec<&DownloadEntry> = rows.values().filter(|e| keep(e)).collect();
        found.sort_by_key(|e| e.id);
        found.last().map(|e| (*e).clone())
    }

    fn ids_where(&self, keep: impl Fn(DownloadId, Status) -> bool) -> Vec<DownloadId> {
        let mut ids: Vec<DownloadId> =
            self.rows.lock().unwrap().values().filter(|e| keep(e.id, e.status)).map(|e| e.id).collect();
        ids.sort_unstable();
        ids
    }
}

/// The queues that have a run going, with what each run started, and the
/// retries counted per download.
#[derive(Default)]
pub struct Runs(Mutex<HashMap<QueueId, HashSet<DownloadId>>>, Mutex<HashMap<DownloadId, u32>>);

impl Runs {
    pub fn running(&self, queue: impl Into<QueueId>) -> bool {
        self.0.lock().unwrap().contains_key(&queue.into())
    }

    /// Queue `queue`'s run started download `id`.
    pub fn started(&self, queue: impl Into<QueueId>, id: impl Into<DownloadId>) {
        self.0.lock().unwrap().entry(queue.into()).or_default().insert(id.into());
    }
}

impl QueueRuns for Runs {
    fn begin(&self, queue: QueueId) {
        self.0.lock().unwrap().insert(queue, HashSet::new());
    }

    fn end(&self, queue: QueueId) {
        self.0.lock().unwrap().remove(&queue);
    }

    fn started_in_run(&self, queue: QueueId) -> HashSet<DownloadId> {
        self.0.lock().unwrap().get(&queue).cloned().unwrap_or_default()
    }

    fn note_started(&self, queue: QueueId, id: DownloadId) {
        self.started(queue, id);
    }

    fn count_retry(&self, id: DownloadId, max: u32) -> Option<u32> {
        let mut retries = self.1.lock().unwrap();
        let n = retries.entry(id).or_insert(0);
        (*n < max).then(|| {
            *n += 1;
            *n
        })
    }

    fn forget_retries(&self, ids: &HashSet<DownloadId>) {
        self.1.lock().unwrap().retain(|id, _| !ids.contains(id));
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

/// Records the downloads held, stopped and restarted. Downloads in
/// `refuse` cannot be started.
#[derive(Default)]
pub struct Control {
    held: Mutex<Vec<u64>>,
    stopped: Mutex<Vec<u64>>,
    forgotten: Mutex<Vec<u64>>,
    restarted: Mutex<Vec<Restart>>,
    limits: Mutex<Vec<(u64, Option<u64>)>>,
    started_again: Mutex<Vec<(u64, Download)>>,
    dispatched: Mutex<Vec<u64>>,
    canceled_waiting: Mutex<Vec<u64>>,
    waiting_for_link: Mutex<Option<DownloadId>>,
    refuse: HashSet<DownloadId>,
    /// What [`DownloadControl::check_partial`] answers.
    partial_check: Option<Result<Option<uidm_domain::download::Mismatch>, ControlError>>,
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
pub const RESTARTED_ID: DownloadId = DownloadId::new(100);

impl Control {
    /// A control that cannot start the downloads in `refuse`.
    pub fn refusing(refuse: &[u64]) -> Self {
        Control { refuse: refuse.iter().copied().map(DownloadId::new).collect(), ..Control::default() }
    }

    pub fn held(&self) -> Vec<u64> {
        self.held.lock().unwrap().clone()
    }

    pub fn stopped(&self) -> Vec<u64> {
        self.stopped.lock().unwrap().clone()
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

    pub fn dispatched(&self) -> Vec<u64> {
        self.dispatched.lock().unwrap().clone()
    }

    pub fn canceled_waiting(&self) -> Vec<u64> {
        self.canceled_waiting.lock().unwrap().clone()
    }

    /// A control whose partial-file checks answer `answer`.
    pub fn checking(answer: Result<Option<uidm_domain::download::Mismatch>, ControlError>) -> Self {
        Control { partial_check: Some(answer), ..Control::default() }
    }
}

impl DownloadControl for Control {
    fn hold(&self, id: DownloadId) {
        self.held.lock().unwrap().push(id.get());
    }

    fn stop(&self, id: DownloadId) {
        self.stopped.lock().unwrap().push(id.get());
    }

    fn forget(&self, id: DownloadId) {
        self.forgotten.lock().unwrap().push(id.get());
    }

    fn set_limit(&self, id: DownloadId, bytes_per_sec: Option<u64>) {
        self.limits.lock().unwrap().push((id.get(), bytes_per_sec));
    }

    fn start_again(&self, id: DownloadId, download: Download) {
        self.started_again.lock().unwrap().push((id.get(), download));
    }

    fn dispatch(&self, id: DownloadId) -> Result<(), ControlError> {
        if self.refuse.contains(&id) {
            return Err(ControlError(format!("download {id} cannot start")));
        }
        self.dispatched.lock().unwrap().push(id.get());
        Ok(())
    }

    fn cancel_waiting(&self, id: DownloadId) {
        self.canceled_waiting.lock().unwrap().push(id.get());
    }

    fn wait_for_link(&self, id: Option<DownloadId>) {
        *self.waiting_for_link.lock().unwrap() = id;
    }

    fn waiting_for_link(&self) -> Option<DownloadId> {
        *self.waiting_for_link.lock().unwrap()
    }

    fn check_partial(
        &self,
        _download: &Download,
        _partial: &str,
    ) -> Result<Option<uidm_domain::download::Mismatch>, ControlError> {
        self.partial_check.clone().expect("the test sets what the check answers")
    }

    fn restart(
        &self,
        download: Download,
        path: Option<PathBuf>,
        partial: Option<PathBuf>,
        remember_limit: bool,
    ) -> (DownloadId, Link) {
        let mut restarted = self.restarted.lock().unwrap();
        let url = download.url.clone();
        restarted.push(Restart { download, path, partial, remember_limit });
        (DownloadId::new(RESTARTED_ID.get() + restarted.len() as u64 - 1), url)
    }
}

/// Records what it was asked to recycle.
#[derive(Default)]
pub struct Files {
    fail: bool,
    recycled: Mutex<Vec<(Option<String>, Option<String>)>>,
    forgot_resume: Mutex<Vec<String>>,
    moved: Mutex<Vec<(PathBuf, PathBuf, bool)>>,
    existing: HashSet<PathBuf>,
    read_only: HashSet<PathBuf>,
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

    /// Files where something already is at each of `paths`.
    pub fn with_existing(paths: &[&str]) -> Self {
        Files { existing: paths.iter().map(PathBuf::from).collect(), ..Files::default() }
    }

    /// Files where UIDM may not write into `dir`.
    pub fn read_only(dir: &str) -> Self {
        Files { read_only: [PathBuf::from(dir)].into(), ..Files::default() }
    }

    /// The moves made: from, to, and whether the file was finished.
    pub fn moved(&self) -> Vec<(PathBuf, PathBuf, bool)> {
        self.moved.lock().unwrap().clone()
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

    fn move_finished(&self, from: &Path, to: &Path) -> Result<(), FilesError> {
        if self.fail {
            return Err(FilesError("the file is open".into()));
        }
        self.moved.lock().unwrap().push((from.to_path_buf(), to.to_path_buf(), true));
        Ok(())
    }

    fn move_partial(&self, from: &Path, to: &Path) -> Result<(), FilesError> {
        self.moved.lock().unwrap().push((from.to_path_buf(), to.to_path_buf(), false));
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        self.existing.contains(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        self.existing.contains(path)
    }

    fn can_write(&self, dir: &Path) -> bool {
        !self.read_only.contains(dir)
    }
}

/// What was paused.
#[derive(Default)]
pub struct Paused(Mutex<(HashSet<QueueId>, HashSet<DownloadId>)>);

impl PausedDownloads for Paused {
    fn remember(&self, queues: &[QueueId], downloads: &[DownloadId]) {
        let mut p = self.0.lock().unwrap();
        p.0.extend(queues);
        p.1.extend(downloads);
    }

    fn take(&self) -> (HashSet<QueueId>, HashSet<DownloadId>) {
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

/// Records the checks started; `running` are the ones going on.
#[derive(Default)]
pub struct Checks {
    running: HashSet<DownloadId>,
    started: Mutex<Vec<u64>>,
}

impl Checks {
    /// Checks where the downloads in `running` are being checked now.
    pub fn going_on(running: &[u64]) -> Self {
        Checks { running: running.iter().copied().map(DownloadId::new).collect(), ..Checks::default() }
    }

    pub fn started(&self) -> Vec<u64> {
        self.started.lock().unwrap().clone()
    }
}

impl SyncChecks for Checks {
    fn checking(&self) -> HashSet<DownloadId> {
        self.running.clone()
    }

    fn check(&self, id: DownloadId) {
        self.started.lock().unwrap().push(id.get());
    }
}

/// Records the runs that ended.
#[derive(Default)]
pub struct Finish(Mutex<Vec<(String, RunEnd)>>);

impl Finish {
    pub fn ended(&self) -> Vec<(String, RunEnd)> {
        self.0.lock().unwrap().clone()
    }
}

impl QueueFinish for Finish {
    fn run_ended(&self, queue: &str, end: &RunEnd) {
        self.0.lock().unwrap().push((queue.to_string(), end.clone()));
    }
}

/// A quota count fixed at `used` bytes, wherever the window lies.
#[derive(Default)]
pub struct Quota {
    used: Mutex<ByteCount>,
    frees_at: u64,
    held: Mutex<Option<(Vec<QueueId>, HashSet<DownloadId>)>>,
    warned: Mutex<bool>,
    announced: Mutex<Vec<QuotaNotice>>,
}

impl Quota {
    /// `used` bytes in the window, which frees up at minute `frees_at`.
    pub fn at(used: ByteCount, frees_at: u64) -> Self {
        Quota { used: Mutex::new(used), frees_at, ..Quota::default() }
    }

    pub fn set_used(&self, used: ByteCount) {
        *self.used.lock().unwrap() = used;
    }

    pub fn announced(&self) -> Vec<QuotaNotice> {
        self.announced.lock().unwrap().clone()
    }
}

impl QuotaState for Quota {
    fn used(&self, _now: u64, _window: u64) -> ByteCount {
        *self.used.lock().unwrap()
    }

    fn frees_at(&self, _now: u64, _window: u64, _limit: ByteCount) -> u64 {
        self.frees_at
    }

    fn paused(&self) -> bool {
        self.held.lock().unwrap().is_some()
    }

    fn hold(&self, queues: &[QueueId], downloads: &[DownloadId]) {
        let mut held = self.held.lock().unwrap();
        let (q, d) = held.get_or_insert_with(Default::default);
        q.extend(queues);
        d.extend(downloads);
    }

    fn release(&self) -> (Vec<QueueId>, HashSet<DownloadId>) {
        self.held.lock().unwrap().take().unwrap_or_default()
    }

    fn warned(&self) -> bool {
        *self.warned.lock().unwrap()
    }

    fn set_warned(&self, warned: bool) {
        *self.warned.lock().unwrap() = warned;
    }

    fn announce(&self, notice: QuotaNotice) {
        self.announced.lock().unwrap().push(notice);
    }
}

/// Records what it was asked to do; never powers anything off.
#[derive(Default)]
pub struct Power {
    exits: AtomicUsize,
    countdown: Mutex<Option<PowerRequest>>,
    started: Mutex<Vec<(PowerRequest, u32)>>,
    canceled: AtomicUsize,
    powered_off: Mutex<Vec<(WhenDone, bool)>>,
}

impl Power {
    pub fn exits(&self) -> usize {
        self.exits.load(Ordering::SeqCst)
    }

    pub fn started(&self) -> Vec<(PowerRequest, u32)> {
        self.started.lock().unwrap().clone()
    }

    pub fn canceled(&self) -> usize {
        self.canceled.load(Ordering::SeqCst)
    }

    pub fn powered_off(&self) -> Vec<(WhenDone, bool)> {
        self.powered_off.lock().unwrap().clone()
    }
}

impl PowerControl for Power {
    fn exit(&self) {
        self.exits.fetch_add(1, Ordering::SeqCst);
    }

    fn start_countdown(&self, request: &PowerRequest, seconds: u32) {
        *self.countdown.lock().unwrap() = Some(request.clone());
        self.started.lock().unwrap().push((request.clone(), seconds));
    }

    fn take_countdown(&self) -> Option<PowerRequest> {
        self.countdown.lock().unwrap().take()
    }

    fn countdown_canceled(&self) {
        self.canceled.fetch_add(1, Ordering::SeqCst);
    }

    fn power_off(&self, action: WhenDone, force_close: bool) {
        self.powered_off.lock().unwrap().push((action, force_close));
    }
}

/// A release source that offers `latest` and records what it was asked.
pub struct Releases {
    latest: Result<Release, UpdateError>,
    fetched: Mutex<Vec<SetupAsset>>,
    started: Mutex<Vec<PathBuf>>,
}

impl Releases {
    pub fn offering(latest: Result<Release, UpdateError>) -> Self {
        Releases { latest, fetched: Mutex::default(), started: Mutex::default() }
    }

    pub fn fetched(&self) -> Vec<SetupAsset> {
        self.fetched.lock().unwrap().clone()
    }

    pub fn started(&self) -> Vec<PathBuf> {
        self.started.lock().unwrap().clone()
    }
}

impl ReleaseSource for Releases {
    fn latest(&self) -> Result<Release, UpdateError> {
        self.latest.clone()
    }

    fn fetch_setup(&self, setup: &SetupAsset) -> Result<PathBuf, UpdateError> {
        self.fetched.lock().unwrap().push(setup.clone());
        Ok(PathBuf::from("/tmp").join(&setup.name))
    }

    fn run_setup(&self, path: &Path) -> Result<(), UpdateError> {
        self.started.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }
}

/// The weekly check's setting and the checks noted.
#[derive(Default)]
pub struct Checked {
    on: bool,
    last: Mutex<u64>,
    noted: Mutex<Vec<(u64, bool)>>,
}

impl Checked {
    pub fn weekly(on: bool, last: u64) -> Self {
        Checked { on, last: Mutex::new(last), ..Checked::default() }
    }

    pub fn noted(&self) -> Vec<(u64, bool)> {
        self.noted.lock().unwrap().clone()
    }
}

impl UpdateChecks for Checked {
    fn weekly(&self) -> (bool, u64) {
        (self.on, *self.last.lock().unwrap())
    }

    fn note_checked(&self, at: u64, save: bool) {
        *self.last.lock().unwrap() = at;
        self.noted.lock().unwrap().push((at, save));
    }
}

/// Records the conversions started; `ffmpeg` says whether it is there.
#[derive(Default)]
pub struct Convert {
    ffmpeg: bool,
    started: Mutex<Vec<Conversion>>,
    running: HashSet<DownloadId>,
    stopped: Mutex<Vec<u64>>,
}

impl Convert {
    /// A converter that finds ffmpeg.
    pub fn with_ffmpeg() -> Self {
        Convert { ffmpeg: true, ..Convert::default() }
    }

    pub fn started(&self) -> Vec<Conversion> {
        self.started.lock().unwrap().clone()
    }

    /// A converter running a conversion as each of the rows `running`.
    pub fn running(running: &[u64]) -> Self {
        Convert { running: running.iter().copied().map(DownloadId::new).collect(), ..Convert::default() }
    }

    pub fn stopped(&self) -> Vec<u64> {
        self.stopped.lock().unwrap().clone()
    }
}

impl Converter for Convert {
    fn available(&self) -> bool {
        self.ffmpeg
    }

    fn start(&self, conversion: Conversion) {
        self.started.lock().unwrap().push(conversion);
    }

    fn stop(&self, id: DownloadId) -> bool {
        if !self.running.contains(&id) {
            return false;
        }
        self.stopped.lock().unwrap().push(id.get());
        true
    }

    fn any_running(&self) -> bool {
        !self.started.lock().unwrap().is_empty()
    }
}

/// Stages downloads as rows in `list`, numbering them from 1000. Downloads
/// whose address ends in `.bad` are not sendable.
pub struct Intake<'a> {
    list: &'a Downloads,
    next: AtomicUsize,
}

impl<'a> Intake<'a> {
    pub fn into(list: &'a Downloads) -> Self {
        Intake { list, next: AtomicUsize::new(1000) }
    }
}

impl DownloadIntake for Intake<'_> {
    fn sendable(&self, download: &Download) -> bool {
        !download.url.as_str().ends_with(".bad")
    }

    fn stage(&self, download: Download) -> (DownloadId, Link) {
        let n = self.next.fetch_add(1, Ordering::SeqCst) as u64;
        let id = DownloadId::new(n);
        let mut e = row(id, Status::Waiting);
        e.url = download.url.clone();
        self.list.rows.lock().unwrap().insert(id, e);
        self.list.keep_request(id, download.clone());
        (id, download.url)
    }
}

/// Records what was brought in front of the user.
#[derive(Default)]
pub struct Shown {
    done: Mutex<Vec<u64>>,
    window: AtomicUsize,
}

impl Shown {
    pub fn done(&self) -> Vec<u64> {
        self.done.lock().unwrap().clone()
    }

    pub fn windows(&self) -> usize {
        self.window.load(Ordering::SeqCst)
    }
}

impl Attention for Shown {
    fn show_done(&self, id: DownloadId) {
        self.done.lock().unwrap().push(id.get());
    }

    fn show_window(&self) {
        self.window.fetch_add(1, Ordering::SeqCst);
    }
}

/// Settings kept in memory; `refuse` makes every save fail.
#[derive(Default)]
pub struct Prefs {
    settings: Mutex<Settings>,
    saves: AtomicUsize,
    refuse: bool,
}

impl Prefs {
    pub fn with(settings: Settings) -> Self {
        Prefs { settings: Mutex::new(settings), ..Prefs::default() }
    }

    pub fn saves(&self) -> usize {
        self.saves.load(Ordering::SeqCst)
    }
}

impl SettingsStore for Prefs {
    fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    fn save_settings(&self, settings: Settings) -> Result<(), SettingsError> {
        if self.refuse {
            return Err(SettingsError::Refused("the settings file is read-only".into()));
        }
        *self.settings.lock().unwrap() = settings;
        self.saves.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// Early starts: downloads in `started` started early and are handed over.
#[derive(Default)]
pub struct Early {
    started: HashSet<DownloadId>,
    handed: Mutex<Vec<(u64, bool, Option<u64>)>>,
    began: Mutex<Vec<(u64, String)>>,
}

impl Early {
    pub fn started(ids: &[u64]) -> Self {
        Early { started: ids.iter().copied().map(DownloadId::new).collect(), ..Early::default() }
    }

    pub fn handed(&self) -> Vec<(u64, bool, Option<u64>)> {
        self.handed.lock().unwrap().clone()
    }

    /// The early starts begun, with the file name each was given.
    pub fn began(&self) -> Vec<(u64, String)> {
        self.began.lock().unwrap().clone()
    }
}

impl EarlyStarts for Early {
    fn start(&self, id: DownloadId, _download: Download, filename: &FileName) {
        self.began.lock().unwrap().push((id.get(), filename.to_string()));
    }

    fn hand_over(&self, id: DownloadId, start: bool, queue: Option<QueueId>) -> bool {
        if !self.started.contains(&id) {
            return false;
        }
        self.handed.lock().unwrap().push((id.get(), start, queue.map(QueueId::get)));
        true
    }
}

/// Records the dialogs waited for.
#[derive(Default)]
pub struct Waiting(Mutex<Vec<u64>>);

impl Waiting {
    pub fn awaited(&self) -> Vec<u64> {
        self.0.lock().unwrap().clone()
    }
}

impl Answers for Waiting {
    fn await_answer(&self, id: DownloadId) {
        self.0.lock().unwrap().push(id.get());
    }

    fn await_new_name(&self, id: DownloadId) {
        self.0.lock().unwrap().push(id.get());
    }
}
