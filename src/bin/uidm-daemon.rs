//! UIDM daemon: the long-lived process that owns the download engine.
//!
//! Clients (a host per browser connection, and the desktop window) connect over
//! loopback TCP and speak the same length-prefixed JSON frames as the browser.
//! Each connection starts with a mutual handshake over the token this daemon
//! wrote at start (see `uidm::ipc`); until the client passes it, it gets no
//! events and its requests are not read.
//!
//! Replies to a request (`hello`, `pong`, `ack`, `downloads`, `added_many`,
//! `media_info`, the site grabber's `grab_progress` and `grab_done`, `error`)
//! go to the client that asked. Download events (`added`, `started`, `progress`, `done`,
//! `failed`, `canceled`, `removed`) go to every connected client. The daemon
//! also applies each event to a `DownloadTable`, so a client that connects late
//! can ask for the whole list.
//!
//! Because downloads live here rather than in the host, they keep running when
//! the browser closes. The daemon exits on its own once it has had no clients
//! and no downloads for `IDLE_EXIT`.
//!
//! The list survives restarts: every change is written to the history file
//! (debounced, and again on exit), and the requests that may still be retried
//! go to a DPAPI-encrypted file next to it (see `uidm::history`). Settings are
//! loaded at start and changed with `set_settings` (see `uidm::settings`).
//!
//! With the save-as dialog on, a new download is probed first and waits as
//! `confirming` until a window answers with `confirm`. If no window is open
//! the daemon starts one; if none connects within `CONFIRM_FALLBACK`, the
//! download starts with the proposed name and folder.
//!
//! When a download finishes or fails, the daemon scans it for viruses if
//! asked, plays the chosen sound, and shows a Windows notification, whose
//! buttons it handles itself (open, show in folder, retry). In dialog mode an
//! open window shows a dialog instead.
//!
//! Queues sit above the engine. Each holds downloads waiting in order; while a
//! queue is started, a scheduler thread starts its waiting downloads up to the
//! queue's own limit (the engine's limit still caps the total), starts and
//! stops queues on their schedules, and when a queue's run is done does what
//! it asks: exit, or sleep, hibernate or shut down after a countdown the
//! window can cancel. Stopping a queue puts its running downloads back to
//! waiting in it, progress kept.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use uidm::app::{progress_to_outgoing, to_request, MAX_MESSAGE_BYTES};
use uidm::engine::DownloadRequest;
use uidm::engine::{self, Engine, Progress};
use uidm::grab::{self, GrabRequest};
use uidm::history::{self, History};
use uidm::ipc::{self, Handshake, AUTH_TIMEOUT, DAEMON_LABEL, HOST_LABEL};
use uidm::net::{self, Login, NetConfig, ProxyKind, ProxyLogin};
use uidm::notify::{self, Sound, Toast};
use uidm::power;
use uidm::protocol::Scan;
use uidm::protocol::{
    read_message, write_frame, Answer, CommandLine, Download, DownloadEntry, DownloadLogin, Incoming, OnDone, Outgoing,
    Properties, Status, Validator,
};
use uidm::queue::{self, Queue, WhenDone, COUNTDOWN_SECS, MAIN_QUEUE};
use uidm::quota::Usage;
#[cfg(windows)]
use uidm::registry;
use uidm::scan;
use uidm::settings::Notify;
use uidm::settings::{self, Duplicate, FileExists, Folders, Settings, SPEED_LIMIT_RANGE_KBPS};
use uidm::settings::{ProxyMode, SocksVersion};
use uidm::state::DownloadTable;

/// Exit after this long with no clients and no queued or running downloads.
const IDLE_EXIT: Duration = Duration::from_secs(60);
const IDLE_POLL: Duration = Duration::from_secs(5);
/// Collect changes for this long before writing the history.
const HISTORY_DEBOUNCE: Duration = Duration::from_secs(1);
/// How long a download waits for a window to show its save-as dialog before
/// it starts with the proposed name and folder.
const CONFIRM_FALLBACK: Duration = Duration::from_secs(20);
/// How long answering a save-as dialog waits for the download it started
/// early to stop, before starting over without its bytes.
const EARLY_STOP_WAIT: Duration = Duration::from_secs(15);
/// The folder early downloads are written in, inside the temporary folder
/// when UIDM's data folder cannot be had.
const EARLY_FOLDER: &str = "UIDM early";

/// Links the link picker's probes ask about at once.
const LINK_PROBES: usize = 4;

/// Where early downloads go: UIDM's own data folder, which only this user
/// can write. The temporary folder is shared by every user on Linux, where
/// another user could read the files or plant a link under the folder.
/// The saved file of an early download reported as stopped, if it in fact
/// finished: its file is there at the size the server gave, and neither a
/// partial file nor a resume file is left (a stopped download keeps both).
fn early_finished_anyway(early: &Early) -> Option<&Path> {
    let target = early.target.as_deref()?;
    let total = early.total?;
    let partial_left = early.file.as_deref().is_some_and(|f| f != target && f.exists());
    let finished = !partial_left
        && !engine::sidecar_path(target).exists()
        && early.file.as_deref().is_none_or(|f| !engine::sidecar_path(f).exists())
        && std::fs::metadata(target).is_ok_and(|m| m.is_file() && m.len() == total);
    finished.then_some(target)
}

fn early_root() -> PathBuf {
    uidm::platform::data_dir().map(|d| d.join("early")).unwrap_or_else(|_| std::env::temp_dir().join(EARLY_FOLDER))
}
/// A failure is announced after this long, so a download the browser
/// extension takes back (and removes) right away is not announced at all.
const FAILED_GRACE: Duration = Duration::from_secs(1);
/// Notifications kept alive so their buttons keep working.
const KEEP_TOASTS: usize = 32;
/// The icon Windows shows on UIDM's notifications, written next to the
/// history so the registry can point at it.
#[cfg(windows)]
const ICON_PNG: &[u8] = include_bytes!("../../extension/icons/128.png");
/// The `client` a desktop window names in its `hello`.
const WINDOW_CLIENT: &str = "window";

/// How long a command line waits for the window its `uidm-gui` opens. One
/// older than this belonged to a window that never connected.
const COMMAND_LINE_WAIT: Duration = Duration::from_secs(60);
/// How often the scheduler looks at the clock when nothing wakes it.
const SCHEDULER_POLL: Duration = Duration::from_secs(5);
/// How often the keep-awake thread looks for running work.
const AWAKE_POLL: Duration = Duration::from_secs(2);
/// How the site grabber names itself to servers.
const GRABBER_AGENT: &str = "Mozilla/5.0 (compatible; UIDM site grabber; obeys robots.txt)";
/// Set to make sleep, hibernate and shut down only log what they would do
/// (for testing the countdown without losing the machine).
const POWER_DRY_RUN_ENV: &str = "UIDM_POWER_DRY_RUN";

static NEXT_CLIENT: AtomicU64 = AtomicU64::new(1);

/// Everything the engine's progress callback needs, so it can exist before the
/// engine does.
struct Hub {
    /// Connected clients by id, each with a channel drained by its writer.
    clients: Mutex<HashMap<u64, Sender<Vec<u8>>>>,
    /// Every download this daemon has seen, kept current from its events.
    table: Mutex<DownloadTable>,
    /// The original request for each download that may still be retried. It
    /// holds the browser's cookies, so it is dropped once a download is done or
    /// removed, and saved only encrypted.
    requests: Mutex<HashMap<u64, Download>>,
    /// Wakes the history writer after every change.
    dirty: Sender<()>,
    /// Clients that said they are a desktop window.
    windows: Mutex<HashSet<u64>>,
    /// Clients that said hello as the browser extension does, with no
    /// `client` (the host strips any).
    browsers: Mutex<HashSet<u64>>,
    /// When a browser extension last connected or sent anything, in
    /// seconds since 1970, for Options' link state.
    browser_seen: Mutex<Option<u64>>,
    /// Finished and failed downloads, for the notification thread.
    notes: Sender<Note>,
    /// The queues, the main queue first. Lock after `table`, never before.
    queues: Mutex<Vec<Queue>>,
    /// Downloads being stopped to wait in their queue again: their
    /// `canceled` event becomes `waiting`.
    held: Mutex<HashSet<u64>>,
    /// Wakes the scheduler when a download ends or a queue changes.
    wake: Sender<()>,
    /// Bytes received lately, for the download quota.
    quota: Mutex<Usage>,
    /// Rows that are conversions, not downloads: their progress is not
    /// received bytes.
    uncounted: Mutex<HashSet<u64>>,
    /// Downloads started while their save-as dialog is open, by the engine
    /// id they run under, which clients never see.
    early: Mutex<HashMap<u64, Early>>,
}

/// A download started while its save-as dialog is open, into a folder of
/// its own, until the dialog is answered.
struct Early {
    /// The download in the list it is for.
    real: u64,
    /// Its folder.
    dir: PathBuf,
    /// The file it writes, once started.
    file: Option<PathBuf>,
    /// The file it saves to when finished, and its size, once known.
    target: Option<PathBuf>,
    total: Option<u64>,
    /// How it ended: finished (with its file), or stopped or failed.
    end: Option<Option<PathBuf>>,
    /// Nobody waits for it: once it ends it goes, with its folder.
    discard: bool,
}

/// A page the browser rendered for a crawl: its HTML and its address then.
/// A page the browser rendered: its HTML, its address, and the addresses
/// its scripts opened in new windows.
type Rendered = Option<(String, String, Vec<String>)>;

/// Work for the notification thread.
enum Note {
    Finished(u64),
    Failed(u64),
    /// A finished download's virus scan is done.
    Scanned(u64),
    /// A download stopped to ask about its taken file name.
    NameTaken(u64),
}

impl Hub {
    /// Apply `msg` to the table and send it to every client. The table lock is
    /// held across the send, so the order clients see matches the table's, and
    /// a `downloads` snapshot (taken under the same lock) is never stale.
    fn publish(&self, msg: &Outgoing) {
        let mut table = self.table.lock().unwrap();
        if let Outgoing::Canceled { id, .. } = msg {
            if self.held.lock().unwrap().remove(id) {
                if let Some(mut item) = table.get(*id).cloned() {
                    item.status = Status::Waiting;
                    // Stopping its queue stops it; starting the queue again
                    // resumes it, without its own limit unless remembered.
                    if !item.limit_remember {
                        item.speed_limit = None;
                    }
                    let waiting = Outgoing::Updated { item };
                    table.apply(&waiting);
                    let _ = self.dirty.send(());
                    let _ = self.wake.send(());
                    self.broadcast(&waiting);
                    return;
                }
            }
        }
        match msg {
            // Bytes a resumed download already had do not count again.
            Outgoing::Started { id, .. } => {
                let done = table.get(*id).map_or(0, |e| e.done);
                self.quota.lock().unwrap().started(*id, done);
            }
            Outgoing::Progress { id, done, .. } if !self.uncounted.lock().unwrap().contains(id) => {
                self.quota.lock().unwrap().progress(*id, *done, uidm::quota::now_minute());
            }
            Outgoing::Done { id, .. } | Outgoing::Failed { id, .. } | Outgoing::Canceled { id, .. } => {
                self.quota.lock().unwrap().ended(*id);
            }
            _ => {}
        }
        table.apply(msg);
        if let Outgoing::Done { id, .. } | Outgoing::Failed { id, .. } = msg {
            // It ended before the stop reached it; nothing to hold.
            self.held.lock().unwrap().remove(id);
        }
        // A one-time queue holds what is still to do: a finished download
        // leaves it. A synchronizing queue keeps it, to check next run.
        if let Outgoing::Done { id, .. } = msg {
            if self.leave_queues_where(*id, |q| !q.kind.is_sync()) {
                self.publish_queues();
            }
        }
        if matches!(
            msg,
            Outgoing::Done { .. } | Outgoing::Failed { .. } | Outgoing::Canceled { .. } | Outgoing::NameTaken { .. }
        ) {
            let _ = self.wake.send(());
        }
        match msg {
            Outgoing::Done { id, url, path, .. } => {
                let referrer = table.get(*id).and_then(|e| e.referrer.clone());
                uidm::platform::mark_from_internet(Path::new(path), url, referrer.as_deref());
                self.requests.lock().unwrap().remove(id);
                let _ = self.notes.send(Note::Finished(*id));
            }
            Outgoing::Failed { id, .. } => {
                let _ = self.notes.send(Note::Failed(*id));
            }
            Outgoing::NameTaken { id, .. } => {
                let _ = self.notes.send(Note::NameTaken(*id));
            }
            _ => {}
        }
        let _ = self.dirty.send(());
        self.broadcast(msg);
        // The list shows a new download's page, description and login name
        // from its request.
        if let Outgoing::Added { id, .. } = msg {
            drop(table);
            let request = self.requests.lock().unwrap().get(id).cloned();
            if let Some(d) = request {
                let _ = self.update(*id, |e| {
                    let before = e.clone();
                    // A source that knows no page apart from the referrer
                    // (a list file, the batch dialog) found the link there.
                    e.page = e.page.take().or(d.page).or_else(|| d.referrer.clone());
                    e.referrer = e.referrer.take().or(d.referrer);
                    e.description = e.description.take().or(d.description);
                    e.category = e.category.take().or(d.category);
                    e.login_user = e.login_user.take().or(d.login.map(|l| l.username));
                    e.speed_limit = e.speed_limit.or(d.speed_limit);
                    e.from_browser = e.from_browser || d.user_agent.is_some();
                    if *e == before {
                        return Err("nothing new".into());
                    }
                    Ok(())
                });
            }
        }
    }

    /// Change download `id` with `f` and announce the new entry, all under the
    /// table lock, so the check `f` makes cannot race another change. `f`
    /// returns an error to leave the entry alone.
    fn update(
        &self,
        id: u64,
        f: impl FnOnce(&mut DownloadEntry) -> Result<(), String>,
    ) -> Result<DownloadEntry, String> {
        let mut table = self.table.lock().unwrap();
        let mut item = table.get(id).cloned().ok_or_else(|| format!("no download with id {id}"))?;
        f(&mut item)?;
        let msg = Outgoing::Updated { item: item.clone() };
        table.apply(&msg);
        let _ = self.dirty.send(());
        self.broadcast(&msg);
        Ok(item)
    }

    /// Whether a client other than a window is connected: the browser
    /// extension, through its host.
    /// Keep the events of an early download (one started while its save-as
    /// dialog is open) from the clients: its progress shows on the download
    /// waiting in the dialog instead. Returns whether `p` was one.
    fn early_event(&self, p: &Progress) -> bool {
        let id = match p {
            Progress::Started { id, .. }
            | Progress::Advanced { id, .. }
            | Progress::Finished { id, .. }
            | Progress::Failed { id, .. }
            | Progress::Canceled { id, .. }
            | Progress::NameTaken { id, .. } => *id,
        };
        let mut early = self.early.lock().unwrap();
        let Some(e) = early.get_mut(&id) else {
            return false;
        };
        let real = e.real;
        let mut shown = None;
        match p {
            Progress::Started { path, partial, total, .. } => {
                e.file = Some(partial.clone().unwrap_or_else(|| path.clone()));
                e.target = Some(path.clone());
                e.total = *total;
                shown = Some((0, *total));
            }
            Progress::Advanced { done, total, .. } => {
                e.total = total.or(e.total);
                shown = Some((*done, *total));
            }
            Progress::Finished { path, .. } => e.end = Some(Some(path.clone())),
            Progress::Failed { .. } | Progress::Canceled { .. } | Progress::NameTaken { .. } => e.end = Some(None),
        }
        if e.discard && e.end.is_some() {
            if let Some(gone) = early.remove(&id) {
                let _ = std::fs::remove_dir_all(gone.dir);
            }
            return true;
        }
        drop(early);
        if let Some((done, total)) = shown {
            let _ = self.update(real, |e| {
                if e.status != Status::Confirming {
                    return Err("answered".into());
                }
                e.done = done;
                e.total = total.or(e.total);
                Ok(())
            });
        }
        true
    }

    /// The engine id of download `real`'s early download, if it has one.
    fn early_of(&self, real: u64) -> Option<u64> {
        self.early.lock().unwrap().iter().find(|(_, e)| e.real == real).map(|(id, _)| *id)
    }

    fn extension_connected(&self) -> bool {
        let windows = self.windows.lock().unwrap().len();
        self.clients.lock().unwrap().len() > windows
    }

    fn window_open(&self) -> bool {
        !self.windows.lock().unwrap().is_empty()
    }

    /// Send `msg` to the desktop windows only.
    fn to_windows(&self, msg: &Outgoing) {
        let Ok(frame) = serde_json::to_vec(msg) else {
            return;
        };
        let windows = self.windows.lock().unwrap();
        for (_, tx) in self.clients.lock().unwrap().iter().filter(|(id, _)| windows.contains(id)) {
            let _ = tx.send(frame.clone());
        }
    }

    /// Whether a browser extension is connected, and when one last was.
    fn browser_link(&self) -> Outgoing {
        Outgoing::BrowserLink {
            connected: !self.browsers.lock().unwrap().is_empty(),
            last_seen: *self.browser_seen.lock().unwrap(),
        }
    }

    /// Note that client `id` is a browser extension; the windows hear of
    /// it when it is the only one.
    fn browser_connected(&self, id: u64) {
        *self.browser_seen.lock().unwrap() = Some(uidm::state::unix_now());
        let first = {
            let mut browsers = self.browsers.lock().unwrap();
            browsers.insert(id) && browsers.len() == 1
        };
        if first {
            self.to_windows(&self.browser_link());
        }
    }

    /// Forget client `id`; when it was the last browser extension, the
    /// windows hear when it went.
    fn client_gone(&self, id: u64) {
        self.clients.lock().unwrap().remove(&id);
        self.windows.lock().unwrap().remove(&id);
        let last = {
            let mut browsers = self.browsers.lock().unwrap();
            browsers.remove(&id) && browsers.is_empty()
        };
        if last {
            *self.browser_seen.lock().unwrap() = Some(uidm::state::unix_now());
            self.to_windows(&self.browser_link());
        }
    }

    /// Send every queue to every client, and save them.
    fn publish_queues(&self) {
        let items = self.queues.lock().unwrap().clone();
        let _ = self.dirty.send(());
        self.broadcast(&Outgoing::Queues { items });
    }

    /// Change queue `id` with `f`, then announce the queues.
    fn update_queue<T>(&self, id: u64, f: impl FnOnce(&mut Queue) -> Result<T, String>) -> Result<T, String> {
        let result = {
            let mut queues = self.queues.lock().unwrap();
            let q = queues.iter_mut().find(|q| q.id == id).ok_or_else(|| format!("no queue with id {id}"))?;
            f(q)?
        };
        self.publish_queues();
        let _ = self.wake.send(());
        Ok(result)
    }

    /// Take download `id` out of whatever queue holds it.
    fn leave_queues(&self, id: u64) -> bool {
        self.leave_queues_where(id, |_| true)
    }

    /// Take download `id` out of the queues `which` picks.
    fn leave_queues_where(&self, id: u64, which: impl Fn(&Queue) -> bool) -> bool {
        let mut changed = false;
        for q in self.queues.lock().unwrap().iter_mut().filter(|q| which(q)) {
            let before = q.order.len();
            q.order.retain(|x| *x != id);
            changed |= q.order.len() != before;
        }
        changed
    }

    /// The queue holding download `id`.
    fn queue_of(&self, id: u64) -> Option<u64> {
        self.queues.lock().unwrap().iter().find(|q| q.order.contains(&id)).map(|q| q.id)
    }

    /// Send `msg` to every client.
    fn broadcast(&self, msg: &Outgoing) {
        let Ok(frame) = serde_json::to_vec(msg) else {
            return;
        };
        // Dropping a client whose writer is gone keeps the map tidy.
        self.clients.lock().unwrap().retain(|_, tx| tx.send(frame.clone()).is_ok());
    }
}

/// Where the daemon keeps its files. A path is `None` when its environment
/// variable is missing; that data then lives in memory only.
struct Paths {
    settings: Option<PathBuf>,
    history: Option<PathBuf>,
    requests: Option<PathBuf>,
    quota: Option<PathBuf>,
}

impl Paths {
    fn from_env() -> Self {
        let warn =
            |what: &str, r: io::Result<PathBuf>| r.map_err(|e| eprintln!("[uidm-daemon] no {what} file: {e}")).ok();
        Paths {
            settings: warn("settings", settings::settings_path()),
            history: warn("history", history::history_path()),
            requests: warn("saved requests", history::requests_path()),
            quota: warn("quota", uidm::quota::quota_path()),
        }
    }
}

struct Daemon {
    engine: Engine,
    hub: Arc<Hub>,
    settings: Mutex<Settings>,
    paths: Paths,
    /// Keeps the history writer and the exit flush from writing at once.
    save_lock: Mutex<()>,
    /// Downloads stopped by `pause_all`, for `resume_all`.
    paused: Mutex<HashSet<u64>>,
    /// Recent notifications; dropping one can stop its buttons working.
    toasts: Mutex<VecDeque<notify::Shown>>,
    /// Queues stopped by `pause_all`, for `resume_all`.
    paused_queues: Mutex<HashSet<u64>>,
    /// What each started queue has started in its current run.
    runs: Mutex<HashMap<u64, HashSet<u64>>>,
    /// Retries each download got from its queue in the current run.
    attempts: Mutex<HashMap<u64, u32>>,
    /// Finished downloads a synchronizing queue is checking with the server.
    checking: Mutex<HashSet<u64>>,
    /// The countdown before a power action, while one runs.
    countdown: Mutex<Option<Countdown>>,
    /// Keeps two scheduler passes from running at once.
    tick_lock: Mutex<()>,
    /// The download waiting for a new link from the browser, if any.
    refresh_waiting: Mutex<Option<u64>>,
    /// Each client's running site crawl, by its stop flag.
    grabs: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    /// Each client's link picker probes, by their stop flag.
    link_probes: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    /// Links from a page, held for the window that is being opened.
    picked_links: Mutex<Option<Outgoing>>,
    /// A command line held for the window its `uidm-gui` is opening, and
    /// when it came.
    command_line: Mutex<Option<(CommandLine, Instant)>>,
    /// What the quota stopped (queues and downloads), while it holds them.
    quota_paused: Mutex<Option<(Vec<u64>, HashSet<u64>)>>,
    /// The nearly-reached warning was sent for the current window.
    quota_warned: AtomicBool,
    /// Crawls waiting for the browser's cookies, by request.
    cookie_waits: Mutex<HashMap<u64, Sender<Option<String>>>>,
    /// Crawls waiting for a page the browser renders: its HTML and address.
    render_waits: Mutex<HashMap<u64, Sender<Rendered>>>,
    /// Grabber projects whose scheduled run is going on, by name.
    grab_runs: Mutex<HashSet<String>>,
    /// Running conversions, by their row's id, with a flag to stop them.
    conversions: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    /// When the projects' schedules were last looked at.
    grab_checked: Mutex<Option<Instant>>,
    /// Addresses retried through the browser's proxy after failing through
    /// UIDM's own proxy setting; they keep using it.
    browser_proxy_retries: Mutex<HashSet<String>>,
}

/// A countdown before sleeping, hibernating or shutting down.
struct Countdown {
    action: WhenDone,
    queue: String,
    /// The download's file name, when one download asked for it.
    file: Option<String>,
    force_close: bool,
    deadline: Instant,
    canceled: Arc<AtomicBool>,
}

impl Daemon {
    /// Load settings, history and saved requests, and start the engine with
    /// them. Returns the receivers the history writer and the notification
    /// thread wait on.
    fn new(paths: Paths) -> (Self, Receiver<()>, Receiver<Note>, Receiver<()>) {
        let mut settings = match &paths.settings {
            Some(path) => Settings::load(path).unwrap_or_else(|e| {
                eprintln!("[uidm-daemon] ignoring {}: {e}", path.display());
                Settings::default()
            }),
            None => Settings::default(),
        };
        // Settings from before each proxy had its own login: the HTTPS and
        // SOCKS proxies keep the shared password they used.
        if std::mem::take(&mut settings.proxy_logins_split) {
            let saved = net::split_proxy_password()
                .map_err(|e| e.to_string())
                .and_then(|()| paths.settings.as_ref().map_or(Ok(()), |p| settings.save(p).map_err(|e| e.to_string())));
            if let Err(e) = saved {
                eprintln!("[uidm-daemon] could not give each proxy its own login: {e}");
            }
        }
        // IDM's "Always turn on Speed Limiter on startup": the limit is on
        // after every start, even if it was turned off last time.
        if settings.speed_limit_on_startup {
            settings.speed_limit_enabled = true;
        }
        let history = match &paths.history {
            Some(path) => History::load(path).unwrap_or_else(|e| {
                // Keep the unreadable file for inspection instead of
                // overwriting it with an empty list.
                let bad = path.with_extension("json.bad");
                eprintln!("[uidm-daemon] could not read {}: {e}; moved to {}", path.display(), bad.display());
                let _ = std::fs::rename(path, &bad);
                History::default()
            }),
            None => History::default(),
        };
        let mut requests = match &paths.requests {
            Some(path) => history::load_requests(path).unwrap_or_else(|e| {
                eprintln!("[uidm-daemon] could not read saved requests: {e}");
                HashMap::new()
            }),
            None => HashMap::new(),
        };

        let mut table = DownloadTable::new();
        let mut queues = history.queues;
        if queues.is_empty() {
            queues.push(Queue::main());
        }
        table.apply(&Outgoing::Downloads { items: history.items });
        requests.retain(|id, _| table.get(*id).is_some_and(|e| e.status.is_retryable()));

        let (dirty, dirty_rx) = channel();
        let (notes, notes_rx) = channel();
        let (wake, wake_rx) = channel();
        let hub = Arc::new(Hub {
            clients: Mutex::new(HashMap::new()),
            table: Mutex::new(table),
            requests: Mutex::new(requests),
            dirty,
            windows: Mutex::new(HashSet::new()),
            browsers: Mutex::new(HashSet::new()),
            browser_seen: Mutex::new(None),
            notes,
            queues: Mutex::new(queues),
            held: Mutex::new(HashSet::new()),
            quota: Mutex::new(paths.quota.as_deref().map(Usage::load).unwrap_or_default()),
            uncounted: Mutex::new(HashSet::new()),
            early: Mutex::new(HashMap::new()),
            wake,
        });
        let events = Arc::clone(&hub);
        let engine = Engine::new(
            Arc::new(move |p: Progress| {
                if !events.early_event(&p) {
                    events.publish(&progress_to_outgoing(p));
                }
            }),
            settings.max_concurrent,
        );
        engine.set_speed_limit(settings.speed_limit_bytes());
        engine.set_per_file_limit(settings.per_file_limit_bytes());
        let max_id = hub.table.lock().unwrap().max_id();
        engine.set_next_id(history.next_id.max(max_id + 1));

        let daemon = Daemon {
            engine,
            hub,
            settings: Mutex::new(settings),
            paths,
            save_lock: Mutex::new(()),
            paused: Mutex::new(HashSet::new()),
            toasts: Mutex::new(VecDeque::new()),
            paused_queues: Mutex::new(HashSet::new()),
            runs: Mutex::new(HashMap::new()),
            attempts: Mutex::new(HashMap::new()),
            checking: Mutex::new(HashSet::new()),
            countdown: Mutex::new(None),
            tick_lock: Mutex::new(()),
            refresh_waiting: Mutex::new(None),
            grabs: Mutex::new(HashMap::new()),
            link_probes: Mutex::new(HashMap::new()),
            picked_links: Mutex::new(None),
            command_line: Mutex::new(None),
            quota_paused: Mutex::new(None),
            quota_warned: AtomicBool::new(false),
            cookie_waits: Mutex::new(HashMap::new()),
            render_waits: Mutex::new(HashMap::new()),
            grab_runs: Mutex::new(HashSet::new()),
            conversions: Mutex::new(HashMap::new()),
            grab_checked: Mutex::new(None),
            browser_proxy_retries: Mutex::new(HashSet::new()),
        };
        (daemon, dirty_rx, notes_rx, wake_rx)
    }

    /// Start a `waiting` (or `confirming`) download now. A download that ran
    /// before resumes its own file; one that never ran goes to the file chosen
    /// for it, if any.
    fn dispatch(&self, id: u64) -> Result<(), String> {
        let download = self.hub.requests.lock().unwrap().get(&id).cloned();
        let mut plan = (None, false);
        let item = self.hub.update(id, |e| {
            if !matches!(e.status, Status::Waiting | Status::Confirming) {
                return Err(format!("download {id} is not waiting to be started"));
            }
            plan = (e.path.clone(), e.connections > 0);
            e.status = Status::Queued;
            Ok(())
        })?;
        let download = download.unwrap_or_else(|| Download {
            url: item.url.clone(),
            referrer: item.referrer.clone(),
            page: item.page.clone(),
            ..Download::default()
        });
        let mut req = self.request_for(download);
        req.speed_limit = item.speed_limit;
        req.reuse_partial = item.partial.as_deref().map(PathBuf::from);
        match plan {
            (Some(path), true) => {
                let path = PathBuf::from(path);
                // A finished file whose server copy changed (see
                // `check_for_change`): nothing of the new copy is here yet,
                // and the file has no resume sidecar. Download beside it and
                // swap it in at the end, so a failed or stopped download
                // leaves the old file whole.
                let replacing = item.done == 0
                    && req.temp_dir.is_none()
                    && req.reuse_partial.is_none()
                    && path.is_file()
                    && !engine::sidecar_path(&path).exists();
                if replacing {
                    req.temp_dir = path.parent().map(Path::to_path_buf);
                }
                req.reuse_path = Some(path);
            }
            (Some(path), false) => {
                let path = PathBuf::from(path);
                if let (Some(name), Some(dir)) = (path.file_name(), path.parent()) {
                    req.filename = Some(name.to_string_lossy().into_owned());
                    req.folders = Folders::fixed(dir.to_path_buf());
                }
            }
            (None, _) => {}
        }
        self.engine.enqueue_as(id, req);
        Ok(())
    }

    /// Put download `id` in queue `queue` at `position` (or at the end). A
    /// download that is not running waits there.
    fn move_to_queue(&self, id: u64, queue: u64, position: Option<usize>) -> Result<(), String> {
        let status = self.status(id).ok_or_else(|| format!("no download with id {id}"))?;
        let sync = {
            let queues = self.hub.queues.lock().unwrap();
            let q = queues.iter().find(|q| q.id == queue).ok_or_else(|| format!("no queue with id {queue}"))?;
            q.kind.is_sync()
        };
        // A synchronizing queue also keeps finished downloads, to check.
        if status == Status::Confirming || (status == Status::Done && !sync) {
            return Err("Only a download that has not finished can wait in a queue.".to_string());
        }
        // Joining a queue (not just moving within it) makes a stopped download
        // wait to run again.
        let reorder = position.is_some() && self.hub.queue_of(id) == Some(queue);
        if status.is_retryable() && status != Status::Waiting && !reorder {
            self.hub.update(id, |e| {
                e.status = Status::Waiting;
                Ok(())
            })?;
        }
        self.hub.leave_queues(id);
        self.hub.update_queue(queue, |q| {
            let at = position.unwrap_or(q.order.len()).min(q.order.len());
            q.order.insert(at, id);
            Ok(())
        })
    }

    /// Take download `id` out of its queue, keeping it in the list. Nothing
    /// puts it back by itself (see `History::tidy_queues`), so no queue
    /// starts it until it is started by hand or moved to a queue again.
    fn remove_from_queue(&self, id: u64) -> Result<(), String> {
        self.status(id).ok_or_else(|| format!("no download with id {id}"))?;
        if !self.hub.leave_queues(id) {
            return Err("That download is not in a queue.".to_string());
        }
        self.hub.publish_queues();
        Ok(())
    }

    /// Create a queue (id 0) or change one. Its downloads and running state
    /// are kept; only what the user edits changes.
    fn save_queue(&self, edited: Queue) -> Result<u64, String> {
        edited.validate()?;
        let name = edited.name.trim().to_string();
        let finished = self.finished_ids();
        let id = {
            let mut queues = self.hub.queues.lock().unwrap();
            if queues.iter().any(|q| q.id != edited.id && q.name.eq_ignore_ascii_case(&name)) {
                return Err(format!("There is already a queue named \"{name}\"."));
            }
            let id = if edited.id == 0 {
                let id = queues.iter().map(|q| q.id).max().unwrap_or(MAIN_QUEUE) + 1;
                queues.push(Queue::new(id, &name));
                id
            } else {
                edited.id
            };
            let q = queues.iter_mut().find(|q| q.id == id).ok_or_else(|| format!("no queue with id {id}"))?;
            if q.schedule != edited.schedule && !q.running {
                // A new schedule may start today even if the old one ran. A
                // run in progress keeps its day, so its stop time still holds.
                q.last_scheduled_day = None;
                q.last_scheduled_start = None;
            }
            q.name = name;
            q.max_concurrent = edited.max_concurrent;
            q.schedule = edited.schedule;
            q.when_done = edited.when_done;
            q.retries = edited.retries;
            q.open_when_done = edited.open_when_done.map(|f| f.trim().to_string()).filter(|f| !f.is_empty());
            q.start_on_launch = edited.start_on_launch;
            q.force_close = edited.force_close;
            q.kind = edited.kind;
            // A one-time queue holds only what is still to do.
            if !q.kind.is_sync() {
                q.order.retain(|d| !finished.contains(d));
            }
            id
        };
        self.hub.publish_queues();
        let _ = self.hub.wake.send(());
        Ok(id)
    }

    /// Delete a queue. Its downloads move to the main queue, which cannot be
    /// deleted; the running ones keep running.
    fn delete_queue(&self, id: u64) -> Result<(), String> {
        if id == MAIN_QUEUE {
            return Err("The main queue cannot be deleted.".to_string());
        }
        let finished = self.finished_ids();
        {
            let mut queues = self.hub.queues.lock().unwrap();
            let pos = queues.iter().position(|q| q.id == id).ok_or_else(|| format!("no queue with id {id}"))?;
            let gone = queues.remove(pos);
            if let Some(main) = queues.iter_mut().find(|q| q.id == MAIN_QUEUE) {
                let keep_done = main.kind.is_sync();
                main.order.extend(gone.order.into_iter().filter(|d| keep_done || !finished.contains(d)));
            }
        }
        self.runs.lock().unwrap().remove(&id);
        self.hub.publish_queues();
        Ok(())
    }

    /// Start a queue: its waiting downloads run in order.
    fn start_queue(&self, id: u64, scheduled_day: Option<i64>) -> Result<(), String> {
        let was_running = self.hub.update_queue(id, |q| {
            let was = q.running;
            q.running = true;
            q.started_by_schedule = scheduled_day.is_some();
            if scheduled_day.is_some() {
                q.last_scheduled_day = scheduled_day;
                q.last_scheduled_start = Some(queue::local_now().total());
            }
            Ok(was)
        })?;
        self.runs.lock().unwrap().insert(id, HashSet::new());
        if !was_running {
            let settings = self.settings.lock().unwrap().clone();
            if settings.sound_on_queue_started {
                notify::play(Sound::QueueStarted, settings.sound_file(Sound::QueueStarted).as_deref());
            }
        }
        Ok(())
    }

    /// Stop a queue. Its running downloads stop and wait in it again, with
    /// their progress kept.
    fn stop_queue(&self, id: u64) -> Result<(), String> {
        let order = self.hub.update_queue(id, |q| {
            q.running = false;
            q.started_by_schedule = false;
            q.when_done_once = None;
            q.open_when_done_once = None;
            q.force_close_once = None;
            q.stop_at_once = None;
            Ok(q.order.clone())
        })?;
        self.runs.lock().unwrap().remove(&id);
        let running: Vec<u64> = {
            let table = self.hub.table.lock().unwrap();
            order
                .into_iter()
                .filter(|i| table.get(*i).is_some_and(|e| matches!(e.status, Status::Queued | Status::Downloading)))
                .collect()
        };
        for i in running {
            self.hub.held.lock().unwrap().insert(i);
            self.engine.cancel(i);
        }
        Ok(())
    }

    /// One pass of the scheduler: start and stop queues on their schedules,
    /// start waiting downloads in started queues, retry failed ones, and end
    /// the runs that are done.
    fn tick(self: &Arc<Self>) {
        let _guard = self.tick_lock.lock().unwrap();
        self.check_quota();
        let now = queue::local_now();

        let queues = self.hub.queues.lock().unwrap().clone();
        for q in &queues {
            // A run a grabber project started ends at the project's stop time.
            if q.running && q.stop_at_once.is_some_and(|end| now.total() >= end) {
                eprintln!("[uidm-daemon] the grabber project's stop time stops queue {}", q.name);
                let _ = self.stop_queue(q.id);
                continue;
            }
            let Some(schedule) = &q.schedule else {
                continue;
            };
            if let Some(day) = schedule.due(now, q.last_scheduled_day, q.last_scheduled_start) {
                if !q.running {
                    eprintln!("[uidm-daemon] schedule starts queue {}", q.name);
                    let _ = self.start_queue(q.id, Some(day));
                }
            }
            // At the end of its time a scheduled run stops: always when it
            // has a stop time, and without one only if it never started
            // anything (otherwise it runs until it is done).
            let started_any = !self.runs.lock().unwrap().get(&q.id).is_none_or(|r| r.is_empty());
            if q.running && q.started_by_schedule && (schedule.stop.is_some() || !started_any) {
                let ended = q.last_scheduled_day.and_then(|day| schedule.end_of_run(day)).is_some_and(|end| now >= end);
                if ended {
                    eprintln!("[uidm-daemon] schedule stops queue {}", q.name);
                    let _ = self.stop_queue(q.id);
                }
            }
        }

        let queues = self.hub.queues.lock().unwrap().clone();
        for q in queues.iter().filter(|q| q.running) {
            let statuses: Vec<(u64, Status)> = {
                let table = self.hub.table.lock().unwrap();
                q.order.iter().filter_map(|id| table.get(*id).map(|e| (*id, e.status))).collect()
            };
            let started = self.runs.lock().unwrap().entry(q.id).or_default().clone();

            // Failed in this run and retries left: wait again.
            for (id, status) in &statuses {
                if *status == Status::Failed && started.contains(id) {
                    let mut attempts = self.attempts.lock().unwrap();
                    let n = attempts.entry(*id).or_insert(0);
                    if *n < q.retries {
                        *n += 1;
                        eprintln!("[uidm-daemon] queue {} retries #{id} ({}/{})", q.name, n, q.retries);
                        drop(attempts);
                        let _ = self.hub.update(*id, |e| {
                            e.status = Status::Waiting;
                            Ok(())
                        });
                    }
                }
            }
            let statuses: Vec<(u64, Status)> = {
                let table = self.hub.table.lock().unwrap();
                q.order.iter().filter_map(|id| table.get(*id).map(|e| (*id, e.status))).collect()
            };

            let checking = self.checking.lock().unwrap().clone();
            let mut active = statuses
                .iter()
                .filter(|(id, s)| matches!(s, Status::Queued | Status::Downloading) || checking.contains(id))
                .count();
            for (id, status) in &statuses {
                if active >= q.max_concurrent {
                    break;
                }
                if *status == Status::Waiting && self.dispatch(*id).is_ok() {
                    self.runs.lock().unwrap().entry(q.id).or_default().insert(*id);
                    active += 1;
                }
                // A synchronizing run checks each finished download once,
                // as many at a time as it downloads.
                let unchecked = || !self.runs.lock().unwrap().get(&q.id).is_some_and(|r| r.contains(id));
                if q.kind.is_sync() && *status == Status::Done && !checking.contains(id) && unchecked() {
                    self.runs.lock().unwrap().entry(q.id).or_default().insert(*id);
                    self.checking.lock().unwrap().insert(*id);
                    active += 1;
                    let (daemon, id) = (Arc::clone(self), *id);
                    thread::spawn(move || {
                        let _ = daemon.check_for_change(id);
                        daemon.checking.lock().unwrap().remove(&id);
                        let _ = daemon.hub.wake.send(());
                    });
                }
            }

            // A download asking about a taken file name is still to do.
            let busy = active > 0 || statuses.iter().any(|(_, s)| matches!(s, Status::Waiting | Status::Confirming));
            // A scheduled run that has started nothing yet stays open until
            // its stop time, for downloads added meanwhile.
            let started_any = !self.runs.lock().unwrap().get(&q.id).is_none_or(|r| r.is_empty());
            if !busy && (started_any || !q.started_by_schedule) {
                self.finish_run(q.id);
            }
        }
    }

    /// The downloads that have finished.
    fn finished_ids(&self) -> HashSet<u64> {
        let table = self.hub.table.lock().unwrap();
        table.entries().filter(|e| e.status == Status::Done).map(|e| e.id).collect()
    }

    /// Ask the server about finished download `id` and, when its copy
    /// changed since (by size, ETag or Last-Modified, see
    /// `Validator::changed`) or the file is gone, make it wait to be
    /// downloaded again in place: same row, same file, which `dispatch`
    /// replaces only once the new copy is complete. Returns whether it
    /// changed. A check that fails leaves the download and its file alone,
    /// with a note saying why.
    fn check_for_change(&self, id: u64) -> Result<bool, String> {
        let entry = self.entry(id).ok_or_else(|| format!("no download with id {id}"))?;
        if entry.status != Status::Done {
            return Err(format!("download {id} has not finished"));
        }
        let found = self.probe_finished(&entry);
        let found = match found {
            Ok(found) => found,
            Err(e) => {
                eprintln!("[uidm-daemon] could not check #{id} for changes: {e}");
                let note = format!("Could not check the server for a newer copy: {e}");
                let _ = self.hub.update(id, |e| {
                    if e.status != Status::Done {
                        return Err("started meanwhile".into());
                    }
                    e.note = Some(note);
                    Ok(())
                });
                return Err(e);
            }
        };
        let known = entry.validator.clone().unwrap_or(Validator { size: entry.total, ..Validator::default() });
        let gone = !entry.path.as_deref().is_some_and(|p| Path::new(p).is_file());
        let changed = gone || known.changed(&found);
        self.hub.update(id, |e| {
            if e.status != Status::Done {
                return Err("The download started again in the meantime.".into());
            }
            e.note = None;
            if changed {
                // Its own file, to download over (see `dispatch`). The
                // validator stays the old one until the new copy starts, so
                // a download that fails is found changed again next time.
                e.status = Status::Waiting;
                e.done = 0;
                e.parts = None;
                e.error = None;
                e.scan = None;
                e.partial = None;
                e.connections = e.connections.max(1);
            } else {
                // Learn what the server says now, such as an ETag it did
                // not send before.
                e.validator = Some(found);
            }
            Ok(())
        })?;
        eprintln!(
            "[uidm-daemon] #{id} {}: {}",
            if changed { "changed on the server" } else { "is unchanged" },
            entry.url
        );
        Ok(changed)
    }

    /// What the server says now about finished download `entry`'s file.
    fn probe_finished(&self, entry: &DownloadEntry) -> Result<Validator, String> {
        if entry.path.is_none() {
            return Err("it has no file".into());
        }
        let download = self.hub.requests.lock().unwrap().get(&entry.id).cloned().unwrap_or_else(|| Download {
            url: entry.url.clone(),
            referrer: entry.referrer.clone(),
            page: entry.page.clone(),
            ..Download::default()
        });
        if entry.parts.is_some() || download.media.is_some() || download.body.is_some() {
            return Err("a stream or a form's answer cannot be checked".into());
        }
        let req = self.request_for(download);
        let found = engine::inspect(&req.url, None, &req.headers, &req.net).map_err(|e| e.to_string())?;
        Ok(Validator { size: found.total, etag: found.etag, last_modified: found.last_modified })
    }

    /// Check the finished downloads at `urls` for changes, and put those
    /// whose server copy changed in `queue` to be downloaded again, starting
    /// it. Returns how many changed, and the addresses it found finished in
    /// the list (which need no new row).
    fn resync(&self, urls: &[String], queue: u64) -> (usize, HashSet<String>) {
        let urls: HashSet<&str> = urls.iter().map(String::as_str).collect();
        let (ids, listed): (Vec<u64>, HashSet<String>) = {
            let table = self.hub.table.lock().unwrap();
            table
                .entries()
                .filter(|e| e.status == Status::Done && urls.contains(e.url.as_str()))
                .map(|e| (e.id, e.url.clone()))
                .unzip()
        };
        let changed = ids
            .into_iter()
            .filter(|id| matches!(self.check_for_change(*id), Ok(true)) && self.move_to_queue(*id, queue, None).is_ok())
            .count();
        if changed > 0 && !self.hub.queues.lock().unwrap().iter().any(|q| q.id == queue && q.running) {
            if let Err(e) = self.start_queue(queue, None) {
                eprintln!("[uidm-daemon] could not start queue {queue}: {e}");
            }
        }
        (changed, listed)
    }

    /// A started queue has nothing left to do: stop it and, if it started
    /// anything, do what it asks when done.
    fn finish_run(&self, id: u64) {
        let started = self.runs.lock().unwrap().remove(&id).unwrap_or_default();
        {
            let mut attempts = self.attempts.lock().unwrap();
            for d in &started {
                attempts.remove(d);
            }
        }
        let Ok((name, action, open, force_close)) = self.hub.update_queue(id, |q| {
            q.running = false;
            q.started_by_schedule = false;
            q.stop_at_once = None;
            let action = q.when_done_once.take().unwrap_or(q.when_done);
            let open = q.open_when_done_once.take().or_else(|| q.open_when_done.clone());
            let force_close = q.force_close_once.take().unwrap_or(q.force_close);
            Ok((q.name.clone(), action, open, force_close))
        }) else {
            return;
        };
        eprintln!("[uidm-daemon] queue {name} is done");
        if !started.is_empty() {
            let settings = self.settings.lock().unwrap().clone();
            if settings.sound_on_queue_done {
                notify::play(Sound::QueueDone, settings.sound_file(Sound::QueueDone).as_deref());
            }
            if let Some(file) = open {
                open_after_run(&file);
            }
            self.when_done(action, &name, None, force_close);
        }
    }

    /// Do what a finished queue, or with `file` one finished download, asks.
    fn when_done(&self, action: WhenDone, queue: &str, file: Option<&str>, force_close: bool) {
        match action {
            WhenDone::Nothing => {}
            WhenDone::Exit => {
                eprintln!("[uidm-daemon] {} done: exiting", file.unwrap_or(queue));
                self.hub.broadcast(&Outgoing::Quit);
                self.save_history();
                // Other downloads may still run; the idle monitor ends the
                // daemon once they are done and every window has closed.
            }
            _ => self.start_countdown(action, queue, file, force_close),
        }
    }

    /// Run what download `id` asked to happen once it finished, then clear
    /// it so downloading it again does not repeat it.
    fn after_download(&self, id: u64) {
        let mut then = OnDone::default();
        let cleared = self.hub.update(id, |e| {
            if e.status != Status::Done {
                return Err("not finished".into());
            }
            then = e.on_done.take().ok_or_else(|| "nothing to do".to_string())?;
            Ok(())
        });
        if let Ok(e) = cleared {
            let name =
                e.path.as_deref().and_then(|p| Path::new(p).file_name()).map(|n| n.to_string_lossy().into_owned());
            self.when_done(then.then, "", Some(name.as_deref().unwrap_or(&e.url)), then.force_close);
        }
    }

    /// Do what `uidm-gui`'s command line asked. `/q` counts only when that
    /// call opened the window (`started`), as in IDM, where it works only
    /// for the first copy. Problems go to the windows, since the caller may
    /// be gone.
    fn run_command_line(self: &Arc<Self>, cmd: CommandLine, started: bool) {
        if let Some(mut download) = cmd.download {
            download.limit_browser_request();
            let (id, url) = self.add(download);
            eprintln!("[uidm-daemon] add #{id} from the command line: {url}");
            if cmd.quit_after && started {
                let exit = OnDone { then: WhenDone::Exit, ..OnDone::default() };
                if let Err(e) = self.set_on_done(id, Some(exit)) {
                    eprintln!("[uidm-daemon] /q for #{id}: {e}");
                }
            }
        }
        if cmd.start_queue {
            match self.start_queue(MAIN_QUEUE, None) {
                Ok(()) => {
                    eprintln!("[uidm-daemon] start the main queue from the command line");
                    let _ = self.hub.wake.send(());
                }
                Err(message) => self.hub.to_windows(&Outgoing::Error { message }),
            }
        }
    }

    /// Set or clear what download `id` does once it finishes.
    fn set_on_done(&self, id: u64, on_done: Option<OnDone>) -> Result<(), String> {
        self.hub.update(id, |e| {
            if e.status == Status::Done {
                return Err("This download has already finished.".into());
            }
            e.on_done = on_done.filter(|o| *o != OnDone::default());
            Ok(())
        })?;
        Ok(())
    }

    fn start_countdown(&self, action: WhenDone, queue: &str, file: Option<&str>, force_close: bool) {
        let canceled = Arc::new(AtomicBool::new(false));
        let deadline = Instant::now() + Duration::from_secs(COUNTDOWN_SECS as u64);
        if let Some(old) = self.countdown.lock().unwrap().replace(Countdown {
            action,
            queue: queue.to_string(),
            file: file.map(str::to_string),
            force_close,
            deadline,
            canceled: Arc::clone(&canceled),
        }) {
            old.canceled.store(true, Ordering::Relaxed);
        }
        eprintln!("[uidm-daemon] {} done: {} in {COUNTDOWN_SECS}s", file.unwrap_or(queue), action.label());
        self.hub.broadcast(&Outgoing::Countdown {
            action,
            seconds: COUNTDOWN_SECS,
            queue: queue.to_string(),
            file: file.map(str::to_string),
        });
        self.show_window();
        // Save now: the machine may go away when the countdown ends.
        self.save_history();
        thread::spawn(move || {
            while Instant::now() < deadline {
                if canceled.load(Ordering::Relaxed) {
                    return;
                }
                thread::sleep(Duration::from_millis(250));
            }
            if canceled.load(Ordering::Relaxed) {
                return;
            }
            power_off(action, force_close);
        });
    }

    /// End the countdown early and carry out its action now.
    fn countdown_now(&self) {
        let Some(c) = self.countdown.lock().unwrap().take() else {
            return;
        };
        c.canceled.store(true, Ordering::Relaxed);
        self.hub.broadcast(&Outgoing::CountdownCanceled);
        power_off(c.action, c.force_close);
    }

    fn cancel_countdown(&self) {
        if let Some(c) = self.countdown.lock().unwrap().take() {
            c.canceled.store(true, Ordering::Relaxed);
            eprintln!("[uidm-daemon] countdown canceled");
            self.hub.broadcast(&Outgoing::CountdownCanceled);
        }
    }

    /// The countdown still running, as the message a window joining now needs.
    fn countdown_message(&self) -> Option<Outgoing> {
        let mut slot = self.countdown.lock().unwrap();
        let c = slot.as_ref()?;
        if c.canceled.load(Ordering::Relaxed) || Instant::now() >= c.deadline {
            *slot = None;
            return None;
        }
        Some(Outgoing::Countdown {
            action: c.action,
            seconds: c.deadline.saturating_duration_since(Instant::now()).as_secs() as u32,
            queue: c.queue.clone(),
            file: c.file.clone(),
        })
    }

    /// Whether work is running that sleep would interrupt: a download, a
    /// site grab or a conversion.
    fn is_working(&self) -> bool {
        self.engine.active_count() > 0
            || !self.grabs.lock().unwrap().is_empty()
            || !self.grab_runs.lock().unwrap().is_empty()
            || !self.conversions.lock().unwrap().is_empty()
    }

    /// Whether the daemon must stay up for a queue: one is started, one has a
    /// schedule ahead, or a countdown runs.
    fn queues_need_daemon(&self) -> bool {
        let now = queue::local_now();
        let projects = || {
            let Ok(dir) = grab::projects_dir() else {
                return false;
            };
            !self.grab_runs.lock().unwrap().is_empty()
                || grab::recent_projects(&dir).iter().any(|name| {
                    grab::Project::load(&dir, name)
                        .ok()
                        .and_then(|p| p.schedule)
                        .is_some_and(|s| s.next_start(now).is_some() || s.open_run(now).is_some())
                })
        };
        self.countdown_message().is_some()
            || projects()
            || self.hub.queues.lock().unwrap().iter().any(|q| {
                q.running
                    || q.schedule.as_ref().is_some_and(|s| s.next_start(now).is_some() || s.open_run(now).is_some())
            })
    }

    fn entry(&self, id: u64) -> Option<DownloadEntry> {
        self.hub.table.lock().unwrap().get(id).cloned()
    }

    /// Bring the window forward: ask an open one to show itself, or start one.
    fn show_window(&self) {
        if self.hub.window_open() {
            self.hub.broadcast(&Outgoing::Show);
        } else if let Err(e) = open_window() {
            eprintln!("[uidm-daemon] could not open the window: {e}");
        }
    }

    /// Stop every queued or running download, remembering them for
    /// `resume_all`.
    fn pause_all(&self) {
        let (queues, ids) = self.stop_running();
        self.paused_queues.lock().unwrap().extend(queues);
        self.paused.lock().unwrap().extend(ids);
    }

    /// Stop every started queue and every queued or running download,
    /// keeping their progress. Returns what was stopped.
    fn stop_running(&self) -> (Vec<u64>, Vec<u64>) {
        let running: Vec<u64> = self.hub.queues.lock().unwrap().iter().filter(|q| q.running).map(|q| q.id).collect();
        for q in &running {
            let _ = self.stop_queue(*q);
        }
        let ids: Vec<u64> = self
            .hub
            .table
            .lock()
            .unwrap()
            .entries()
            .filter(|e| matches!(e.status, Status::Queued | Status::Downloading))
            .map(|e| e.id)
            .collect();
        for id in &ids {
            self.engine.cancel(*id);
        }
        (running, ids)
    }

    /// The quota's state for a window that just connected, if downloads are
    /// paused for it or it is nearly reached.
    fn quota_message(&self) -> Option<Outgoing> {
        let (quota, warn) = {
            let s = self.settings.lock().unwrap();
            (s.quota(), s.quota_warn)
        };
        let (limit, window) = quota?;
        let now = uidm::quota::now_minute();
        let mut usage = self.hub.quota.lock().unwrap();
        let used = usage.used(now, window);
        let paused = self.quota_paused.lock().unwrap().is_some();
        (paused || (warn && used >= limit / 10 * 9)).then(|| Outgoing::Quota {
            used,
            limit,
            paused,
            resumes_at: paused.then(|| usage.frees_at(now, window, limit) * 60),
        })
    }

    /// Pause everything when the download quota is reached, warn shortly
    /// before, and resume what it stopped once the window allows. Runs with
    /// the scheduler.
    fn check_quota(&self) {
        let (quota, warn) = {
            let s = self.settings.lock().unwrap();
            (s.quota(), s.quota_warn)
        };
        let now = uidm::quota::now_minute();
        let (used, limit, window) = match quota {
            Some((limit, window)) => (self.hub.quota.lock().unwrap().used(now, window), limit, window),
            None => (0, u64::MAX, 0),
        };
        let paused = self.quota_paused.lock().unwrap().is_some();
        let msg = |paused: bool, resumes_at: Option<u64>| Outgoing::Quota { used, limit, paused, resumes_at };
        if paused && used < limit {
            let (queues, ids) = self.quota_paused.lock().unwrap().take().unwrap_or_default();
            eprintln!("[uidm-daemon] quota allows downloads again");
            self.quota_warned.store(false, Ordering::Relaxed);
            for q in queues {
                let _ = self.start_queue(q, None);
            }
            for id in ids.into_iter().filter(|id| self.status(*id) == Some(Status::Canceled)) {
                if let Err(e) = self.retry(id) {
                    eprintln!("[uidm-daemon] could not resume #{id}: {e}");
                }
            }
            self.hub.broadcast(&msg(false, None));
        } else if used >= limit {
            // Also stops what started since.
            let (queues, ids) = self.stop_running();
            let first = !paused;
            {
                let mut held = self.quota_paused.lock().unwrap();
                let (q, i) = held.get_or_insert_with(Default::default);
                q.extend(queues);
                i.extend(ids);
            }
            if first {
                let at = self.hub.quota.lock().unwrap().frees_at(now, window, limit) * 60;
                eprintln!("[uidm-daemon] quota reached: {used} of {limit} bytes");
                self.hub.broadcast(&msg(true, Some(at)));
            }
        } else if warn && used >= limit / 10 * 9 && !self.quota_warned.swap(true, Ordering::Relaxed) {
            self.hub.broadcast(&msg(false, None));
        }
    }

    /// Start again what `pause_all` stopped, and every interrupted download.
    fn resume_all(&self) {
        let queues: HashSet<u64> = std::mem::take(&mut *self.paused_queues.lock().unwrap());
        for q in queues {
            let _ = self.start_queue(q, None);
        }
        let paused: HashSet<u64> = std::mem::take(&mut *self.paused.lock().unwrap());
        let ids: Vec<u64> = self
            .hub
            .table
            .lock()
            .unwrap()
            .entries()
            .filter(|e| e.status == Status::Interrupted || (e.status == Status::Canceled && paused.contains(&e.id)))
            .map(|e| e.id)
            .collect();
        for id in ids {
            if let Err(e) = self.retry(id) {
                eprintln!("[uidm-daemon] could not resume #{id}: {e}");
            }
        }
    }

    /// Handle one note: scan a finished download first when asked, then play
    /// the sound and notify.
    fn on_note(self: &Arc<Self>, note: Note) {
        let settings = self.settings.lock().unwrap().clone();
        match note {
            Note::Finished(id) if settings.scan_after_download => {
                let scanning = self.hub.update(id, |e| {
                    if e.status != Status::Done || e.path.is_none() {
                        return Err("not a finished file".into());
                    }
                    e.scan = Some(Scan::Scanning);
                    Ok(())
                });
                let Ok(entry) = scanning else {
                    return;
                };
                let daemon = Arc::clone(self);
                thread::spawn(move || {
                    let path = PathBuf::from(entry.path.unwrap_or_default());
                    let result = scan::scan(settings.scanner, &settings.scanner_command, &path);
                    eprintln!("[uidm-daemon] scan #{id}: {result:?}");
                    let _ = daemon.hub.update(id, |e| {
                        e.scan = Some(result);
                        Ok(())
                    });
                    let _ = daemon.hub.notes.send(Note::Scanned(id));
                });
            }
            Note::Finished(id) | Note::Scanned(id) => {
                self.notify(id, false, &settings);
                self.after_download(id);
            }
            Note::Failed(id) => {
                thread::sleep(FAILED_GRACE);
                if self.entry(id).is_some_and(|e| e.status == Status::Failed)
                    && !self.retry_through_browser_proxy(id, &settings)
                {
                    self.notify(id, true, &settings);
                }
            }
            // Without a window to ask, a taken name gets a new one.
            Note::NameTaken(id) => self.await_answer(id, Some(FileExists::Rename)),
        }
    }

    /// Wait for a window to answer download `id`'s save-as dialog, opening
    /// one if none is open. If none connects within `CONFIRM_FALLBACK`, the
    /// download starts as proposed: `fallback` decides what happens if its
    /// name is taken, `None` leaving that to the setting.
    fn await_answer(self: &Arc<Self>, id: u64, fallback: Option<FileExists>) {
        if !self.hub.window_open() {
            if let Err(e) = open_window() {
                eprintln!("[uidm-daemon] could not open the window: {e}");
            }
        }
        let daemon = Arc::clone(self);
        thread::spawn(move || {
            thread::sleep(CONFIRM_FALLBACK);
            if daemon.status(id) == Some(Status::Confirming) && !daemon.hub.window_open() {
                eprintln!("[uidm-daemon] no window to confirm #{id}; starting it as proposed");
                // A "download later" asked only about a duplicate keeps waiting.
                let (start, queue) = daemon
                    .hub
                    .requests
                    .lock()
                    .unwrap()
                    .get(&id)
                    .map_or((true, None), |d| (d.start != Some(false), d.queue));
                let answer = Answer { start, queue, file_exists: fallback, ..Answer::start(id) };
                if let Err(e) = daemon.confirm(answer) {
                    eprintln!("[uidm-daemon] could not start #{id}: {e}");
                }
            }
        });
    }

    fn notify(self: &Arc<Self>, id: u64, failed: bool, settings: &Settings) {
        let Some(entry) = self.entry(id) else {
            return;
        };
        if failed && settings.sound_on_failed {
            notify::play(Sound::Failed, settings.sound_file(Sound::Failed).as_deref());
        }
        if !failed && settings.sound_on_done {
            notify::play(Sound::Done, settings.sound_file(Sound::Done).as_deref());
        }
        // A download that asked for the completion dialog gets it, or a
        // notification in its place when no window is open.
        let dialog = !failed && entry.on_done.is_some_and(|o| o.dialog);
        let toast = match settings.notify {
            _ if dialog => !self.hub.window_open(),
            Notify::Off => false,
            Notify::Toast => true,
            // An open window shows its own dialog.
            Notify::Dialog => !self.hub.window_open(),
        };
        if !toast {
            return;
        }
        let daemon = Arc::clone(self);
        match notify::show(&toast_for(&entry, failed), move |arg| daemon.on_toast(&arg)) {
            Ok(t) => {
                let mut toasts = self.toasts.lock().unwrap();
                toasts.push_back(t);
                while toasts.len() > KEEP_TOASTS {
                    toasts.pop_front();
                }
            }
            Err(e) => eprintln!("[uidm-daemon] could not show a notification: {e}"),
        }
    }

    /// Act on a click on a notification or one of its buttons.
    fn on_toast(&self, arg: &str) {
        let (action, id) = match arg.split_once(':') {
            Some((a, id)) => (a, id.parse::<u64>().ok()),
            None => (arg, None),
        };
        let entry = id.and_then(|id| self.entry(id));
        let path = entry.as_ref().and_then(|e| e.path.clone());
        match (action, path) {
            // A file with a threat is opened only from the window, which asks.
            ("open", Some(_)) if matches!(entry.as_ref().and_then(|e| e.scan.as_ref()), Some(Scan::Threat { .. })) => {
                self.show_window()
            }
            ("open", Some(path)) => {
                let _ = uidm::platform::open(path.as_ref());
            }
            ("show", Some(path)) => {
                let _ = uidm::platform::reveal(Path::new(&path));
            }
            ("retry", _) => {
                if let Some(id) = id {
                    if let Err(e) = self.retry(id) {
                        eprintln!("[uidm-daemon] retry from a notification: {e}");
                    }
                }
            }
            _ => self.show_window(),
        }
    }

    /// Take a new download: ask where to save it when the settings say so
    /// (unless its type starts without asking), otherwise queue it at once,
    /// or only add it to the Download later queue when the settings say
    /// that. If its address is already in the list, the duplicate setting
    /// decides first, asking in the save-as dialog under `Ask` even when
    /// that dialog is off.
    fn add(self: &Arc<Self>, mut download: Download) -> (u64, String) {
        if let Some(old) = self.duplicate_of(&download) {
            let choice = download.duplicate.unwrap_or(self.settings.lock().unwrap().if_duplicate);
            eprintln!("[uidm-daemon] {} is already in the list as #{} ({choice:?})", download.url, old.id);
            if choice == Duplicate::Ask {
                return self.ask(download, Some(old.id));
            }
            if let Some(kept) = self.settle_duplicate(&mut download, &old, choice) {
                return kept;
            }
        }
        let settings = self.settings.lock().unwrap().clone();
        if download.start == Some(false) || (download.start.is_none() && settings.add_only) {
            return self.add_later(download);
        }
        let name = engine::Inspection::unprobed(&download.url, download.filename.as_deref()).filename;
        // A folder UIDM may not write to asks first, so the dialog can offer
        // administrator rights or another folder.
        let folder = match download.folder.as_deref().map(Path::new).filter(|f| f.is_absolute()) {
            Some(f) => f.to_path_buf(),
            None => settings.folders_for(&download.url).for_name(&name),
        };
        let denied = !download.elevate && !uidm::elevate::can_write(&folder);
        if denied || download.ask.unwrap_or(settings.ask_where_to_save && !settings.starts_without_asking(&name)) {
            self.ask(download, None)
        } else {
            self.start(download, None)
        }
    }

    /// The newest download in the list with `download`'s address. Form posts
    /// and streams are never duplicates: one address can stand for different
    /// files there (another form, another quality).
    fn duplicate_of(&self, download: &Download) -> Option<DownloadEntry> {
        if uidm::app::post_body(download).is_some() || download.media.is_some() {
            return None;
        }
        let table = self.hub.table.lock().unwrap();
        table.entries().filter(|e| uidm::app::same_address(&e.url, &download.url)).last().cloned()
    }

    /// Apply `choice` to `download`, whose address download `old` already
    /// has. Returns the download to report when nothing new should be added;
    /// otherwise `download` is ready to add.
    fn settle_duplicate(
        self: &Arc<Self>,
        download: &mut Download,
        old: &DownloadEntry,
        choice: Duplicate,
    ) -> Option<(u64, String)> {
        match choice {
            Duplicate::Ask | Duplicate::NewName => {
                download.file_exists = Some(FileExists::Rename);
                None
            }
            Duplicate::Existing => self.use_existing(old),
            // A running download's file cannot be replaced under it.
            Duplicate::Overwrite if !old.status.is_terminal() => self.use_existing(old),
            Duplicate::Overwrite => {
                if let Some(path) = old.path.as_deref().map(Path::new) {
                    download.filename = path.file_name().map(|n| n.to_string_lossy().into_owned());
                    download.folder = path.parent().map(|d| d.display().to_string());
                }
                download.file_exists = Some(FileExists::Overwrite);
                if let Err(e) = self.remove(old.id, false) {
                    eprintln!("[uidm-daemon] could not remove #{}: {e}", old.id);
                }
                None
            }
        }
    }

    /// Point the user at download `old` instead of adding its address again:
    /// show a finished one's completion dialog again, resume a stopped one,
    /// and bring the window up for a running one. `None` when the finished
    /// file is gone, so the new download should be added after all.
    fn use_existing(self: &Arc<Self>, old: &DownloadEntry) -> Option<(u64, String)> {
        let kept = Some((old.id, old.url.clone()));
        match old.status {
            Status::Done if !old.path.as_deref().is_some_and(|p| Path::new(p).is_file()) => None,
            Status::Done => {
                if self.hub.window_open() {
                    self.hub.broadcast(&Outgoing::ShowDone { id: old.id });
                } else {
                    // Without a window, a notification stands in for the
                    // dialog, whatever the notification setting.
                    let mut settings = self.settings.lock().unwrap().clone();
                    settings.notify = Notify::Toast;
                    settings.sound_on_done = false;
                    self.notify(old.id, false, &settings);
                }
                kept
            }
            s if s.is_retryable() => match self.retry(old.id) {
                Ok(resumed) => Some(resumed),
                Err(e) => {
                    eprintln!("[uidm-daemon] could not resume #{}: {e}", old.id);
                    kept
                }
            },
            _ => {
                self.show_window();
                kept
            }
        }
    }

    /// Take download `id` off the list though it never ran: a duplicate the
    /// user chose not to add.
    fn drop_entry(&self, id: u64) {
        self.hub.requests.lock().unwrap().remove(&id);
        if self.hub.leave_queues(id) {
            self.hub.publish_queues();
        }
        self.hub.publish(&Outgoing::Removed { id });
    }

    /// Add `download` to a queue (the one it names, else the one Download
    /// later goes to, else the main one) without starting it or opening a
    /// connection.
    fn add_later(&self, download: Download) -> (u64, String) {
        let later = self.settings.lock().unwrap().later_queue;
        let queue = self.queue_or_main(download.queue.or(later));
        let (id, url) = self.stage(download);
        let _ = self.hub.update_queue(queue, |q| {
            q.order.push(id);
            Ok(())
        });
        (id, url)
    }

    /// `queue` if it exists, else the main queue.
    fn queue_or_main(&self, queue: Option<u64>) -> u64 {
        queue.filter(|q| self.hub.queues.lock().unwrap().iter().any(|x| x.id == *q)).unwrap_or(MAIN_QUEUE)
    }

    /// Announce `download` as waiting, not yet in any queue. A full-path
    /// `folder` fixes where it will be saved.
    fn stage(&self, download: Download) -> (u64, String) {
        let url = download.url.clone();
        let path = download
            .folder
            .as_deref()
            .map(PathBuf::from)
            .filter(|f| f.is_absolute())
            .map(|f| f.join(engine::Inspection::unprobed(&url, download.filename.as_deref()).filename));
        // The size the browser's response gave, until the server is asked.
        let size = download.size;
        let id = self.engine.reserve_id();
        self.hub.requests.lock().unwrap().insert(id, download);
        self.hub.publish(&Outgoing::Added { id, url: url.clone() });
        let _ = self.hub.update(id, |e| {
            e.status = Status::Waiting;
            if e.total.is_none() {
                e.total = size;
            }
            if let Some(p) = &path {
                e.path = Some(p.display().to_string());
            }
            Ok(())
        });
        (id, url)
    }

    /// Add `items` to wait in `queue`, announcing the queue once rather than
    /// once per download. Items that are not web addresses are skipped, and
    /// so are those already in the list unless their own answer or the
    /// duplicate setting says to add them (with a new name, or in place of a
    /// stopped download). Returns
    /// how many were added and how many were skipped as duplicates.
    fn add_many(self: &Arc<Self>, items: Vec<Download>, queue: Option<u64>) -> (usize, usize) {
        let (ids, skipped) = self.add_many_ids(items, queue);
        (ids.len(), skipped)
    }

    /// `add_many`, answering with the new downloads' ids.
    fn add_many_ids(self: &Arc<Self>, items: Vec<Download>, queue: Option<u64>) -> (Vec<u64>, usize) {
        let queue = self.queue_or_main(queue);
        let choice = self.settings.lock().unwrap().if_duplicate;
        let mut ids = Vec::new();
        let mut skipped = 0;
        for mut d in items {
            if !uidm::batch::is_download_url(&d.url) || check_body(&d).is_err() {
                continue;
            }
            if let Some(old) = self.duplicate_of(&d) {
                let choice = d.duplicate.unwrap_or(choice);
                let add = match choice {
                    Duplicate::NewName => true,
                    Duplicate::Overwrite => old.status.is_terminal(),
                    Duplicate::Ask | Duplicate::Existing => false,
                };
                if !add {
                    skipped += 1;
                    continue;
                }
                self.settle_duplicate(&mut d, &old, choice);
            }
            ids.push(self.stage(d).0);
        }
        let _ = self.hub.update_queue(queue, |q| {
            q.order.extend(ids.iter().copied());
            Ok(())
        });
        (ids, skipped)
    }

    /// Read a stream's playlist on another thread and answer `reply` with
    /// what it offers.
    fn probe_media(&self, request: u64, download: Download, reply: Sender<Vec<u8>>) {
        let req = self.request_for(download);
        thread::spawn(move || {
            let info = engine::probe_media(&req.url, &req.headers, &req.net, req.ffmpeg.as_deref());
            if let Ok(frame) = serde_json::to_vec(&Outgoing::MediaInfo { request, info }) {
                let _ = reply.send(frame);
            }
        });
    }

    /// Crawl a site for `client` on another thread, sending it progress and
    /// then what was found. With `download_found`, each file found goes to
    /// that queue at once, and the queue starts.
    fn grab(self: &Arc<Self>, client: u64, req: GrabRequest, reply: Sender<Vec<u8>>) -> Result<(), String> {
        req.validate()?;
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut grabs = self.grabs.lock().unwrap();
            if grabs.contains_key(&client) {
                return Err("A site crawl is already running.".into());
            }
            grabs.insert(client, Arc::clone(&stop));
        }
        let daemon = Arc::clone(self);
        thread::spawn(move || {
            let send = |msg: &Outgoing| {
                if let Ok(frame) = serde_json::to_vec(msg) {
                    let _ = reply.send(frame);
                }
            };
            eprintln!("[uidm-daemon] client #{client} crawls {}", req.start);
            let mut handed = 0;
            let mut added = Vec::new();
            let login = daemon.login_cookies(&req);
            let out = daemon.run_crawl(&req, login.as_deref(), &stop, |o| {
                if let Some(queue) = req.download_found {
                    let fresh = &o.files[handed..];
                    if !fresh.is_empty() {
                        added.extend(daemon.download_in_queue(&req, login.as_deref(), fresh, queue, handed == 0));
                    }
                    handed = o.files.len();
                }
                send(&Outgoing::GrabProgress {
                    pages: o.pages,
                    bytes: o.bytes,
                    found: o.files.len(),
                    explored: o.explored,
                    added: added.clone(),
                })
            });
            daemon.grabs.lock().unwrap().remove(&client);
            eprintln!("[uidm-daemon] crawl for #{client} read {} pages, found {} files", out.pages, out.files.len());
            send(&Outgoing::GrabDone {
                files: out.files,
                pages: out.pages,
                explored: out.explored,
                stopped: out.stopped,
                login_cookies: login,
            });
        });
        Ok(())
    }

    /// Add `files` a crawl for `req` found to `queue` and start it; with
    /// `first`, also set how many it downloads at once to the request's.
    /// Files on the login page's site carry `login` (its cookies).
    /// Returns the new downloads' ids.
    fn download_in_queue(
        self: &Arc<Self>,
        req: &GrabRequest,
        login: Option<&str>,
        files: &[grab::GrabFile],
        queue: u64,
        first: bool,
    ) -> Vec<u64> {
        let settings = self.settings.lock().unwrap().clone();
        let items = files.iter().filter_map(|f| grab::download_for(req, f, &settings, login)).collect();
        let (added, _) = self.add_many_ids(items, Some(queue));
        if first {
            let at_once = req.download_at_once as usize;
            let _ = self.hub.update_queue(queue, |q| {
                q.max_concurrent = at_once;
                Ok(())
            });
        }
        if !added.is_empty() && !self.hub.queues.lock().unwrap().iter().any(|q| q.id == queue && q.running) {
            if let Err(e) = self.start_queue(queue, None) {
                eprintln!("[uidm-daemon] could not start the grabber's queue: {e}");
            }
        }
        added
    }

    /// Crawl for `req` on this thread, with the settings' proxy and logins
    /// (and the project's own login for its start site), the browser's
    /// cookies when it asks, `login` (see `login_cookies`) for its login
    /// page's site, and short waits so `stop` is noticed quickly.
    fn run_crawl(
        &self,
        req: &GrabRequest,
        login: Option<&str>,
        stop: &AtomicBool,
        progress: impl FnMut(&grab::Outcome),
    ) -> grab::Outcome {
        let settings = self.settings.lock().unwrap().clone();
        let start = Download { url: req.start.clone(), ..Download::default() };
        let mut net = self.net_for(&start, &settings);
        if let (Some(user), Ok(url)) =
            (req.login_user.as_deref().map(str::trim).filter(|u| !u.is_empty()), url::Url::parse(&req.start))
        {
            if let Some(host) = url.host_str() {
                net.logins.push(Login {
                    site: host.to_string(),
                    username: user.to_string(),
                    password: grab::project_password(&req.start, user).unwrap_or_default(),
                    // Given for this project's start site.
                    plain: true,
                });
            }
        }
        let headers = [("User-Agent".to_string(), GRABBER_AGENT.to_string())];
        let cookies = if req.browser_cookies { self.browser_cookies(&req.start) } else { None };
        // Each site's cookies go to that site's pages only.
        let page_headers = |url: &str| {
            let mut h = headers.to_vec();
            if let Some(c) = grab::crawl_cookies(req, url, cookies.as_deref(), login) {
                h.push(("Cookie".to_string(), c));
            }
            h
        };
        // Rendering needs the browser extension connected.
        let render = AtomicBool::new(req.render_scripts && self.hub.extension_connected());
        let fetch = |url: &str| {
            // A page as the browser shows it, when asked and the browser
            // can; otherwise as the server sends it.
            let rendering = render.load(Ordering::Relaxed) && !url.ends_with("/robots.txt");
            let rendered = rendering.then(|| self.render_in_browser(url)).flatten();
            // A browser that does not answer once is not asked again.
            if rendering && rendered.is_none() {
                eprintln!("[uidm-daemon] the browser did not render {url}; reading pages as sent");
                render.store(false, Ordering::Relaxed);
            }
            if let Some((mut html, at, popups)) = rendered {
                // Pages the page's scripts opened in new windows are links
                // like the rest, unless the crawl ignores them.
                if !req.ignore_popups {
                    for p in popups.iter().filter(|p| p.starts_with("http://") || p.starts_with("https://")) {
                        html.push_str(&format!("<a href=\"{}\"></a>", p.replace('"', "%22")));
                    }
                }
                let mut body = html.into_bytes();
                body.truncate(grab::MAX_PAGE_BYTES as usize);
                let page = grab::Page { url: at, content_type: Some("text/html".into()), body };
                if let Some(m) = req.mirror.as_ref().filter(|m| m.pages) {
                    save_page(req, m, &page);
                }
                return Ok(page);
            }
            let page = engine::fetch(url, &page_headers(url), &net, grab::MAX_PAGE_BYTES)
                .map(|f| grab::Page { url: f.url, content_type: f.content_type, body: f.body })
                .map_err(|e| e.to_string())?;
            if let Some(m) = req.mirror.as_ref().filter(|m| m.pages) {
                save_page(req, m, &page);
            }
            Ok(page)
        };
        // Wait in short steps, so stopping is quick.
        let wait = |d: Duration| {
            let until = Instant::now() + d;
            while Instant::now() < until && !stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(100).min(until - Instant::now()));
            }
        };
        // A file's size, for the size filters and duplicates.
        let size = |url: &str| engine::inspect(url, None, &page_headers(url), &net).ok().and_then(|i| i.total);
        grab::crawl(req, fetch, size, wait, stop, progress)
    }

    /// Look for due grabber projects at most every half minute: the
    /// scheduler wakes on every download event, and this reads files.
    fn check_grab_schedules(self: &Arc<Self>) {
        {
            let mut last = self.grab_checked.lock().unwrap();
            if last.is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
                return;
            }
            *last = Some(Instant::now());
        }
        self.run_grab_schedules(queue::local_now());
    }

    /// Start the saved grabber projects whose scheduled run is due, each on
    /// its own thread. Runs with the scheduler.
    fn run_grab_schedules(self: &Arc<Self>, now: queue::Moment) {
        let Ok(dir) = grab::projects_dir() else {
            return;
        };
        for name in grab::recent_projects(&dir) {
            let Ok(mut project) = grab::Project::load(&dir, &name) else {
                continue;
            };
            let due = project
                .schedule
                .as_ref()
                .and_then(|s| s.due(now, project.last_scheduled_day, project.last_scheduled_start));
            let Some(day) = due else {
                continue;
            };
            if !self.grab_runs.lock().unwrap().insert(name.clone()) {
                continue;
            }
            // Saved first, so a restart does not start the same run again.
            project.last_scheduled_day = Some(day);
            project.last_scheduled_start = Some(now.total());
            if let Err(e) = project.save(&dir) {
                eprintln!("[uidm-daemon] could not save grabber project {name}: {e}");
            }
            let daemon = Arc::clone(self);
            let dir = dir.clone();
            thread::spawn(move || {
                daemon.run_scheduled_grab(&dir, project, day);
                daemon.grab_runs.lock().unwrap().remove(&name);
            });
        }
    }

    /// One scheduled run of a grabber project, as its `run` says: crawl
    /// until done or until the schedule's stop time and keep what it found,
    /// then maybe add the new finds to the project's queue and start it; or
    /// only add the ticked files and start the queue. The stop time ends the
    /// queue's run too, and what the project asks when done (with its own
    /// force-close choice) happens after the crawl, or once the queue is done.
    fn run_scheduled_grab(self: &Arc<Self>, dir: &Path, mut project: grab::Project, day: i64) {
        eprintln!("[uidm-daemon] schedule runs grabber project {} ({:?})", project.name, project.run);
        let end = project.schedule.as_ref().filter(|s| s.stop.is_some()).and_then(|s| s.end_of_run(day));
        let resync;
        let login = self.login_cookies(&project.request);
        let to_download = match project.run {
            grab::RunAction::DownloadChecked => {
                resync = project.to_resync(&[]);
                project.checked_files()
            }
            run => {
                let stop = AtomicBool::new(false);
                let out = self.run_crawl(&project.request, login.as_deref(), &stop, |_| {
                    if end.is_some_and(|end| queue::local_now() >= end) {
                        stop.store(true, Ordering::Relaxed);
                    }
                });
                resync = project.to_resync(&out.files);
                let new = project.record_run(out.files);
                eprintln!("[uidm-daemon] grabber project {} found {} new files", project.name, new.len());
                if run == grab::RunAction::ExploreAndDownload {
                    new
                } else {
                    Vec::new()
                }
            }
        };
        if let Err(e) = project.save(dir) {
            eprintln!("[uidm-daemon] could not save grabber project {}: {e}", project.name);
        }
        // Set before the queue starts, so a run that ends at once still
        // does it. A queue already running is a run someone else started,
        // such as the user: the project's files join it, but its stop time
        // and when-done action stay that run's own.
        let mut owns_run = false;
        let _ = self.hub.update_queue(project.queue, |q| {
            if q.running {
                return Ok(());
            }
            owns_run = true;
            q.when_done_once = Some(project.when_done);
            q.open_when_done_once = project.open_when_done.clone();
            q.force_close_once = Some(project.force_close);
            q.stop_at_once = end.map(queue::Moment::total);
            Ok(())
        });
        // A synchronizing project downloads again what earlier runs got and
        // the server has since changed.
        let (changed, listed) = self.resync(&resync, project.queue);
        let to_download: Vec<grab::GrabFile> = to_download.into_iter().filter(|f| !listed.contains(&f.url)).collect();
        // Through the usual path, which skips addresses already in the list.
        let added = if to_download.is_empty() {
            0
        } else {
            self.download_in_queue(&project.request, login.as_deref(), &to_download, project.queue, true).len()
        };
        eprintln!("[uidm-daemon] grabber project {} added {added} files to its queue, {changed} changed", project.name);
        let added = added + changed;
        if added == 0 {
            // Nothing to wait for: done now.
            if owns_run {
                let _ = self.hub.update_queue(project.queue, |q| {
                    q.when_done_once = None;
                    q.open_when_done_once = None;
                    q.force_close_once = None;
                    q.stop_at_once = None;
                    Ok(())
                });
            }
            if let Some(file) = &project.open_when_done {
                open_after_run(file);
            }
            self.when_done(project.when_done, &project.name, None, project.force_close);
        }
    }
    /// Convert finished download `id` on another thread. The new file is a
    /// row of its own: added and started now, with ffmpeg's progress, done
    /// with a note, or failed with ffmpeg's complaint.
    fn convert(
        self: &Arc<Self>,
        id: u64,
        to: uidm::media::convert::Target,
        delete_original: bool,
    ) -> Result<(), String> {
        let entry = self.entry(id).ok_or_else(|| format!("no download with id {id}"))?;
        let input = entry
            .path
            .as_deref()
            .map(PathBuf::from)
            .filter(|p| entry.status == Status::Done && p.is_file())
            .ok_or("Only a finished download whose file is still there can be converted.")?;
        let ffmpeg = self
            .settings
            .lock()
            .unwrap()
            .ffmpeg()
            .ok_or("Converting needs ffmpeg. Install it, or choose it in Options > General.")?;
        let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "converted".into());
        let ext = to.extension();
        let same = input.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext));
        let name = if same { format!("{stem} (converted).{ext}") } else { format!("{stem}.{ext}") };
        let dir = input.parent().map(Path::to_path_buf).unwrap_or_default();
        let output =
            (0..).map(|n| dir.join(engine::numbered_name(&name, n))).find(|p| !p.exists()).expect("some name is free");
        let size = std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0);
        let new_id = self.engine.reserve_id();
        self.hub.uncounted.lock().unwrap().insert(new_id);
        let url = entry.url.clone();
        self.hub.publish(&Outgoing::Added { id: new_id, url: url.clone() });
        self.hub.publish(&Outgoing::Started {
            id: new_id,
            url: url.clone(),
            path: output.display().to_string(),
            total: Some(size),
            resumable: false,
            connections: 1,
            partial: None,
            validator: None,
        });
        let stop = Arc::new(AtomicBool::new(false));
        self.conversions.lock().unwrap().insert(new_id, Arc::clone(&stop));
        eprintln!("[uidm-daemon] converting #{id} to {}", output.display());
        let daemon = Arc::clone(self);
        thread::spawn(move || {
            let mut last = Instant::now() - Duration::from_secs(1);
            let result = uidm::media::convert::convert(&ffmpeg, &input, &output, to, &stop, |f| {
                if last.elapsed() >= Duration::from_millis(500) {
                    last = Instant::now();
                    daemon.hub.publish(&Outgoing::Progress {
                        id: new_id,
                        url: url.clone(),
                        done: (f * size as f64) as u64,
                        total: Some(size),
                        parts: None,
                        segments: None,
                    });
                }
            });
            daemon.conversions.lock().unwrap().remove(&new_id);
            let from = input.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            match result {
                Ok(()) => {
                    if delete_original {
                        if let Err(e) = daemon.remove(id, true) {
                            eprintln!("[uidm-daemon] kept the original of #{id}: {e}");
                        }
                    }
                    daemon.hub.publish(&Outgoing::Done {
                        id: new_id,
                        url,
                        path: output.display().to_string(),
                        note: Some(format!("Converted from {from}")),
                    });
                }
                Err(e) if e == "canceled" => daemon.hub.publish(&Outgoing::Canceled { id: new_id, url }),
                Err(error) => daemon.hub.publish(&Outgoing::Failed { id: new_id, url, error }),
            }
        });
        Ok(())
    }

    /// Ask GitHub (at `url`) for the latest release, through the settings'
    /// proxy, and note when that happened.
    fn check_update(&self, url: &str) -> Outgoing {
        let settings = self.settings.lock().unwrap().clone();
        let start = Download { url: url.to_string(), ..Download::default() };
        let net = self.net_for(&start, &settings);
        let headers = [
            ("User-Agent".to_string(), format!("UIDM/{}", env!("CARGO_PKG_VERSION"))),
            ("Accept".to_string(), "application/vnd.github+json".to_string()),
        ];
        let found = engine::fetch(url, &headers, &net, 1024 * 1024)
            .map_err(|e| format!("Could not reach GitHub: {e}"))
            .and_then(|page| uidm::update::parse_release(&page.body))
            // Only Windows has a Setup to run; elsewhere the page is shown.
            .map(|release| uidm::update::Release { setup: release.setup.filter(|_| cfg!(windows)), ..release });
        {
            let mut s = self.settings.lock().unwrap();
            s.last_update_check = uidm::state::unix_now();
            if let Some(path) = &self.paths.settings {
                let _ = s.save(path);
            }
        }
        match found {
            Ok(release) => Outgoing::Update {
                newer: uidm::update::is_newer(&release.version, env!("CARGO_PKG_VERSION")).then_some(release),
                error: None,
            },
            Err(e) => Outgoing::Update { newer: None, error: Some(e) },
        }
    }

    /// Update now: download the newest release's Setup through the
    /// settings' proxy, check it, save it to the temp folder and run it,
    /// then stop the downloads, save the list and quit with the windows,
    /// so Setup can replace the files. Setup closes whatever is left.
    fn install_update(&self) -> Result<(), String> {
        let release = match self.check_update(uidm::update::LATEST_RELEASE) {
            Outgoing::Update { newer: Some(r), .. } => r,
            Outgoing::Update { error: Some(e), .. } => return Err(e),
            _ => return Err("This is the newest version.".into()),
        };
        let setup = release.setup.ok_or("This release has no Setup program; download it from its page.")?;
        let settings = self.settings.lock().unwrap().clone();
        let net = self.net_for(&Download { url: setup.url.clone(), ..Download::default() }, &settings);
        let headers = [("User-Agent".to_string(), format!("UIDM/{}", env!("CARGO_PKG_VERSION")))];
        eprintln!("[uidm-daemon] downloading {}", setup.url);
        let fetched = engine::fetch(&setup.url, &headers, &net, uidm::update::MAX_SETUP_BYTES)
            .map_err(|e| format!("Could not download the update: {e}"))?;
        uidm::update::check_setup(&setup, &fetched.body)?;
        let path = std::env::temp_dir().join(&setup.name);
        std::fs::write(&path, &fetched.body)
            .map_err(|e| format!("Could not save the update to {}: {e}", path.display()))?;
        // /fromuidm closes UIDM without asking: the user asked for this.
        Command::new(&path)
            .arg("/fromuidm=yes")
            .spawn()
            .map_err(|e| format!("Could not start {}: {e}", path.display()))?;
        eprintln!("[uidm-daemon] started {}; quitting for it", path.display());
        self.pause_all();
        self.save_history();
        self.hub.broadcast(&Outgoing::Quit);
        Ok(())
    }

    /// Once a week, when a window connects, look for a newer release on
    /// another thread and tell the windows if there is one.
    fn weekly_update_check(self: &Arc<Self>) {
        let (on, last) = {
            let s = self.settings.lock().unwrap();
            (s.check_updates, s.last_update_check)
        };
        if !on || uidm::state::unix_now().saturating_sub(last) < uidm::update::CHECK_EVERY_SECS {
            return;
        }
        // Noted at once, so two windows do not both check.
        self.settings.lock().unwrap().last_update_check = uidm::state::unix_now();
        let daemon = Arc::clone(self);
        thread::spawn(move || {
            if let msg @ Outgoing::Update { newer: Some(_), .. } = daemon.check_update(uidm::update::LATEST_RELEASE) {
                daemon.hub.broadcast(&msg);
            }
        });
    }

    /// List the files inside zip download `id` on another thread, reading
    /// the end of the archive with its own request (cookies and all), and
    /// answer `reply` with `zip_preview`.
    fn preview_zip(&self, id: u64, reply: Sender<Vec<u8>>) {
        let entry = self.entry(id);
        let download = self.hub.requests.lock().unwrap().get(&id).cloned();
        let answer = move |entries: Vec<uidm::zip::ZipEntry>, error: Option<String>| {
            if let Ok(frame) = serde_json::to_vec(&Outgoing::ZipPreview { id, entries, error }) {
                let _ = reply.send(frame);
            }
        };
        let Some(entry) = entry else {
            return answer(Vec::new(), Some(format!("no download with id {id}")));
        };
        let download = download.unwrap_or_else(|| Download { url: entry.url.clone(), ..Download::default() });
        let req = self.request_for(download);
        thread::spawn(move || {
            let total = match entry.total {
                Some(t) => Ok(t),
                None => engine::inspect(&req.url, None, &req.headers, &req.net)
                    .map_err(|e| e.to_string())
                    .and_then(|i| i.total.ok_or_else(|| "The server does not say how big the file is.".to_string())),
            };
            let listed = total.and_then(|total| {
                uidm::zip::list(total, |a, b| {
                    engine::fetch_range_bytes(&req.url, &req.headers, &req.net, a, b)
                        .map_err(|e| format!("The server would not send part of the file: {e}"))
                })
            });
            match listed {
                Ok(entries) => answer(entries, None),
                Err(e) => answer(Vec::new(), Some(e)),
            }
        });
    }

    /// Ask the browser extension to render `url` in a background tab and
    /// wait for its HTML and final address. `None` without an extension or
    /// an answer in time.
    fn render_in_browser(&self, url: &str) -> Rendered {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let request = NEXT.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.render_waits.lock().unwrap().insert(request, tx);
        self.hub.broadcast(&Outgoing::RenderPage { request, url: url.to_string() });
        // The extension gives up on a page after 30 seconds itself.
        let got = rx.recv_timeout(Duration::from_secs(40)).ok().flatten();
        self.render_waits.lock().unwrap().remove(&request);
        got
    }

    /// The cookies for `req`'s login page's site: the browser's, where the
    /// user signed in after Open, else the ones the user pasted. `None`
    /// without a login page.
    fn login_cookies(&self, req: &GrabRequest) -> Option<String> {
        let login = req.login_url.as_deref()?;
        self.browser_cookies(login).or_else(|| req.login_cookies.as_deref().and_then(grab::clean_cookies))
    }

    /// Ask the browser extension for its cookies for `url`, waiting a few
    /// seconds for the answer. `None` without an extension or cookies.
    fn browser_cookies(&self, url: &str) -> Option<String> {
        // Without the extension nobody answers: no need to wait.
        if !self.hub.extension_connected() {
            return None;
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let request = NEXT.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.cookie_waits.lock().unwrap().insert(request, tx);
        self.hub.broadcast(&Outgoing::CookiesWanted { request, url: url.to_string() });
        let got = rx.recv_timeout(Duration::from_secs(5)).ok().flatten();
        self.cookie_waits.lock().unwrap().remove(&request);
        eprintln!("[uidm-daemon] browser cookies for the crawl: {}", if got.is_some() { "found" } else { "none" });
        got.filter(|c| !c.is_empty())
    }

    /// Stop `client`'s crawl, if it has one.
    fn stop_grab(&self, client: u64) {
        if let Some(stop) = self.grabs.lock().unwrap().get(&client) {
            stop.store(true, Ordering::Relaxed);
        }
    }

    /// Ask the server of each of `links` for its size and type, the way a
    /// download of it would (its cookies, referrer, proxy and login), for
    /// `client`'s link picker. `LINK_PROBES` links are asked at a time, so a
    /// long list neither waits on one slow server nor floods the sites, and
    /// each answer goes out as it comes. Any earlier probing for `client`
    /// stops.
    fn probe_links(&self, client: u64, request: u64, links: Vec<Download>, reply: Sender<Vec<u8>>) {
        let stop = Arc::new(AtomicBool::new(false));
        if let Some(old) = self.link_probes.lock().unwrap().insert(client, Arc::clone(&stop)) {
            old.store(true, Ordering::Relaxed);
        }
        let reqs: Arc<Vec<DownloadRequest>> = Arc::new(links.into_iter().map(|d| self.request_for(d)).collect());
        let next = Arc::new(AtomicUsize::new(0));
        for _ in 0..LINK_PROBES.min(reqs.len()) {
            let (reqs, next, stop, reply) = (Arc::clone(&reqs), Arc::clone(&next), Arc::clone(&stop), reply.clone());
            thread::spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(req) = reqs.get(index).filter(|_| !stop.load(Ordering::Relaxed)) else {
                    return;
                };
                let found = engine::inspect(&req.url, None, &req.headers, &req.net);
                // Stopped while asking: the picker is gone, or asked anew.
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let msg = match found {
                    Ok(i) => {
                        Outgoing::LinkInfo { request, index, total: i.total, content_type: i.content_type, error: None }
                    }
                    Err(e) => Outgoing::LinkInfo {
                        request,
                        index,
                        total: None,
                        content_type: None,
                        error: Some(e.to_string()),
                    },
                };
                // A send that fails means the client went.
                if !serde_json::to_vec(&msg).is_ok_and(|frame| reply.send(frame).is_ok()) {
                    return;
                }
            });
        }
    }

    /// Stop `client`'s link probes, if it has any.
    fn stop_probe_links(&self, client: u64) {
        if let Some(stop) = self.link_probes.lock().unwrap().remove(&client) {
            stop.store(true, Ordering::Relaxed);
        }
    }

    /// Announce `download` as queued, probe it on another thread, then hold it
    /// as `confirming` with the proposed file until a window answers.
    /// `duplicate_of` is another download with the same address, for the
    /// dialog to ask about.
    fn ask(self: &Arc<Self>, download: Download, duplicate_of: Option<u64>) -> (u64, String) {
        let url = download.url.clone();
        let id = self.engine.reserve_id();
        self.hub.requests.lock().unwrap().insert(id, download.clone());
        self.hub.publish(&Outgoing::Added { id, url: url.clone() });

        let daemon = Arc::clone(self);
        thread::spawn(move || {
            let req = daemon.request_for(download.clone());
            // Probing a POST download would send a GET (or repeat the POST),
            // so its dialog proposes a name from the request alone.
            // What the browser's response said is all there is to show then.
            let probed = if uidm::app::post_body(&download).is_some() {
                let mut found = engine::Inspection::unprobed(&download.url, download.filename.as_deref());
                found.total = download.size;
                found.content_type = download.content_type.clone();
                Ok(found)
            } else {
                engine::inspect(&download.url, download.filename.as_deref(), &req.headers, &req.net)
            };
            let found = match probed {
                Ok(found) => found,
                Err(e) => {
                    // Same as a download that fails to start, so the browser
                    // can take it back.
                    daemon.hub.publish(&Outgoing::Failed { id, url: download.url, error: e.to_string() });
                    return;
                }
            };
            let path = req.folders.for_name(&found.filename).join(&found.filename);
            // The other one may have been removed while this was probed.
            let duplicate_of = duplicate_of.filter(|o| daemon.status(*o).is_some());
            let settings = daemon.settings.lock().unwrap().clone();
            // A type that starts without asking, known only now from the
            // server's name for the file, skips the dialog after all.
            let writable = download.elevate || uidm::elevate::can_write(path.parent().unwrap_or(&path));
            let auto = duplicate_of.is_none()
                && download.ask != Some(true)
                && writable
                && settings.starts_without_asking(&found.filename);
            let asked = daemon.hub.update(id, |e| {
                if e.status != Status::Queued {
                    return Err("no longer queued".into());
                }
                e.status = if auto { Status::Waiting } else { Status::Confirming };
                e.path = Some(path.display().to_string());
                e.total = found.total;
                e.resumable = found.resumable;
                e.duplicate_of = duplicate_of;
                Ok(())
            });
            if asked.is_err() {
                return; // canceled while being probed
            }
            if auto {
                if let Err(e) = daemon.dispatch(id) {
                    eprintln!("[uidm-daemon] could not start #{id}: {e}");
                }
                return;
            }
            // Only a file the server sends in ranges can be stopped and
            // carried on elsewhere once the dialog is answered.
            let plain = uidm::app::post_body(&download).is_none() && download.media.is_none();
            if settings.start_while_asking
                && writable
                && duplicate_of.is_none()
                && plain
                && found.resumable
                && found.total.is_some()
            {
                daemon.start_early(id, download.clone(), &found.filename);
            }
            daemon.await_answer(id, None);
        });
        (id, url)
    }

    /// Start download `id` while its save-as dialog is open, under an engine
    /// id of its own (whose events clients never see) and into a folder of
    /// its own. Answering the dialog keeps what it fetched: see
    /// `settle_early`.
    fn start_early(&self, id: u64, download: Download, filename: &str) {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = early_root();
        let dir = root.join(format!("{}-{n}", std::process::id()));
        // A new folder each time: one that is already there, or a link in
        // its place, makes this fail rather than be written through.
        if let Err(e) = std::fs::create_dir_all(&root).and_then(|()| std::fs::create_dir(&dir)) {
            eprintln!("[uidm-daemon] could not start #{id} early: {e}");
            return;
        }
        let mut req = self.request_for(download);
        req.folders = Folders::fixed(dir.clone());
        req.filename = Some(filename.to_string());
        req.file_exists = FileExists::Overwrite;
        req.temp_dir = None;
        // The server's date stays as the settings say: the engine sets it
        // only on a finished file, and moving that file into place keeps it.
        let shadow = self.engine.reserve_id();
        self.hub
            .early
            .lock()
            .unwrap()
            .insert(shadow, Early { real: id, dir, file: None, target: None, total: None, end: None, discard: false });
        eprintln!("[uidm-daemon] #{id} starts early while its dialog is open");
        self.engine.enqueue_as(shadow, req);
    }

    /// Stop the early download `shadow` and wait for it, returning how it
    /// ended. `None` if it did not stop in time; its events
    /// stay hidden, and its folder goes when UIDM next starts.
    fn stop_early(&self, shadow: u64) -> Option<Early> {
        self.engine.cancel(shadow);
        let deadline = Instant::now() + EARLY_STOP_WAIT;
        loop {
            {
                let mut early = self.hub.early.lock().unwrap();
                if early.get(&shadow).is_some_and(|e| e.end.is_some()) {
                    return early.remove(&shadow);
                }
                if Instant::now() >= deadline {
                    if let Some(e) = early.get_mut(&shadow) {
                        e.real = 0;
                        e.discard = true;
                    }
                    return None;
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Download `id`'s dialog was answered: stop its early download and put
    /// what it fetched at the file the answer chose (a free name there,
    /// unless the answer or setting overwrites). A finished file moves there
    /// and the download is done; a partial one becomes the download's own
    /// partial file, so starting it carries on from there. Returns whether
    /// it finished.
    fn settle_early(&self, id: u64, shadow: u64) -> bool {
        let Some(early) = self.stop_early(shadow) else {
            return false;
        };
        let entry = self.entry(id);
        let path = entry.as_ref().and_then(|e| e.path.as_deref()).map(PathBuf::from);
        let overwrite = {
            let asked = self.hub.requests.lock().unwrap().get(&id).and_then(|d| d.file_exists);
            asked.unwrap_or(self.settings.lock().unwrap().file_exists) == FileExists::Overwrite
        };
        let mut kept = false;
        // Stopping it can land after its last byte was written and its file
        // saved, which the engine still reports as stopped: a whole file
        // with no partial file or resume file left is kept as finished,
        // rather than fetched again.
        let end = match early.end {
            Some(None) => Some(early_finished_anyway(&early).map(Path::to_path_buf)),
            end => end,
        };
        match (entry, path, end, early.file) {
            (Some(e), Some(path), Some(Some(finished)), _) => {
                let to = free_path(&path, overwrite);
                match move_file(&finished, &to) {
                    Ok(()) => {
                        eprintln!("[uidm-daemon] #{id} finished while its dialog was open");
                        self.hub.leave_queues(id);
                        self.hub.publish(&Outgoing::Done {
                            id,
                            url: e.url,
                            path: to.display().to_string(),
                            note: None,
                        });
                        let _ = std::fs::remove_dir_all(&early.dir);
                        return true;
                    }
                    Err(err) => eprintln!("[uidm-daemon] could not keep #{id}'s early download: {err}"),
                }
            }
            (Some(_), Some(path), _, Some(partial))
                if partial.is_file() && engine::sidecar_path(&partial).is_file() =>
            {
                let to = free_path(&path, overwrite);
                kept = self
                    .hub
                    .update(id, |e| {
                        e.path = Some(to.display().to_string());
                        e.partial = Some(partial.display().to_string());
                        e.connections = e.connections.max(1);
                        Ok(())
                    })
                    .is_ok();
            }
            _ => {}
        }
        if !kept {
            let _ = std::fs::remove_dir_all(&early.dir);
        }
        false
    }

    /// Stop download `id`'s early download, if it has one, and drop what it
    /// fetched once it has stopped.
    fn discard_early(&self, id: u64) {
        let Some(shadow) = self.hub.early_of(id) else {
            return;
        };
        let mut early = self.hub.early.lock().unwrap();
        let ended = early.get(&shadow).is_some_and(|e| e.end.is_some());
        if ended {
            if let Some(gone) = early.remove(&shadow) {
                let _ = std::fs::remove_dir_all(gone.dir);
            }
            return;
        }
        if let Some(e) = early.get_mut(&shadow) {
            e.real = 0;
            e.discard = true;
        }
        drop(early);
        self.engine.cancel(shadow);
    }

    /// Answer the save-as dialog of a `confirming` download, or start a
    /// `waiting` one. Missing names and folders keep the proposed ones. With
    /// `start` false it waits in `queue` (the main queue by default).
    /// `file_exists`, when given, decides what happens if the name is taken,
    /// now and on later starts. For a download asked about as a duplicate,
    /// `duplicate` decides what happens to the other download first (a new
    /// name when missing). A category and description, when given, are kept
    /// with it. The dialog's "remember" boxes change the settings.
    fn confirm(self: &Arc<Self>, answer: Answer) -> Result<(), String> {
        let Answer {
            id,
            mut filename,
            mut folder,
            start,
            queue,
            mut file_exists,
            duplicate,
            category,
            description,
            remember_folder,
            remember_duplicate,
            remember_queue,
            elevate,
        } = answer;
        let category = category.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
        let description = description.map(|d| d.trim().to_string());
        let other = self.entry(id).and_then(|e| e.duplicate_of).and_then(|o| self.entry(o));
        if let Some(old) = other {
            let choice = duplicate.unwrap_or(Duplicate::NewName);
            if remember_duplicate {
                let mut settings = self.settings.lock().unwrap().clone();
                if settings.if_duplicate != choice {
                    settings.if_duplicate = choice;
                    if let Err(e) = self.set_settings(settings) {
                        eprintln!("[uidm-daemon] could not remember the duplicate choice: {e}");
                    }
                }
            }
            let mut download = self.hub.requests.lock().unwrap().get(&id).cloned().unwrap_or_default();
            if let Some((kept, _)) = self.settle_duplicate(&mut download, &old, choice) {
                eprintln!("[uidm-daemon] #{id} duplicates #{}; using #{kept} instead", old.id);
                self.drop_entry(id);
                return Ok(());
            }
            // Replacing the other download means writing over its file.
            if choice == Duplicate::Overwrite && old.path.is_some() {
                filename = download.filename.clone();
                folder = download.folder.clone();
            }
            file_exists = file_exists.or(download.file_exists);
        }
        let item = self.hub.update(id, |e| {
            if !matches!(e.status, Status::Confirming | Status::Waiting) {
                return Err(format!("download {id} is not waiting to be started"));
            }
            e.duplicate_of = None;
            if let Some(c) = &category {
                e.category = Some(c.clone());
            }
            if let Some(d) = &description {
                e.description = Some(d.clone()).filter(|d| !d.is_empty());
            }
            if filename.is_none() && folder.is_none() {
                // Keep whatever the download already has: a chosen file, a
                // partial file, or nothing yet.
                e.status = Status::Waiting;
                return Ok(());
            }
            let proposed = e.path.as_deref().map(Path::new);
            let name = filename
                .as_deref()
                .map(engine::clean_filename)
                .filter(|n| !n.is_empty())
                .or_else(|| proposed.and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()))
                .ok_or("the download has no file name")?;
            let dir = match folder.as_deref().map(str::trim) {
                Some(f) if !f.is_empty() => PathBuf::from(f),
                _ => proposed.and_then(Path::parent).map(Path::to_path_buf).ok_or("the download has no folder")?,
            };
            if !dir.is_absolute() {
                return Err(format!("the folder must be a full path: {}", dir.display()));
            }
            e.path = Some(dir.join(&name).display().to_string());
            e.status = Status::Waiting;
            Ok(())
        })?;
        let mut settings = self.settings.lock().unwrap().clone();
        let mut changed = false;
        if let Some(path) = item.path.as_deref().map(Path::new) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let chosen = item.category.clone().unwrap_or_else(|| settings.category_name(&name, &item.url).to_string());
            if let Some(on) = remember_folder {
                changed |= settings.set_remember_folder(&chosen, on);
            }
            // A category that remembers its folder keeps the one chosen here.
            let picked = remember_folder == Some(true) || folder.as_deref().is_some_and(|f| !f.trim().is_empty());
            if let (true, Some(dir)) = (picked, path.parent()) {
                changed |= settings.remember_folder(&chosen, dir);
            }
        }
        if remember_queue && !start {
            let later = Some(queue.unwrap_or(MAIN_QUEUE));
            changed |= settings.later_queue != later;
            settings.later_queue = later;
        }
        if changed {
            if let Err(e) = self.set_settings(settings) {
                eprintln!("[uidm-daemon] could not remember the dialog's choices: {e}");
            }
        }
        if file_exists.is_some() || category.is_some() || description.is_some() || elevate {
            let mut requests = self.hub.requests.lock().unwrap();
            let request =
                requests.entry(id).or_insert_with(|| Download { url: item.url.clone(), ..Download::default() });
            if file_exists.is_some() {
                request.file_exists = file_exists;
            }
            if category.is_some() {
                request.category = category;
            }
            if description.is_some() {
                request.description = item.description.clone();
            }
            // The folder goes with it too: a retry resumes the staged file,
            // which is not in that folder, so only this says where it goes.
            if elevate {
                request.elevate = true;
                if let Some(dir) = item.path.as_deref().map(Path::new).and_then(Path::parent) {
                    request.folder = Some(dir.display().to_string());
                }
            }
        }
        // A download started early stops first, so its bytes can move to
        // the file chosen, which takes a moment.
        if let Some(shadow) = self.hub.early_of(id) {
            let daemon = Arc::clone(self);
            thread::spawn(move || {
                if daemon.settle_early(id, shadow) {
                    return;
                }
                if let Err(e) = daemon.start_or_wait(id, start, queue) {
                    eprintln!("[uidm-daemon] could not go on with #{id}: {e}");
                }
            });
            return Ok(());
        }
        self.start_or_wait(id, start, queue)
    }

    /// Start a `waiting` download now, or (with `start` false) have it wait
    /// in `queue`, the Download later queue, or the main one.
    fn start_or_wait(&self, id: u64, start: bool, queue: Option<u64>) -> Result<(), String> {
        if start {
            return self.dispatch(id);
        }
        if self.hub.queue_of(id).is_none() {
            let later = self.settings.lock().unwrap().later_queue;
            self.move_to_queue(id, self.queue_or_main(queue.or(later)), None)?;
        }
        Ok(())
    }

    /// Cancel a download, whether it waits for its dialog or is in the engine.
    fn cancel(&self, id: u64) {
        if let Some(stop) = self.conversions.lock().unwrap().get(&id) {
            stop.store(true, Ordering::Relaxed);
            return;
        }
        if matches!(self.status(id), Some(Status::Confirming | Status::Waiting)) {
            self.discard_early(id);
            let url = self.hub.table.lock().unwrap().get(id).map(|e| e.url.clone());
            if let Some(url) = url {
                self.hub.publish(&Outgoing::Canceled { id, url });
            }
            return;
        }
        self.engine.cancel(id);
    }

    /// Queue `download`, remember it for retries, and announce it.
    /// `reuse_path` is a partial file an earlier attempt left behind.
    fn start(&self, download: Download, reuse_path: Option<PathBuf>) -> (u64, String) {
        self.start_resuming(download, reuse_path, None)
    }

    /// `start`, with where the earlier attempt kept its partial data when
    /// that was a temporary folder.
    fn start_resuming(
        &self,
        download: Download,
        reuse_path: Option<PathBuf>,
        reuse_partial: Option<PathBuf>,
    ) -> (u64, String) {
        let url = download.url.clone();
        let mut req = self.request_for(download.clone());
        req.reuse_path = reuse_path;
        req.reuse_partial = reuse_partial;
        let id = self.engine.enqueue(req);
        self.hub.requests.lock().unwrap().insert(id, download);
        // A very fast download can finish before its request is stored; drop
        // the request (and its cookies) in that case too.
        if self.status(id) == Some(Status::Done) {
            self.hub.requests.lock().unwrap().remove(&id);
        }
        self.hub.publish(&Outgoing::Added { id, url: url.clone() });
        (id, url)
    }

    fn status(&self, id: u64) -> Option<Status> {
        self.hub.table.lock().unwrap().get(id).map(|e| e.status)
    }

    /// Re-queue a failed, canceled or interrupted download from its stored
    /// request, or from its URL alone when the request was not kept. The
    /// engine resumes the partial file when the server still reports the same
    /// file.
    fn retry(&self, id: u64) -> Result<(u64, String), String> {
        let entry =
            self.hub.table.lock().unwrap().get(id).cloned().ok_or_else(|| format!("no download with id {id}"))?;
        if !entry.status.is_retryable() {
            return Err(format!("download {id} is not failed, canceled or interrupted"));
        }
        let download = self.hub.requests.lock().unwrap().remove(&id).unwrap_or_else(|| Download {
            url: entry.url.clone(),
            referrer: entry.referrer.clone(),
            page: entry.page.clone(),
            ..Download::default()
        });
        // The new entry keeps the old one's description and category, and
        // its speed limit unless the user chose not to remember it on stop
        // and resume.
        let download = Download {
            description: entry.description.clone(),
            category: entry.category.clone(),
            speed_limit: entry.speed_limit.filter(|_| entry.limit_remember),
            ..download
        };
        // A retried download keeps its place in its queue.
        let queue = self.hub.queue_of(id);
        self.hub.leave_queues(id);
        self.hub.publish(&Outgoing::Removed { id });
        // Only a download that started has a file of its own to resume. One
        // that never got past its save-as dialog only has a proposed path,
        // which may be somebody else's file.
        let started = entry.connections > 0;
        let reuse_path = entry.path.filter(|_| started).map(PathBuf::from);
        let reuse_partial = entry.partial.filter(|_| started).map(PathBuf::from);
        let (new_id, url) = self.start_resuming(download, reuse_path, reuse_partial);
        if !entry.limit_remember {
            let _ = self.set_limit_remember(new_id, false);
        }
        if let Some(q) = queue {
            let _ = self.move_to_queue(new_id, q, None);
        }
        Ok((new_id, url))
    }

    /// Set or clear download `id`'s own speed limit. It is kept on the
    /// entry (so in the history) and in the stored request, and a running
    /// download changes speed at once.
    fn set_download_limit(&self, id: u64, bytes_per_sec: Option<u64>) -> Result<(), String> {
        let limit = bytes_per_sec.filter(|b| *b > 0);
        let (lo, hi) = SPEED_LIMIT_RANGE_KBPS;
        if let Some(b) = limit.filter(|b| !(lo * 1024..=hi * 1024).contains(b)) {
            return Err(format!("{} KB/s is not a speed limit between {lo} and {hi} KB/s.", b / 1024));
        }
        self.hub.update(id, |e| {
            e.speed_limit = limit;
            Ok(())
        })?;
        if let Some(d) = self.hub.requests.lock().unwrap().get_mut(&id) {
            d.speed_limit = limit;
        }
        self.engine.set_download_limit(id, limit);
        Ok(())
    }

    /// Set whether download `id` keeps its own speed limit when it is
    /// stopped and resumed.
    fn set_limit_remember(&self, id: u64, remember: bool) -> Result<(), String> {
        self.hub.update(id, |e| {
            e.limit_remember = remember;
            Ok(())
        })?;
        Ok(())
    }

    /// Start download `id` again from the first byte, under the same id, so
    /// it keeps its place and description. It goes to the same name and
    /// folder; a file still there is handled by the file-exists setting, as
    /// anybody else's file would be.
    fn redownload(&self, id: u64) -> Result<(), String> {
        let again = |s: Status| matches!(s, Status::Done | Status::Failed | Status::Canceled | Status::Interrupted);
        let entry = self.entry(id).ok_or_else(|| format!("no download with id {id}"))?;
        if !again(entry.status) {
            return Err("Only a finished or stopped download can be downloaded again.".into());
        }
        let stored = self.hub.requests.lock().unwrap().get(&id).cloned();
        if stored.is_none() && entry.parts.is_some() {
            return Err(
                "This stream's playlist is no longer known. Play it in the browser again to download it.".into()
            );
        }
        let mut download = stored.unwrap_or_else(|| Download {
            url: entry.url.clone(),
            referrer: entry.referrer.clone(),
            page: entry.page.clone(),
            ..Download::default()
        });
        download.description = entry.description.clone();
        download.category = entry.category.clone();
        download.speed_limit = entry.speed_limit;
        // Only a download that started has a file of its own; a proposed
        // path may be somebody else's.
        let wrote = entry.status == Status::Done || entry.connections > 0;
        if let Some(path) = entry.path.as_deref().filter(|_| wrote).map(Path::new) {
            download.filename = path.file_name().map(|n| n.to_string_lossy().into_owned());
            download.folder = path.parent().map(|d| d.display().to_string());
        }
        // No reuse_path, and no resume sidecar left: a partial file still
        // there is a taken name like any other, not something to resume.
        if entry.status != Status::Done {
            for p in [entry.path.as_deref(), entry.partial.as_deref()].into_iter().flatten() {
                let _ = std::fs::remove_file(engine::sidecar_path(Path::new(p)));
            }
        }
        let req = self.request_for(download.clone());
        self.hub.update(id, |e| {
            if !again(e.status) {
                return Err("The download started again in the meantime.".into());
            }
            e.status = Status::Queued;
            e.done = 0;
            e.total = None;
            e.parts = None;
            e.error = None;
            e.scan = None;
            e.note = None;
            e.resumable = false;
            e.connections = 0;
            e.partial = None;
            Ok(())
        })?;
        self.hub.requests.lock().unwrap().insert(id, download);
        eprintln!("[uidm-daemon] downloading #{id} again: {}", entry.url);
        self.engine.enqueue_as(id, req);
        Ok(())
    }

    /// Give download `id` a new link. Without `download`, wait for the next one
    /// the browser hands over. With it, check the new link against the
    /// partial file: on a mismatch (and no `restart`) return why, changing
    /// nothing. Otherwise store the new request: a waiting download keeps
    /// waiting with it, any other starts again, resuming its partial file
    /// when the link is for the same file.
    fn refresh_link(&self, id: u64, download: Option<Download>, restart: bool) -> Result<Option<String>, String> {
        let entry = self.entry(id).ok_or_else(|| format!("no download with id {id}"))?;
        if !entry.status.is_retryable() {
            return Err("Only a stopped download can get a new link.".to_string());
        }
        let Some(download) = download else {
            self.set_refresh_waiting(Some(id));
            return Ok(None);
        };
        if !uidm::batch::is_download_url(&download.url) {
            return Err(format!("{} is not an http, https or ftp link", download.url));
        }
        if self.refresh_waiting.lock().unwrap().is_some_and(|w| w == id) {
            self.set_refresh_waiting(None);
        }
        let started = entry.connections > 0;
        if started && !restart {
            if let Some(path) = entry.partial.as_deref().or(entry.path.as_deref()) {
                let req = self.request_for(download.clone());
                let found = engine::inspect(&download.url, None, &req.headers, &req.net)
                    .map_err(|e| format!("could not reach the new link: {e}"))?;
                let ignore_date = self.settings.lock().unwrap().resume_ignores_date;
                if let Err(reason) = engine::partial_matches(Path::new(path), &found, ignore_date) {
                    return Ok(Some(reason));
                }
            }
        }
        eprintln!("[uidm-daemon] new link for #{id}: {}", download.url);
        self.hub.requests.lock().unwrap().insert(id, download.clone());
        if entry.status == Status::Waiting {
            self.hub.update(id, |e| {
                e.url = download.url.clone();
                Ok(())
            })?;
            return Ok(None);
        }
        self.retry(id)?;
        Ok(None)
    }

    /// Change a stopped or finished download's details. The link, referrer
    /// and login go into the request a retry starts from; a finished
    /// download's file is renamed or moved on disk first, and an unfinished
    /// one's partial file moves with its folder.
    fn set_properties(&self, id: u64, p: Properties) -> Result<(), String> {
        let entry = self.entry(id).ok_or_else(|| format!("no download with id {id}"))?;
        let done = entry.status == Status::Done;
        let stopped = |s: Status| s == Status::Done || s.is_retryable();
        if !stopped(entry.status) {
            return Err("Stop the download before changing its properties.".into());
        }
        let trimmed = |s: Option<String>| s.map(|s| s.trim().to_string());
        let url = trimmed(p.url).filter(|u| *u != entry.url);
        if let Some(u) = url.as_deref().filter(|u| !uidm::batch::is_download_url(u)) {
            return Err(format!("{u} is not an http, https or ftp link"));
        }
        let referrer = trimmed(p.referrer);
        if let Some(r) = referrer.as_deref().filter(|r| !r.is_empty() && !uidm::batch::is_web_url(r)) {
            return Err(format!("{r} is not an http or https address"));
        }
        let description = trimmed(p.description);
        if p.login.is_some() && done {
            return Err("A finished download keeps no login. Download it again to use one.".into());
        }
        let mut new_folder = None;
        let moved = match (p.filename, p.folder) {
            (None, None) => None,
            (Some(_), _) if !done => return Err("Only a finished download's file can be renamed.".into()),
            (None, Some(folder)) if !done => {
                // An unfinished download moves to another folder: its partial
                // file and resume sidecar go along when they are kept beside
                // it, and the next attempt saves there. A partial file in the
                // temporary folder stays put; it is moved into place when the
                // download finishes, as always.
                let dir = PathBuf::from(folder.trim());
                if !dir.is_absolute() {
                    return Err(format!("The folder must be a full path: {}", dir.display()));
                }
                new_folder = Some(dir.clone());
                entry
                    .path
                    .as_deref()
                    .map(PathBuf::from)
                    .and_then(|from| Some((from.file_name()?.to_owned(), from)))
                    .map(|(name, from)| (from, dir.join(name)))
                    .filter(|(from, to)| to != from)
            }
            (filename, folder) => {
                let from = entry.path.as_deref().map(PathBuf::from).ok_or("The download has no file.")?;
                let name = filename
                    .as_deref()
                    .map(engine::clean_filename)
                    .filter(|n| !n.is_empty())
                    .or_else(|| from.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .ok_or("The download has no file name.")?;
                let dir = match folder.as_deref().map(str::trim) {
                    Some(f) if !f.is_empty() => PathBuf::from(f),
                    _ => from.parent().map(Path::to_path_buf).ok_or("The download has no folder.")?,
                };
                if !dir.is_absolute() {
                    return Err(format!("The folder must be a full path: {}", dir.display()));
                }
                let to = dir.join(name);
                (to != from).then_some((from, to))
            }
        };
        if let Some((from, to)) = &moved {
            if done {
                move_file(from, to)?;
            } else if entry.connections > 0 {
                move_partial(from, to)?;
            } else if to.exists() {
                // Never started: only a proposed path, which may be
                // somebody else's file, so nothing is moved.
                return Err(format!("{} already exists.", to.display()));
            }
            eprintln!("[uidm-daemon] moved #{id} to {}", to.display());
        }
        let mut login_user = entry.login_user.clone();
        if !done {
            let mut requests = self.hub.requests.lock().unwrap();
            let req = requests.entry(id).or_insert_with(|| Download {
                url: entry.url.clone(),
                referrer: entry.referrer.clone(),
                page: entry.page.clone(),
                ..Download::default()
            });
            if let Some(u) = &url {
                req.url = u.clone();
            }
            if let Some(r) = &referrer {
                req.referrer = Some(r.clone()).filter(|r| !r.is_empty());
            }
            if let Some(d) = &description {
                req.description = Some(d.clone()).filter(|d| !d.is_empty());
            }
            if let Some(dir) = &new_folder {
                req.folder = Some(dir.display().to_string());
            }
            if let Some(l) = p.login {
                let username = l.username.trim().to_string();
                req.login = (!username.is_empty()).then(|| DownloadLogin {
                    password: l.password.or_else(|| req.login.as_ref().and_then(|o| o.password.clone())),
                    username,
                });
            }
            login_user = req.login.as_ref().map(|l| l.username.clone());
        }
        self.hub.update(id, |e| {
            if !stopped(e.status) {
                return Err("The download started while its properties were being saved.".into());
            }
            if let Some(u) = &url {
                e.url = u.clone();
            }
            if let Some(r) = &referrer {
                e.referrer = Some(r.clone()).filter(|r| !r.is_empty());
            }
            if let Some(d) = &description {
                e.description = Some(d.clone()).filter(|d| !d.is_empty());
            }
            if let Some((_, to)) = &moved {
                e.path = Some(to.display().to_string());
            }
            e.login_user = login_user;
            Ok(())
        })?;
        Ok(())
    }

    /// Change which download waits for a link from the browser, and tell
    /// every client.
    fn set_refresh_waiting(&self, id: Option<u64>) {
        *self.refresh_waiting.lock().unwrap() = id;
        self.hub.broadcast(&Outgoing::RefreshWaiting { id });
    }

    /// Validate, save and apply new settings, then tell every client. Running
    /// downloads keep going; new ones use the new values.
    /// The engine request for `download`, with the settings' folder,
    /// connections and file-name policy, and its proxy and logins.
    fn request_for(&self, download: Download) -> DownloadRequest {
        let settings = self.settings.lock().unwrap().clone();
        let net = self.net_for(&download, &settings);
        let folder = download.folder.as_deref().map(PathBuf::from).filter(|f| f.is_absolute());
        let mut req = to_request(download, &settings);
        req.net = Arc::new(net);
        if let Some(folder) = folder {
            req.folders = Folders::fixed(folder);
        }
        req
    }

    /// Under "the browser's proxy only after download errors", retry failed
    /// download `id` once, through the proxy its browser reported. Returns
    /// whether it did.
    fn retry_through_browser_proxy(&self, id: u64, s: &Settings) -> bool {
        if !(s.use_browser_proxy && s.browser_proxy_after_errors) {
            return false;
        }
        let url = self
            .hub
            .requests
            .lock()
            .unwrap()
            .get(&id)
            .filter(|d| d.proxy.is_some() || d.browser_proxy.is_some())
            .map(|d| d.url.clone());
        let Some(url) = url else {
            return false;
        };
        if !self.browser_proxy_retries.lock().unwrap().insert(url) {
            return false;
        }
        match self.retry(id) {
            Ok((new_id, _)) => {
                eprintln!("[uidm-daemon] #{id} failed; retrying it as #{new_id} through the browser's proxy");
                true
            }
            Err(e) => {
                eprintln!("[uidm-daemon] could not retry #{id} through the browser's proxy: {e}");
                false
            }
        }
    }

    /// The proxy and logins for `download`. A download from the browser uses
    /// the browser's proxy when the settings say so: always, or once it
    /// failed through UIDM's own setting.
    fn net_for(&self, download: &Download, s: &Settings) -> NetConfig {
        let browser_wanted = s.use_browser_proxy
            && (!s.browser_proxy_after_errors || self.browser_proxy_retries.lock().unwrap().contains(&download.url));
        // The browser's whole setting when it sent one (its direct
        // connection too, as `Some(None)`), else the fixed proxy an older
        // extension sends.
        let browser = match (&download.browser_proxy, download.proxy.as_deref()) {
            _ if !browser_wanted => None,
            (Some(setting), _) => Some(net::browser_proxy_rules(setting)),
            (None, Some(p)) => Some(Some(net::ProxyRules {
                http: p.strip_prefix("http://").map(str::to_string),
                https: p.strip_prefix("http://").map(str::to_string),
                socks: p.starts_with("socks").then(|| p.to_string()),
                ..net::ProxyRules::default()
            })),
            (None, None) => None,
        };
        let addr = |a: &str| (!a.trim().is_empty()).then(|| a.trim().to_string());
        let proxy_login = |user: &str, kind: ProxyKind| {
            if user.trim().is_empty() {
                return ProxyLogin::default();
            }
            let password = net::proxy_password_path(kind).map(|p| net::load_proxy_password(&p)).unwrap_or_default();
            ProxyLogin::new(user.trim(), &password)
        };
        let proxy = browser.unwrap_or_else(|| match s.proxy_mode {
            ProxyMode::None => None,
            ProxyMode::System => net::system_proxy(),
            ProxyMode::Script => Some(net::ProxyRules {
                pac: Some(uidm::pac::PacSource::Url(s.proxy_pac_url.trim().to_string())),
                ..net::ProxyRules::default()
            }),
            ProxyMode::Manual => {
                // An empty HTTPS proxy means the HTTP one, with its login.
                let (https, https_login) = match addr(&s.proxy_https) {
                    Some(a) => (Some(a), proxy_login(&s.proxy_https_username, ProxyKind::Https)),
                    None => (addr(&s.proxy_http), proxy_login(&s.proxy_username, ProxyKind::Http)),
                };
                Some(net::ProxyRules {
                    http: addr(&s.proxy_http),
                    https,
                    socks: addr(&s.proxy_socks5).map(|a| {
                        let scheme = net::socks_scheme(s.proxy_socks_version == SocksVersion::V5, s.proxy_remote_dns);
                        format!("{scheme}://{a}")
                    }),
                    // ftp:// goes through `NetConfig::ftp_proxy`, below.
                    ftp: None,
                    bypass: net::parse_bypass(&s.proxy_bypass),
                    http_login: proxy_login(&s.proxy_username, ProxyKind::Http),
                    https_login,
                    socks_login: proxy_login(&s.proxy_socks_username, ProxyKind::Socks),
                    socks_for_https: s.proxy_socks_https,
                    socks_for_ftp: s.proxy_socks_ftp,
                    pac: None,
                })
            }
        });
        let manual_ftp = (s.proxy_mode == ProxyMode::Manual).then(|| addr(&s.proxy_ftp)).flatten();
        let mut logins: Vec<Login> = s
            .site_logins
            .iter()
            .filter_map(|l| {
                Some(Login {
                    site: l.site.trim().to_string(),
                    username: l.username.clone(),
                    password: net::load_login_password(&l.site)?,
                    plain: l.plain,
                })
            })
            .collect();
        // The download's own login takes the place of the site logins for
        // its host, even those for one of its folders.
        let host = url::Url::parse(&download.url).ok().and_then(|u| u.host_str().map(str::to_string));
        if let (Some(l), Some(host)) = (&download.login, host) {
            logins.retain(|s| !net::site_matches(net::split_login_pattern(&s.site).0, &host));
            logins.push(Login {
                site: host,
                username: l.username.clone(),
                password: l.password.clone().unwrap_or_default(),
                // Typed for this download, so meant for its address.
                plain: true,
            });
        }
        NetConfig {
            proxy,
            logins,
            ftp_active: s.ftp_active,
            ftp_proxy_login: manual_ftp
                .as_ref()
                .map_or_else(ProxyLogin::default, |_| proxy_login(&s.proxy_ftp_username, ProxyKind::Ftp)),
            ftp_proxy: manual_ftp,
        }
    }

    /// Store the passwords a settings change carries (proxy: DPAPI; site
    /// logins: Credential Manager), forget those of removed logins, and return
    /// the settings without them.
    fn store_secrets(&self, mut new: Settings) -> Result<Settings, String> {
        for (kind, password) in [
            (ProxyKind::Http, new.proxy_password.take()),
            (ProxyKind::Https, new.proxy_https_password.take()),
            (ProxyKind::Ftp, new.proxy_ftp_password.take()),
            (ProxyKind::Socks, new.proxy_socks_password.take()),
        ] {
            if let Some(pw) = password {
                let path = net::proxy_password_path(kind).map_err(|e| e.to_string())?;
                net::save_proxy_password(&path, &pw).map_err(|e| format!("could not save the proxy password: {e}"))?;
            }
        }

        for l in &mut new.site_logins {
            if let Some(pw) = l.password.take() {
                net::save_login_password(&l.site, &l.username, &pw)
                    .map_err(|e| format!("could not save the password for {}: {e}", l.site))?;
            }
        }
        let old = self.settings.lock().unwrap().site_logins.clone();
        for gone in old.iter().filter(|o| !new.site_logins.iter().any(|n| n.site.eq_ignore_ascii_case(&o.site))) {
            if let Err(e) = net::delete_login_password(&gone.site) {
                eprintln!("[uidm-daemon] could not forget the password for {}: {e}", gone.site);
            }
        }
        Ok(new)
    }

    /// The settings as they are, with only the video button's fields taken
    /// from `asked`. The browser extension changes nothing else, and only
    /// the window changes settings at large: a browser connection, from an
    /// extension that is not UIDM's or a bug in it, must not point the virus
    /// scanner, ffmpeg, folders or proxy somewhere else.
    fn with_video_button(&self, asked: &Settings) -> Settings {
        let mut settings = self.settings.lock().unwrap().clone();
        settings.video_button = asked.video_button;
        settings.video_button_excluded_sites = asked.video_button_excluded_sites.clone();
        settings
    }

    fn set_settings(&self, mut new: Settings) -> Result<(), String> {
        new.validate()?;
        // Only the daemon notes update checks; a window's copy may be older.
        new.last_update_check = new.last_update_check.max(self.settings.lock().unwrap().last_update_check);
        let new = self.store_secrets(new)?;
        let old_autostart = self.settings.lock().unwrap().start_with_windows;
        if new.start_with_windows != old_autostart {
            uidm::autostart::set_autostart(new.start_with_windows)
                .map_err(|e| format!("could not change starting at sign-in: {e}"))?;
        }
        if let Some(path) = &self.paths.settings {
            new.save(path).map_err(|e| format!("could not save settings to {}: {e}", path.display()))?;
        }
        self.engine.set_concurrency(new.max_concurrent);
        self.engine.set_speed_limit(new.speed_limit_bytes());
        self.engine.set_per_file_limit(new.per_file_limit_bytes());
        *self.settings.lock().unwrap() = new.clone();
        self.hub.broadcast(&Outgoing::Settings { settings: new });
        Ok(())
    }

    /// Queue every interrupted download again (the "resume at startup"
    /// option).
    fn resume_interrupted(&self) {
        let ids: Vec<u64> = self
            .hub
            .table
            .lock()
            .unwrap()
            .entries()
            .filter(|e| e.status == Status::Interrupted)
            .map(|e| e.id)
            .collect();
        for id in ids {
            match self.retry(id) {
                Ok((new_id, url)) => eprintln!("[uidm-daemon] resuming #{id} as #{new_id}: {url}"),
                Err(e) => eprintln!("[uidm-daemon] could not resume #{id}: {e}"),
            }
        }
    }

    /// Write the history and the saved requests now.
    fn save_history(&self) {
        let _guard = self.save_lock.lock().unwrap();
        let history = History {
            next_id: self.engine.next_id(),
            items: self.hub.table.lock().unwrap().snapshot(),
            queues: self.hub.queues.lock().unwrap().clone(),
        };
        let requests = self.hub.requests.lock().unwrap().clone();
        if let Some(path) = &self.paths.quota {
            if let Err(e) = self.hub.quota.lock().unwrap().save(path) {
                eprintln!("[uidm-daemon] could not save {}: {e}", path.display());
            }
        }
        if let Some(path) = &self.paths.history {
            if let Err(e) = history.save(path) {
                eprintln!("[uidm-daemon] could not save {}: {e}", path.display());
            }
        }
        if let Some(path) = &self.paths.requests {
            if let Err(e) = history::save_requests(path, &requests) {
                eprintln!("[uidm-daemon] could not save {}: {e}", path.display());
            }
        }
    }

    /// Forget a download that is no longer running. Its files stay on disk
    /// unless `delete_files`, which sends them to the Recycle Bin.
    fn remove(&self, id: u64, delete_files: bool) -> Result<(), String> {
        let entry = match self.entry(id) {
            None => return Err(format!("no download with id {id}")),
            Some(e) if !e.status.is_terminal() => {
                return Err(format!("download {id} is still running; cancel it first"))
            }
            Some(e) => e,
        };
        // Only a download that started wrote anything. One that never got
        // past its save-as dialog only has a proposed path, which may be
        // somebody else's file.
        let wrote = entry.status == Status::Done || entry.connections > 0;
        let mut files = match entry.path.as_deref() {
            Some(p) if delete_files && wrote => engine::files_of(Path::new(p)),
            _ => Vec::new(),
        };
        if let Some(p) = entry.partial.as_deref().filter(|_| delete_files && wrote) {
            files.extend(engine::partial_files(Path::new(p)));
        }
        self.hub.requests.lock().unwrap().remove(&id);
        if self.refresh_waiting.lock().unwrap().is_some_and(|w| w == id) {
            self.set_refresh_waiting(None);
        }
        let in_queue = self.hub.leave_queues(id);
        self.hub.publish(&Outgoing::Removed { id });
        if in_queue {
            self.hub.publish_queues();
        }
        if !files.is_empty() {
            eprintln!("[uidm-daemon] recycling the files of #{id}");
            uidm::recycle::recycle(&files)
                .map_err(|e| format!("Removed {} from the list, but its files stay: {e}", entry.url))?;
        }
        Ok(())
    }

    /// Send the whole list to one client, under the table lock so no event can
    /// slip between the snapshot and the client's next frame.
    fn send_list(&self, reply: &Sender<Vec<u8>>) {
        let table = self.hub.table.lock().unwrap();
        let msg = Outgoing::Downloads { items: table.snapshot() };
        let queues = Outgoing::Queues { items: self.hub.queues.lock().unwrap().clone() };
        let refresh = Outgoing::RefreshWaiting { id: *self.refresh_waiting.lock().unwrap() };
        for msg in [msg, queues, refresh] {
            if let Ok(frame) = serde_json::to_vec(&msg) {
                let _ = reply.send(frame);
            }
        }
    }
}

fn main() {
    // Started by a download saved with administrator rights (see
    // `uidm::elevate`): move its finished file into place and exit.
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, from, to] = args.as_slice() {
        if flag == uidm::elevate::MOVE_ARG {
            if let Err(e) = uidm::elevate::move_file(Path::new(from), Path::new(to)) {
                eprintln!("[uidm-daemon] could not move {from} to {to}: {e}");
                std::process::exit(1);
            }
            return;
        }
    }
    // Started through WMI, out of an app package's copy of AppData, the
    // daemon opens its log itself.
    #[cfg(windows)]
    if std::env::args().any(|a| a == uidm::package::LOG_ARG) {
        if let Err(e) = uidm::package::stderr_to(&ipc::daemon_log()) {
            eprintln!("[uidm-daemon] could not open the log: {e}");
        }
    }
    if let Err(e) = run() {
        eprintln!("[uidm-daemon] fatal: {e}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    // Binding is the single-instance lock.
    let addr = ipc::daemon_addr();
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[uidm-daemon] {addr} unavailable ({e}); another daemon is likely running");
            return Ok(());
        }
    };
    eprintln!("[uidm-daemon] listening on {addr}");

    // Publish the token only after winning the bind. A daemon that lost the
    // bind must not overwrite the running daemon's token.
    let token = ipc::new_token()?;
    ipc::write_token(&token)?;
    let token: Arc<str> = token.into();
    match uidm::browsers::register_found() {
        Ok(names) if !names.is_empty() => eprintln!("[uidm-daemon] registered with {}", names.join(", ")),
        Ok(_) => {}
        Err(e) => eprintln!("[uidm-daemon] could not register with the browsers: {e}"),
    }

    let (daemon, dirty, notes, wake) = Daemon::new(Paths::from_env());
    clean_early_folders(&daemon);
    let daemon = Arc::new(daemon);
    spawn_scheduler(Arc::clone(&daemon), wake);
    spawn_history_writer(Arc::clone(&daemon), dirty);
    spawn_notifier(Arc::clone(&daemon), notes);
    #[cfg(windows)]
    if let Err(e) = register_notifications() {
        eprintln!("[uidm-daemon] could not register for notifications: {e}");
    }
    // Queues set to start with UIDM.
    let launch: Vec<u64> =
        daemon.hub.queues.lock().unwrap().iter().filter(|q| q.start_on_launch).map(|q| q.id).collect();
    for id in launch {
        if let Err(e) = daemon.start_queue(id, None) {
            eprintln!("[uidm-daemon] could not start queue {id}: {e}");
        }
    }
    if daemon.settings.lock().unwrap().resume_at_startup {
        daemon.resume_interrupted();
    }
    spawn_awake_keeper(Arc::clone(&daemon));
    spawn_idle_monitor(Arc::clone(&daemon));

    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let daemon = Arc::clone(&daemon);
                let token = Arc::clone(&token);
                thread::spawn(move || handle_client(stream, &daemon, &token));
            }
            Err(e) => eprintln!("[uidm-daemon] accept failed: {e}"),
        }
    }
    Ok(())
}

/// Serve one client until it disconnects. The client is registered for
/// broadcasts only after it authenticates.
fn handle_client(mut stream: TcpStream, daemon: &Arc<Daemon>, token: &str) {
    let id = NEXT_CLIENT.fetch_add(1, Ordering::Relaxed);
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".into());
    if let Err(e) = authenticate(&mut stream, token) {
        eprintln!("[uidm-daemon] client #{id} ({peer}) rejected: {e}");
        return;
    }
    eprintln!("[uidm-daemon] client #{id} connected ({peer})");

    let mut writer_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[uidm-daemon] client #{id}: {e}");
            return;
        }
    };
    let (tx, rx) = channel::<Vec<u8>>();
    daemon.hub.clients.lock().unwrap().insert(id, tx.clone());

    // One writer per client keeps its frames whole even when replies and
    // broadcasts race.
    let writer = thread::spawn(move || {
        while let Ok(frame) = rx.recv() {
            if write_frame(&mut writer_stream, &frame).is_err() {
                break;
            }
        }
    });

    let mut reader = stream;
    loop {
        match read_message(&mut reader, MAX_MESSAGE_BYTES) {
            Ok(Some(raw)) => dispatch(&raw, daemon, id, &tx),
            Ok(None) => break, // clean disconnect
            Err(e) => {
                eprintln!("[uidm-daemon] client #{id} read error: {e}");
                break;
            }
        }
    }

    // Removing the map's sender and dropping ours leaves the writer's channel
    // with no senders, so it drains what is queued and ends. Downloads this
    // client started keep running.
    daemon.hub.client_gone(id);
    daemon.stop_grab(id);
    daemon.stop_probe_links(id);
    drop(tx);
    let _ = writer.join();
    eprintln!("[uidm-daemon] client #{id} disconnected");
}

/// Run the daemon side of the handshake described in `uidm::ipc`.
fn authenticate(stream: &mut TcpStream, token: &str) -> io::Result<()> {
    // The whole handshake gets AUTH_TIMEOUT, not each read of it.
    let until = Instant::now() + AUTH_TIMEOUT;
    let client_nonce = match ipc::recv_handshake(&mut ipc::Deadline::new(stream, until))? {
        Handshake::AuthHello { client_nonce } => client_nonce,
        _ => return Err(denied("first frame was not auth_hello")),
    };
    let server_nonce = ipc::new_nonce()?;
    let proof = ipc::proof(token, DAEMON_LABEL, &client_nonce, &server_nonce);
    ipc::send_handshake(stream, &Handshake::AuthChallenge { server_nonce: server_nonce.clone(), proof })?;

    let expected = ipc::proof(token, HOST_LABEL, &client_nonce, &server_nonce);
    match ipc::recv_handshake(&mut ipc::Deadline::new(stream, until))? {
        Handshake::Auth { proof } if ipc::ct_eq(proof.as_bytes(), expected.as_bytes()) => {}
        _ => return Err(denied("missing or wrong auth proof")),
    }
    ipc::send_handshake(stream, &Handshake::AuthOk)?;

    stream.set_read_timeout(None)
}

fn denied(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, why.to_string())
}

/// Handle one request frame from client `client`, sending any direct reply to
/// `reply`.
fn dispatch(raw: &[u8], daemon: &Arc<Daemon>, client: u64, reply: &Sender<Vec<u8>>) {
    let send = |msg: &Outgoing| {
        if let Ok(frame) = serde_json::to_vec(msg) {
            let _ = reply.send(frame);
        }
    };
    let incoming: Incoming = match serde_json::from_slice(raw) {
        Ok(msg) => msg,
        Err(e) => {
            send(&Outgoing::Error { message: format!("invalid message: {e}") });
            return;
        }
    };

    // Anything from the extension shows it is still there.
    if daemon.hub.browsers.lock().unwrap().contains(&client) {
        *daemon.hub.browser_seen.lock().unwrap() = Some(uidm::state::unix_now());
    }
    match incoming {
        Incoming::Hello { client: kind, .. } => {
            if kind.is_none() {
                daemon.hub.browser_connected(client);
            }
            if kind.as_deref() == Some(WINDOW_CLIENT) {
                daemon.hub.windows.lock().unwrap().insert(client);
                send(&daemon.hub.browser_link());
                // A window that opens during a countdown shows it too.
                if let Some(countdown) = daemon.countdown_message() {
                    send(&countdown);
                }
                if let Some(quota) = daemon.quota_message() {
                    send(&quota);
                }
                daemon.weekly_update_check();
                // Links picked from a page while no window was open.
                if let Some(links) = daemon.picked_links.lock().unwrap().take() {
                    send(&links);
                }
                // The command line of the `uidm-gui` that opened this window.
                let held = daemon.command_line.lock().unwrap().take();
                if let Some((cmd, _)) = held.filter(|(_, at)| at.elapsed() < COMMAND_LINE_WAIT) {
                    daemon.run_command_line(cmd, true);
                }
            }
            send(&Outgoing::Hello { name: "uidm-daemon".to_string(), version: env!("CARGO_PKG_VERSION").to_string() })
        }
        Incoming::Ping => send(&Outgoing::Pong),
        Incoming::CommandLine(cmd) => {
            let window_open = daemon.hub.window_open();
            if window_open {
                daemon.run_command_line(cmd, false);
            } else {
                *daemon.command_line.lock().unwrap() = Some((cmd, Instant::now()));
            }
            send(&Outgoing::CommandLine { window_open });
        }
        Incoming::Add(mut download) => {
            download.limit_browser_request();
            if let Err(message) = check_body(&download) {
                send(&Outgoing::Rejected { url: download.url, message });
                return;
            }
            let (id, url) = daemon.add(download);
            eprintln!("[uidm-daemon] add #{id}: {url}");
            send(&Outgoing::Ack { id, url, status: "accepted".to_string() });
        }
        Incoming::AddMany { items, queue } => {
            let (count, skipped) = daemon.add_many(items, queue);
            eprintln!("[uidm-daemon] added {count} downloads to wait, skipped {skipped} already in the list");
            send(&Outgoing::AddedMany { count, skipped });
        }
        Incoming::ProbeMedia { request, mut download } => {
            download.limit_browser_request();
            if let Err(message) = check_body(&download) {
                send(&Outgoing::Error { message });
                return;
            }
            daemon.probe_media(request, download, reply.clone());
        }
        Incoming::Grab(req) => {
            if let Err(message) = daemon.grab(client, req, reply.clone()) {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::CancelGrab => daemon.stop_grab(client),
        Incoming::ProbeLinks { request, links } => {
            if let Some(message) = links.iter().find_map(|d| check_body(d).err()) {
                send(&Outgoing::Error { message });
                return;
            }
            daemon.probe_links(client, request, links, reply.clone());
        }
        Incoming::StopProbeLinks => daemon.stop_probe_links(client),
        Incoming::Cancel { id } => {
            eprintln!("[uidm-daemon] cancel #{id}");
            daemon.cancel(id);
        }
        Incoming::Confirm(answer) => {
            let (id, start) = (answer.id, answer.start);
            match daemon.confirm(answer) {
                Ok(()) => eprintln!("[uidm-daemon] confirm #{id} (start: {start})"),
                Err(message) => send(&Outgoing::Error { message }),
            }
        }
        Incoming::Retry { id } => match daemon.retry(id) {
            Ok((new_id, url)) => {
                eprintln!("[uidm-daemon] retry #{id} as #{new_id}");
                send(&Outgoing::Ack { id: new_id, url, status: "accepted".to_string() });
            }
            Err(message) => send(&Outgoing::Error { message }),
        },
        Incoming::Remove { id, delete_files } => {
            if let Err(message) = daemon.remove(id, delete_files) {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::List => daemon.send_list(reply),
        Incoming::SaveQueue { queue } => match daemon.save_queue(queue) {
            Ok(id) => eprintln!("[uidm-daemon] queue {id} saved"),
            Err(message) => send(&Outgoing::Error { message }),
        },
        Incoming::DeleteQueue { id } => {
            if let Err(message) = daemon.delete_queue(id) {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::StartQueue { id } => match daemon.start_queue(id, None) {
            Ok(()) => {
                eprintln!("[uidm-daemon] start queue {id}");
                let _ = daemon.hub.wake.send(());
            }
            Err(message) => send(&Outgoing::Error { message }),
        },
        Incoming::StopQueue { id } => match daemon.stop_queue(id) {
            Ok(()) => eprintln!("[uidm-daemon] stop queue {id}"),
            Err(message) => send(&Outgoing::Error { message }),
        },
        Incoming::SetQueueOnce { id, when_done } => {
            if let Err(message) = daemon.hub.update_queue(id, |q| {
                q.when_done_once = when_done;
                Ok(())
            }) {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::MoveToQueue { id, queue, position } => {
            if let Err(message) = daemon.move_to_queue(id, queue, position) {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::RemoveFromQueue { id } => {
            if let Err(message) = daemon.remove_from_queue(id) {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::CancelCountdown => daemon.cancel_countdown(),
        Incoming::CountdownNow => daemon.countdown_now(),
        Incoming::RefreshLink { id, mut download, restart } => {
            if let Some(d) = &mut download {
                d.limit_browser_request();
            }
            let url = download.as_ref().map(|d| d.url.clone()).unwrap_or_default();
            if let Some(d) = &download {
                if let Err(message) = check_body(d) {
                    send(&Outgoing::Error { message });
                    return;
                }
            }
            match daemon.refresh_link(id, download, restart) {
                Ok(Some(reason)) => send(&Outgoing::RefreshMismatch { id, url, reason }),
                Ok(None) => {}
                Err(message) => send(&Outgoing::Error { message }),
            }
        }
        Incoming::CancelRefresh => daemon.set_refresh_waiting(None),
        Incoming::PickLinks { page, mut links, user_agent } => {
            links.retain(|l| uidm::batch::is_web_url(&l.url));
            links.truncate(uidm::batch::MAX_BATCH);
            eprintln!("[uidm-daemon] {} links from {page} to pick from", links.len());
            let msg = Outgoing::PickLinks { page, links, user_agent };
            if daemon.hub.window_open() {
                daemon.hub.broadcast(&msg);
                daemon.hub.broadcast(&Outgoing::Show);
            } else {
                *daemon.picked_links.lock().unwrap() = Some(msg);
                daemon.show_window();
            }
        }
        Incoming::PreviewZip { id } => daemon.preview_zip(id, reply.clone()),
        Incoming::Convert { id, to, delete_original } => {
            if let Err(message) = daemon.convert(id, to, delete_original) {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::InstallUpdate => {
            let (daemon, reply) = (Arc::clone(daemon), reply.clone());
            thread::spawn(move || {
                if let Err(error) = daemon.install_update() {
                    if let Ok(frame) = serde_json::to_vec(&Outgoing::UpdateFailed { error }) {
                        let _ = reply.send(frame);
                    }
                }
            });
        }
        Incoming::CheckUpdate => {
            let (daemon, reply) = (Arc::clone(daemon), reply.clone());
            thread::spawn(move || {
                let msg = daemon.check_update(uidm::update::LATEST_RELEASE);
                if let Ok(frame) = serde_json::to_vec(&msg) {
                    let _ = reply.send(frame);
                }
            });
        }
        Incoming::Rendered { request, html, url, popups } => {
            if let Some(tx) = daemon.render_waits.lock().unwrap().remove(&request) {
                let _ = tx.send(html.zip(url).map(|(h, u)| (h, u, popups)));
            }
        }
        Incoming::Cookies { request, cookies } => {
            if let Some(tx) = daemon.cookie_waits.lock().unwrap().remove(&request) {
                let _ = tx.send(cookies);
            }
        }
        Incoming::SetSpeedLimit { id, bytes_per_sec, remember } => {
            let set = daemon
                .set_download_limit(id, bytes_per_sec)
                .and_then(|()| remember.map_or(Ok(()), |r| daemon.set_limit_remember(id, r)));
            if let Err(message) = set {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::SetOnDone { id, on_done } => {
            if let Err(message) = daemon.set_on_done(id, on_done) {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::Redownload { id } => {
            if let Err(message) = daemon.redownload(id) {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::SetProperties { id, changes } => {
            if let Err(message) = daemon.set_properties(id, changes) {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::PauseAll => {
            eprintln!("[uidm-daemon] pause all");
            daemon.pause_all();
        }
        Incoming::ResumeAll => {
            eprintln!("[uidm-daemon] resume all");
            daemon.resume_all();
        }
        Incoming::GetSettings => send(&Outgoing::Settings { settings: daemon.settings.lock().unwrap().clone() }),
        Incoming::SetSettings { settings } => {
            match daemon.set_settings(if daemon.hub.windows.lock().unwrap().contains(&client) {
                settings
            } else {
                daemon.with_video_button(&settings)
            }) {
                Ok(()) => eprintln!("[uidm-daemon] settings changed"),
                Err(message) => send(&Outgoing::Error { message }),
            }
        }
    }
}

/// Run the queue scheduler: a pass whenever something wakes it (a download
/// ended, a queue changed) and every few seconds for the clock.
fn spawn_scheduler(daemon: Arc<Daemon>, wake: Receiver<()>) {
    thread::spawn(move || loop {
        match wake.recv_timeout(SCHEDULER_POLL) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                while wake.try_recv().is_ok() {}
                daemon.tick();
                daemon.check_grab_schedules();
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        }
    });
}

/// Run notifications on their own thread, so a slow toast or a scan never
/// holds up downloads.
fn spawn_notifier(daemon: Arc<Daemon>, notes: Receiver<Note>) {
    thread::spawn(move || {
        while let Ok(note) = notes.recv() {
            daemon.on_note(note);
        }
    });
}

/// Give UIDM's notifications a name and icon. Unpackaged apps need this
/// registration before Windows shows their notifications.
#[cfg(windows)]
fn register_notifications() -> io::Result<()> {
    let icon = history::history_path()?.with_file_name("uidm.png");
    if std::fs::read(&icon).ok().as_deref() != Some(ICON_PNG) {
        settings::write_atomic(&icon, ICON_PNG)?;
    }
    registry::register_app_id(&icon)
}

/// What a notification says about `e`.
fn toast_for(e: &DownloadEntry, failed: bool) -> Toast {
    let path = e.path.as_deref().map(Path::new);
    let name =
        path.and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| e.url.clone());
    let id = e.id;
    if failed {
        return Toast {
            title: "Download failed".into(),
            lines: vec![name, e.error.clone().unwrap_or_default()],
            buttons: vec![("Retry".into(), format!("retry:{id}"))],
            launch: format!("window:{id}"),
        };
    }
    let folder = path.and_then(|p| p.parent()).map(|p| p.display().to_string()).unwrap_or_default();
    let size = e.total.map(human_bytes).unwrap_or_default();
    let detail = [size, folder].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" in ");
    let show = ("Show in folder".to_string(), format!("show:{id}"));
    match &e.scan {
        Some(Scan::Threat { .. }) => Toast {
            title: "Threat found in a download".into(),
            lines: vec![name, "The virus scan found a threat. Open UIDM to decide what to do.".into()],
            buttons: vec![show],
            launch: format!("window:{id}"),
        },
        scan => Toast {
            title: match scan {
                Some(Scan::Failed { .. }) => "Download complete (virus scan failed)".into(),
                _ => "Download complete".into(),
            },
            lines: vec![name, detail],
            buttons: vec![("Open".into(), format!("open:{id}")), show],
            launch: format!("window:{id}"),
        },
    }
}

fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Refuse a POST whose body is not valid base64, rather than send another
/// body than the form's.
fn check_body(d: &Download) -> Result<(), String> {
    use base64::Engine as _;
    match &d.body {
        Some(body) if base64::engine::general_purpose::STANDARD.decode(body).is_err() => {
            Err(format!("the form body of {} is not valid base64", d.url))
        }
        _ => Ok(()),
    }
}

/// `path`, or when a file is there, the first free numbered name next to
/// it (`name (1).ext`, ...), unless `overwrite` says to remove that file.
fn free_path(path: &Path, overwrite: bool) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    if overwrite && std::fs::remove_file(path).is_ok() {
        return path.to_path_buf();
    }
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return path.to_path_buf();
    };
    let name = name.to_string_lossy();
    (1..).map(|n| dir.join(engine::numbered_name(&name, n))).find(|p| !p.exists()).unwrap_or_else(|| path.to_path_buf())
}

/// Delete the folders of early downloads a previous run left behind, except
/// those holding the partial file of a download in the list.
fn clean_early_folders(daemon: &Daemon) {
    let Ok(dirs) = std::fs::read_dir(early_root()) else {
        return;
    };
    let partials: Vec<PathBuf> =
        daemon.hub.table.lock().unwrap().entries().filter_map(|e| e.partial.as_deref().map(PathBuf::from)).collect();
    for dir in dirs.flatten().map(|d| d.path()) {
        if !partials.iter().any(|p| p.starts_with(&dir)) {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

/// Move a finished file to `to`, which must not exist yet. Across drives
/// this copies the file, then deletes the original.
fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if !from.is_file() {
        return Err(format!("The file is no longer at {}.", from.display()));
    }
    if to.exists() {
        return Err(format!("{} already exists.", to.display()));
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map_err(|e| format!("Could not move the file to {}: {e}", to.display()))?;
    std::fs::remove_file(from).map_err(|e| format!("Copied the file, but could not delete {}: {e}", from.display()))
}

/// Move an unfinished download's file `from` and its resume sidecar to
/// `to`, refusing to land on any file already there. Either may be missing:
/// a download that never started has no file yet, and one writing to the
/// temporary folder keeps its data there.
fn move_partial(from: &Path, to: &Path) -> Result<(), String> {
    let (sidecar, new_sidecar) = (engine::sidecar_path(from), engine::sidecar_path(to));
    for taken in [to, new_sidecar.as_path()] {
        if taken.exists() {
            return Err(format!("{} already exists.", taken.display()));
        }
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    if from.is_file() {
        move_file(from, to)?;
    }
    if sidecar.is_file() {
        move_file(&sidecar, &new_sidecar)?;
    }
    Ok(())
}

/// Keep a page a crawl read under its mirror folder, with its links to
/// kept pages and files pointing at the local copies. Not robots.txt, and
/// not pages that are not HTML.
fn save_page(req: &GrabRequest, m: &grab::Mirror, page: &grab::Page) {
    let html = page.content_type.as_deref().is_none_or(|t| t.to_ascii_lowercase().contains("html"));
    let Ok(url) = url::Url::parse(&page.url) else {
        return;
    };
    if !html || url.path() == "/robots.txt" {
        return;
    }
    let root = Path::new(m.folder.trim());
    let here = grab::mirror_path(root, &url, true);
    if here.exists() && !m.overwrite {
        return;
    }
    let text = String::from_utf8_lossy(&page.body);
    let out = grab::rewrite_links(&text, &url, &here, root, |u| grab::kept_locally(req, u));
    let written = here
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| uidm::settings::write_atomic(&here, out.as_bytes()));
    if let Err(e) = written {
        eprintln!("[uidm-daemon] could not save {}: {e}", here.display());
    }
}

/// Sleep, hibernate or shut down now, unless a test asked for a dry run.
fn power_off(action: WhenDone, force_close: bool) {
    if std::env::var_os(POWER_DRY_RUN_ENV).is_some() {
        eprintln!("[uidm-daemon] dry run: would {}", action.label());
        return;
    }
    eprintln!("[uidm-daemon] {}", action.label());
    if let Err(e) = power::perform(action, force_close) {
        eprintln!("[uidm-daemon] could not {}: {e}", action.label());
    }
}

/// Open the program or file a queue names for the end of its run, as
/// Explorer would on a double-click.
fn open_after_run(file: &str) {
    eprintln!("[uidm-daemon] opening {file}");
    if let Err(e) = uidm::platform::open(file.as_ref()) {
        eprintln!("[uidm-daemon] could not open {file}: {e}");
    }
}

/// Start the desktop window from this daemon's folder.
fn open_window() -> io::Result<()> {
    uidm::platform::program_command("uidm-gui")?
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

/// Write the history shortly after it changes, batching bursts of events
/// (progress, a cleared list) into one write.
fn spawn_history_writer(daemon: Arc<Daemon>, dirty: Receiver<()>) {
    thread::spawn(move || {
        while dirty.recv().is_ok() {
            thread::sleep(HISTORY_DEBOUNCE);
            while dirty.try_recv().is_ok() {}
            daemon.save_history();
        }
    });
}

/// Exit the process once it has been idle (no clients, no downloads) for
/// `IDLE_EXIT`, so a daemon started by the browser does not linger forever.
/// Hold Windows awake while anything downloads, a site grab runs or a
/// video converts, when the "keep awake" option is on. One thread owns the
/// request, since Windows ties it to the thread that made it.
fn spawn_awake_keeper(daemon: Arc<Daemon>) {
    thread::spawn(move || {
        let mut held = false;
        loop {
            let want = daemon.settings.lock().unwrap().keep_awake && daemon.is_working();
            if want != held {
                power::keep_awake(want);
                held = want;
            }
            thread::sleep(AWAKE_POLL);
        }
    });
}

fn spawn_idle_monitor(daemon: Arc<Daemon>) {
    thread::spawn(move || {
        let mut idle_since: Option<Instant> = None;
        loop {
            thread::sleep(IDLE_POLL);
            let idle = daemon.hub.clients.lock().unwrap().is_empty()
                && daemon.engine.active_count() == 0
                && !daemon.queues_need_daemon();
            match (idle, idle_since) {
                (true, None) => idle_since = Some(Instant::now()),
                (true, Some(t)) if t.elapsed() >= IDLE_EXIT => {
                    eprintln!("[uidm-daemon] idle for {}s, exiting", IDLE_EXIT.as_secs());
                    daemon.save_history();
                    std::process::exit(0);
                }
                (false, _) => idle_since = None,
                _ => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    const BODY: &[u8] = b"the new bytes";

    /// A server that answers every request with `BODY`, without ranges.
    fn serve() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                thread::spawn(move || {
                    let mut req = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buf) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => req.extend_from_slice(&buf[..n]),
                        }
                    }
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                        BODY.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    if !req.starts_with(b"HEAD") {
                        let _ = stream.write_all(BODY);
                    }
                });
            }
        });
        format!("http://{addr}/f.bin")
    }

    /// A server that sends `body` in byte ranges, slowly, counting the bytes
    /// it sends in `sent`.
    fn serve_ranges(body: Arc<Vec<u8>>, sent: Arc<AtomicU64>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let (body, sent) = (Arc::clone(&body), Arc::clone(&sent));
                thread::spawn(move || {
                    let mut req = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buf) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => req.extend_from_slice(&buf[..n]),
                        }
                    }
                    let text = String::from_utf8_lossy(&req).to_ascii_lowercase();
                    let len = body.len() as u64;
                    let range = text
                        .lines()
                        .find_map(|l| l.strip_prefix("range: bytes="))
                        .and_then(|r| r.trim().split_once('-'))
                        .and_then(|(a, b)| {
                            Some((a.parse::<u64>().ok()?, b.parse::<u64>().unwrap_or(len - 1).min(len - 1)))
                        });
                    let (status, start, end, extra) = match range {
                        Some((a, b)) => {
                            ("206 Partial Content", a, b, format!("Content-Range: bytes {a}-{b}/{len}\r\n"))
                        }
                        None => ("200 OK", 0, len - 1, String::new()),
                    };
                    let head = format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nETag: \"e1\"\r\n{extra}Connection: close\r\n\r\n",
                        end + 1 - start
                    );
                    if stream.write_all(head.as_bytes()).is_err() || req.starts_with(b"HEAD") {
                        return;
                    }
                    for chunk in body[start as usize..=end as usize].chunks(4096) {
                        if stream.write_all(chunk).is_err() {
                            return;
                        }
                        sent.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                        thread::sleep(Duration::from_millis(15));
                    }
                });
            }
        });
        format!("http://{addr}/big.bin")
    }

    struct Setup {
        daemon: Arc<Daemon>,
        dir: PathBuf,
        /// What a pretend window receives.
        window: Receiver<Vec<u8>>,
    }

    /// A daemon with no files, saving to a fresh folder, with a pretend
    /// window connected so nothing opens on screen.
    fn setup(name: &str, choice: Duplicate) -> Setup {
        let dir = std::env::temp_dir().join(format!("uidm_dup_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (daemon, ..) = Daemon::new(Paths { settings: None, history: None, requests: None, quota: None });
        {
            let mut s = daemon.settings.lock().unwrap();
            s.download_dir = Some(dir.display().to_string());
            s.ask_where_to_save = false;
            s.if_duplicate = choice;
        }
        let (tx, window) = channel();
        daemon.hub.clients.lock().unwrap().insert(1, tx);
        daemon.hub.windows.lock().unwrap().insert(1);
        Setup { daemon: Arc::new(daemon), dir, window }
    }

    impl Setup {
        /// Put a finished download of `url` in the list, its file at `path`
        /// holding `old`.
        fn finished(&self, url: &str, path: &Path) -> u64 {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"old").unwrap();
            let id = self.daemon.engine.reserve_id();
            let url = url.to_string();
            self.daemon.hub.publish(&Outgoing::Added { id, url: url.clone() });
            self.daemon.hub.publish(&Outgoing::Done { id, url, path: path.display().to_string(), note: None });
            id
        }

        fn failed(&self, url: &str) -> u64 {
            let id = self.daemon.engine.reserve_id();
            let url = url.to_string();
            self.daemon.hub.publish(&Outgoing::Added { id, url: url.clone() });
            self.daemon.hub.publish(&Outgoing::Failed { id, url, error: "gone".into() });
            id
        }

        fn add(&self, url: &str) -> u64 {
            self.daemon.add(Download { url: url.into(), ..Download::default() }).0
        }

        fn len(&self) -> usize {
            self.daemon.hub.table.lock().unwrap().len()
        }

        /// Wait for download `id` to reach `status`, and return it.
        fn wait(&self, id: u64, status: Status) -> DownloadEntry {
            let until = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(e) = self.daemon.entry(id).filter(|e| e.status == status) {
                    return e;
                }
                assert!(Instant::now() < until, "#{id} never became {status:?}: {:?}", self.daemon.entry(id));
                thread::sleep(Duration::from_millis(20));
            }
        }

        fn window_got(&self, wanted: impl Fn(&Outgoing) -> bool) -> bool {
            self.window.try_iter().filter_map(|f| serde_json::from_slice::<Outgoing>(&f).ok()).any(|m| wanted(&m))
        }
    }

    #[test]
    fn the_windows_hear_when_the_browser_extension_connects_and_goes() {
        let t = setup("browser_link", Duplicate::Ask);
        assert!(matches!(t.daemon.hub.browser_link(), Outgoing::BrowserLink { connected: false, last_seen: None }));

        // The extension says hello with no `client`.
        let (tx, _browser) = channel();
        t.daemon.hub.clients.lock().unwrap().insert(2, tx.clone());
        dispatch(br#"{"op":"hello","version":"1"}"#, &t.daemon, 2, &tx);
        assert!(t.window_got(|m| matches!(m, Outgoing::BrowserLink { connected: true, .. })));
        assert!(t.daemon.hub.browser_seen.lock().unwrap().is_some());

        // A second one is no news; the first going is none either.
        let (tx3, _browser3) = channel();
        t.daemon.hub.clients.lock().unwrap().insert(3, tx3.clone());
        dispatch(br#"{"op":"hello"}"#, &t.daemon, 3, &tx3);
        t.daemon.hub.client_gone(2);
        assert!(!t.window_got(|m| matches!(m, Outgoing::BrowserLink { .. })));

        // Anything it sends keeps it seen.
        *t.daemon.hub.browser_seen.lock().unwrap() = Some(1);
        dispatch(br#"{"op":"ping"}"#, &t.daemon, 3, &tx3);
        assert!(t.daemon.hub.browser_seen.lock().unwrap().is_some_and(|s| s > 1));

        // The last one going is, with when.
        t.daemon.hub.client_gone(3);
        assert!(t.window_got(|m| matches!(m, Outgoing::BrowserLink { connected: false, last_seen: Some(s) } if *s > 1)));
    }

    #[test]
    fn a_command_line_runs_in_the_open_window_or_waits_for_the_new_one() {
        let t = setup("command_line", Duplicate::NewName);
        let entry = |url: &str| t.daemon.hub.table.lock().unwrap().entries().find(|e| e.url == url).cloned();
        let ask = |url: &str| -> (Outgoing, Receiver<Vec<u8>>) {
            let (tx, rx) = channel();
            let msg =
                format!(r#"{{"op":"command_line","download":{{"url":"{url}","start":false}},"quit_after":true}}"#);
            dispatch(msg.as_bytes(), &t.daemon, 9, &tx);
            let reply = rx.try_iter().filter_map(|f| serde_json::from_slice::<Outgoing>(&f).ok()).last().unwrap();
            (reply, rx)
        };

        // A window is open: it is added now, and /q does nothing, as it was
        // not this call that started UIDM.
        let (reply, _) = ask("https://example.com/open.bin");
        assert!(matches!(reply, Outgoing::CommandLine { window_open: true }));
        let e = entry("https://example.com/open.bin").expect("added now");
        assert_eq!(e.status, Status::Waiting, "/a only adds it");
        assert_eq!(e.on_done, None);

        // No window: it waits for the one this call opens, then has /q.
        t.daemon.hub.windows.lock().unwrap().clear();
        let (reply, _) = ask("https://example.com/new.bin");
        assert!(matches!(reply, Outgoing::CommandLine { window_open: false }));
        assert!(entry("https://example.com/new.bin").is_none(), "held for the window");
        let (tx, _rx) = channel();
        dispatch(br#"{"op":"hello","client":"window"}"#, &t.daemon, 1, &tx);
        let e = entry("https://example.com/new.bin").expect("added once the window says hello");
        assert_eq!(e.on_done.map(|o| o.then), Some(WhenDone::Exit));

        // One held too long belonged to a window that never came.
        t.daemon.hub.windows.lock().unwrap().clear();
        ask("https://example.com/stale.bin");
        let held = t.daemon.command_line.lock().unwrap().take().unwrap().0;
        let old = Instant::now().checked_sub(COMMAND_LINE_WAIT + Duration::from_secs(1)).unwrap();
        *t.daemon.command_line.lock().unwrap() = Some((held, old));
        dispatch(br#"{"op":"hello","client":"window"}"#, &t.daemon, 1, &tx);
        assert!(entry("https://example.com/stale.bin").is_none());
    }

    #[test]
    fn new_name_keeps_the_old_file() {
        let t = setup("new_name", Duplicate::NewName);
        let url = serve();
        let old = t.finished(&url, &t.dir.join("f.bin"));
        let new = t.add(&format!("{url}#again"));
        assert_ne!(new, old);
        let done = t.wait(new, Status::Done);
        assert_eq!(done.path.map(PathBuf::from), Some(t.dir.join("f (1).bin")));
        assert_eq!(std::fs::read(t.dir.join("f.bin")).unwrap(), b"old");
        assert!(t.daemon.entry(old).is_some(), "the old download stays in the list");
    }

    #[test]
    fn overwrite_replaces_the_old_download_and_its_file() {
        let t = setup("overwrite", Duplicate::Overwrite);
        let url = serve();
        // Saved somewhere its category would not pick now.
        let path = t.dir.join("elsewhere").join("f.bin");
        let old = t.finished(&url, &path);
        let new = t.add(&url);
        let done = t.wait(new, Status::Done);
        assert_eq!(done.path.map(PathBuf::from), Some(path.clone()));
        assert_eq!(std::fs::read(&path).unwrap(), BODY);
        assert!(t.daemon.entry(old).is_none(), "the old download left the list");
    }

    #[test]
    fn overwrite_keeps_a_running_download() {
        let t = setup("overwrite_running", Duplicate::Overwrite);
        let url = serve();
        let old = t.daemon.engine.reserve_id();
        t.daemon.hub.publish(&Outgoing::Added { id: old, url: url.clone() });
        assert_eq!(t.daemon.status(old), Some(Status::Queued));
        assert_eq!(t.add(&url), old);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn existing_shows_a_finished_download_again() {
        let t = setup("existing_done", Duplicate::Existing);
        let url = serve();
        let old = t.finished(&url, &t.dir.join("f.bin"));
        assert_eq!(t.add(&url), old);
        assert_eq!(t.len(), 1, "nothing was added");
        assert!(t.window_got(|m| matches!(m, Outgoing::ShowDone { id } if *id == old)));
    }

    #[test]
    fn existing_adds_anyway_when_the_finished_file_is_gone() {
        let t = setup("existing_gone", Duplicate::Existing);
        let url = serve();
        let old = t.finished(&url, &t.dir.join("f.bin"));
        std::fs::remove_file(t.dir.join("f.bin")).unwrap();
        let new = t.add(&url);
        assert_ne!(new, old);
        t.wait(new, Status::Done);
    }

    #[test]
    fn existing_resumes_a_stopped_download() {
        let t = setup("existing_failed", Duplicate::Existing);
        let url = serve();
        let old = t.failed(&url);
        let resumed = t.add(&url);
        assert_ne!(resumed, old, "a retry gets a new id");
        t.wait(resumed, Status::Done);
        assert!(t.daemon.entry(old).is_none());
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn ask_holds_it_in_the_dialog_then_follows_the_answer() {
        let t = setup("ask", Duplicate::Ask);
        let url = serve();
        let old = t.finished(&url, &t.dir.join("f.bin"));
        let new = t.add(&url);
        let asked = t.wait(new, Status::Confirming);
        assert_eq!(asked.duplicate_of, Some(old));
        t.daemon.confirm(Answer { duplicate: Some(Duplicate::Existing), ..Answer::start(new) }).unwrap();
        assert!(t.daemon.entry(new).is_none(), "the new download was not added");
        assert!(t.window_got(|m| matches!(m, Outgoing::ShowDone { id } if *id == old)));

        // No answer about the duplicate means a new name.
        let again = t.add(&url);
        t.wait(again, Status::Confirming);
        t.daemon.confirm(Answer::start(again)).unwrap();
        let done = t.wait(again, Status::Done);
        assert_eq!(done.path.map(PathBuf::from), Some(t.dir.join("f (1).bin")));
        assert_eq!(done.duplicate_of, None);
    }

    #[test]
    fn ask_then_overwrite_writes_over_the_old_file() {
        let t = setup("ask_overwrite", Duplicate::Ask);
        let url = serve();
        let path = t.dir.join("elsewhere").join("f.bin");
        let old = t.finished(&url, &path);
        let new = t.add(&url);
        t.wait(new, Status::Confirming);
        t.daemon
            .confirm(Answer {
                filename: Some("ignored.bin".into()),
                duplicate: Some(Duplicate::Overwrite),
                ..Answer::start(new)
            })
            .unwrap();
        let done = t.wait(new, Status::Done);
        assert_eq!(done.path.map(PathBuf::from), Some(path.clone()));
        assert_eq!(std::fs::read(&path).unwrap(), BODY);
        assert!(t.daemon.entry(old).is_none());
    }

    #[test]
    fn add_many_skips_duplicates_unless_told_to_add_them() {
        let url = serve();
        let items = |urls: &[&str]| {
            urls.iter().map(|u| Download { url: u.to_string(), ..Download::default() }).collect::<Vec<_>>()
        };
        let t = setup("many_ask", Duplicate::Ask);
        t.finished(&url, &t.dir.join("f.bin"));
        assert_eq!(t.daemon.add_many(items(&[&url, "https://example.com/other.bin"]), None), (1, 1));

        let t = setup("many_new_name", Duplicate::NewName);
        t.finished(&url, &t.dir.join("f.bin"));
        assert_eq!(t.daemon.add_many(items(&[&url]), None), (1, 0));

        // An item's own answer wins over the setting.
        let t = setup("many_answered", Duplicate::Ask);
        t.finished(&url, &t.dir.join("f.bin"));
        let mut answered = items(&[&url]);
        answered[0].duplicate = Some(Duplicate::NewName);
        assert_eq!(t.daemon.add_many(answered, None), (1, 0));
    }

    #[test]
    fn form_posts_are_never_duplicates() {
        let t = setup("post", Duplicate::Existing);
        let url = "https://example.com/form";
        t.finished(url, &t.dir.join("form"));
        let get = Download { url: url.into(), ..Download::default() };
        assert!(t.daemon.duplicate_of(&get).is_some());
        let post = Download { method: Some("POST".into()), ..get };
        assert!(t.daemon.duplicate_of(&post).is_none());
    }

    #[test]
    fn properties_rename_and_move_a_finished_file() {
        let t = setup("props_move", Duplicate::NewName);
        let id = t.finished("https://x/f.bin", &t.dir.join("f.bin"));
        let changes = Properties {
            filename: Some("renamed.bin".into()),
            folder: Some(t.dir.join("sub").display().to_string()),
            description: Some("  kept for later ".into()),
            ..Properties::default()
        };
        t.daemon.set_properties(id, changes).unwrap();
        let e = t.daemon.entry(id).unwrap();
        let moved = t.dir.join("sub").join("renamed.bin");
        assert_eq!(e.path.map(PathBuf::from), Some(moved.clone()));
        assert_eq!(std::fs::read(&moved).unwrap(), b"old");
        assert!(!t.dir.join("f.bin").exists());
        assert_eq!(e.description.as_deref(), Some("kept for later"));

        // Never over another file.
        std::fs::write(t.dir.join("taken.bin"), b"x").unwrap();
        let onto = Properties {
            filename: Some("taken.bin".into()),
            folder: Some(t.dir.display().to_string()),
            ..Properties::default()
        };
        assert!(t.daemon.set_properties(id, onto).is_err());
        assert!(moved.exists());
    }

    #[test]
    fn properties_change_what_a_retry_sends() {
        let t = setup("props_retry", Duplicate::NewName);
        let id = t.failed("https://x/old.bin");
        let changes = Properties {
            url: Some("https://y/new.bin".into()),
            referrer: Some("https://y/page".into()),
            login: Some(DownloadLogin { username: "me".into(), password: Some("pw".into()) }),
            ..Properties::default()
        };
        t.daemon.set_properties(id, changes).unwrap();
        let e = t.daemon.entry(id).unwrap();
        assert_eq!(e.url, "https://y/new.bin");
        assert_eq!(e.referrer.as_deref(), Some("https://y/page"));
        assert_eq!(e.login_user.as_deref(), Some("me"));
        let req = t.daemon.hub.requests.lock().unwrap().get(&id).cloned().unwrap();
        assert_eq!(req.url, "https://y/new.bin");
        assert_eq!(req.referrer.as_deref(), Some("https://y/page"));

        // A new user name without a password keeps the old password.
        let rename = Properties {
            login: Some(DownloadLogin { username: "other".into(), password: None }),
            ..Properties::default()
        };
        t.daemon.set_properties(id, rename).unwrap();
        let req = t.daemon.hub.requests.lock().unwrap().get(&id).cloned().unwrap();
        let login = req.login.clone().unwrap();
        assert_eq!((login.username.as_str(), login.password.as_deref()), ("other", Some("pw")));
        let net = t.daemon.net_for(&req, &Settings::default());
        let found = net::find_login(&net.logins, "https://y/new.bin").unwrap();
        assert_eq!(found.username, "other");

        // An empty user name removes the login.
        let clear = Properties { login: Some(DownloadLogin::default()), ..Properties::default() };
        t.daemon.set_properties(id, clear).unwrap();
        assert_eq!(t.daemon.entry(id).unwrap().login_user, None);
        assert!(t.daemon.hub.requests.lock().unwrap()[&id].login.is_none());
    }

    /// A stopped download moved to another folder takes its partial file
    /// and sidecar along, and resumes and finishes there without fetching
    /// its bytes again.
    #[test]
    fn properties_move_a_stopped_download_with_its_partial_file() {
        const LEN: usize = 1024 * 1024;
        let t = setup("props_move_partial", Duplicate::NewName);
        t.daemon.settings.lock().unwrap().connections = 2;
        let body: Arc<Vec<u8>> = Arc::new((0..LEN).map(|i| (i % 251) as u8).collect());
        let sent = Arc::new(AtomicU64::new(0));
        let id = t.add(&serve_ranges(Arc::clone(&body), Arc::clone(&sent)));
        let until = Instant::now() + Duration::from_secs(30);
        while t.daemon.status(id) != Some(Status::Downloading) || sent.load(Ordering::Relaxed) < LEN as u64 / 4 {
            assert!(Instant::now() < until, "the download never got going");
            thread::sleep(Duration::from_millis(20));
        }
        // Running, it cannot move.
        let elsewhere = t.dir.join("elsewhere");
        let move_it = || Properties { folder: Some(elsewhere.display().to_string()), ..Properties::default() };
        assert!(t.daemon.set_properties(id, move_it()).is_err());
        t.daemon.cancel(id);
        let stopped = t.wait(id, Status::Canceled);
        let old = PathBuf::from(stopped.path.unwrap());
        assert_eq!(old, t.dir.join("big.bin"));
        assert!(old.is_file() && engine::sidecar_path(&old).is_file());
        let kept = std::fs::metadata(&old).unwrap().len();

        t.daemon.set_properties(id, move_it()).unwrap();
        let moved = elsewhere.join("big.bin");
        assert_eq!(t.daemon.entry(id).unwrap().path.map(PathBuf::from), Some(moved.clone()));
        assert!(!old.exists() && !engine::sidecar_path(&old).exists(), "nothing is left behind");
        assert!(moved.is_file() && engine::sidecar_path(&moved).is_file());
        assert_eq!(std::fs::metadata(&moved).unwrap().len(), kept);

        let before = sent.load(Ordering::Relaxed);
        let (resumed, _) = t.daemon.retry(id).unwrap();
        let done = t.wait(resumed, Status::Done);
        assert_eq!(done.path.map(PathBuf::from), Some(moved.clone()));
        assert_eq!(std::fs::read(&moved).unwrap(), *body);
        assert!(!old.exists(), "it did not start over in the old folder");
        let again = sent.load(Ordering::Relaxed) - before;
        assert!(again < LEN as u64 - LEN as u64 / 8, "sent {again} bytes again; it did not resume");
    }

    #[test]
    fn properties_move_a_download_that_never_started() {
        let t = setup("props_move_unstarted", Duplicate::NewName);
        let id = t.failed("https://x/a.bin");
        let elsewhere = t.dir.join("later");
        let changes = Properties { folder: Some(elsewhere.display().to_string()), ..Properties::default() };
        t.daemon.set_properties(id, changes).unwrap();
        let req = t.daemon.hub.requests.lock().unwrap().get(&id).cloned().unwrap();
        assert_eq!(req.folder.map(PathBuf::from), Some(elsewhere.clone()), "the next attempt saves there");

        // A proposed file that was never started may be somebody else's: it
        // stays where it is.
        let other = t.dir.join("theirs.bin");
        std::fs::write(&other, b"theirs").unwrap();
        t.daemon
            .hub
            .update(id, |e| {
                e.path = Some(other.display().to_string());
                Ok(())
            })
            .unwrap();
        let back = Properties { folder: Some(t.dir.join("again").display().to_string()), ..Properties::default() };
        t.daemon.set_properties(id, back).unwrap();
        assert_eq!(std::fs::read(&other).unwrap(), b"theirs");
        assert_eq!(t.daemon.entry(id).unwrap().path.map(PathBuf::from), Some(t.dir.join("again").join("theirs.bin")));
        let relative = Properties { folder: Some("rel".into()), ..Properties::default() };
        assert!(t.daemon.set_properties(id, relative).is_err());
        let rename = Properties { filename: Some("b.bin".into()), ..Properties::default() };
        assert!(t.daemon.set_properties(id, rename).is_err(), "only a finished file is renamed");
    }

    #[test]
    fn the_link_picker_hears_each_links_type_and_size() {
        let t = setup("probe_links", Duplicate::NewName);
        let link = |url: String| Download { url, ..Download::default() };
        let links = vec![link(serve()), link("http://127.0.0.1:1/gone.zip".into()), link(serve())];
        let (tx, rx) = channel();
        t.daemon.probe_links(7, 42, links, tx);
        let mut heard = std::collections::BTreeMap::new();
        while heard.len() < 3 {
            let frame = rx.recv_timeout(Duration::from_secs(10)).expect("every link is answered");
            if let Ok(Outgoing::LinkInfo { request: 42, index, total, content_type, error }) =
                serde_json::from_slice(&frame)
            {
                heard.insert(index, (total, content_type, error));
            }
        }
        let found = (Some(BODY.len() as u64), Some("application/octet-stream".to_string()), None);
        assert_eq!(heard[&0], found);
        assert_eq!(heard[&2], found);
        assert!(heard[&1].2.is_some(), "a link that cannot be reached says why");

        // A new request stops the last; closing the picker stops the new one.
        let (tx, _rx) = channel();
        let before = Arc::clone(&t.daemon.link_probes.lock().unwrap()[&7]);
        t.daemon.probe_links(7, 43, vec![link(serve())], tx);
        assert!(before.load(Ordering::Relaxed));
        let now = Arc::clone(&t.daemon.link_probes.lock().unwrap()[&7]);
        t.daemon.stop_probe_links(7);
        assert!(now.load(Ordering::Relaxed) && t.daemon.link_probes.lock().unwrap().is_empty());
    }

    #[test]
    fn the_browsers_proxy_can_wait_for_a_failure() {
        let t = setup("browser_proxy", Duplicate::NewName);
        let d = Download { url: "https://x/a.zip".into(), proxy: Some("http://bp:3128".into()), ..Download::default() };
        let through = |s: &Settings| t.daemon.net_for(&d, s).proxy.and_then(|p| p.proxy_for(&d.url));
        let after = Settings { use_browser_proxy: true, browser_proxy_after_errors: true, ..Settings::default() };
        assert_eq!(through(&after), None, "not before it failed");
        let always = Settings { browser_proxy_after_errors: false, ..after.clone() };
        assert_eq!(through(&always).as_deref(), Some("http://bp:3128"));
        t.daemon.browser_proxy_retries.lock().unwrap().insert(d.url.clone());
        assert_eq!(through(&after).as_deref(), Some("http://bp:3128"), "once it failed");
        // A download that never failed is not retried through it.
        assert!(!t.daemon.retry_through_browser_proxy(999, &after));
    }

    #[test]
    fn the_browsers_whole_proxy_setting_is_used() {
        use uidm::protocol::{BrowserProxy, BrowserProxyMode};
        let t = setup("browser_proxy_setting", Duplicate::NewName);
        let s = Settings {
            use_browser_proxy: true,
            proxy_mode: ProxyMode::Manual,
            proxy_http: "uidm:8080".into(),
            ..Settings::default()
        };
        let through = |setting: Option<BrowserProxy>, s: &Settings| {
            let d = Download {
                url: "http://x.net/a.zip".into(),
                proxy: Some("http://old:1".into()),
                browser_proxy: setting,
                ..Download::default()
            };
            t.daemon.net_for(&d, s).proxy.and_then(|p| p.proxy_for(&d.url))
        };
        // The browser connects directly: so does the download, whatever
        // UIDM's own setting and the older field say.
        let direct = BrowserProxy::default();
        assert_eq!(through(Some(direct.clone()), &s), None);
        let fixed = BrowserProxy {
            mode: BrowserProxyMode::Fixed,
            http: Some("http://bp:3128".into()),
            bypass: vec!["x.net".into()],
            ..BrowserProxy::default()
        };
        assert_eq!(through(Some(fixed.clone()), &s), None, "the browser's bypass list");
        let fixed = BrowserProxy { bypass: vec![], ..fixed };
        assert_eq!(through(Some(fixed), &s).as_deref(), Some("http://bp:3128"));
        // Older extensions send only the fixed proxy.
        assert_eq!(through(None, &s).as_deref(), Some("http://old:1"));
        // Without the option, UIDM's own setting applies.
        let own = Settings { use_browser_proxy: false, ..s.clone() };
        assert_eq!(through(Some(direct), &own).as_deref(), Some("http://uidm:8080"));
    }

    #[test]
    fn a_browser_download_shows_the_size_the_browser_saw() {
        let t = setup("browser_size", Duplicate::NewName);
        let (id, _) = t.daemon.add_later(Download {
            url: "https://x/big.iso".into(),
            size: Some(4_000_000_000),
            content_type: Some("application/x-iso9660-image".into()),
            ..Download::default()
        });
        assert_eq!(t.daemon.entry(id).unwrap().total, Some(4_000_000_000));
    }

    #[test]
    fn the_browser_changes_only_the_video_button() {
        let t = setup("browser_settings", Duplicate::NewName);
        let asked = Settings {
            video_button: false,
            video_button_excluded_sites: vec!["v.net".into()],
            video_auto_capture: true,
            browser_menu_hidden_firefox: vec!["download".into()],
            capture_force_key: settings::CaptureKey { delete: true, ..settings::CaptureKey::NONE },
            ..Settings::default()
        };
        let kept = t.daemon.with_video_button(&asked);
        assert!(!kept.video_button);
        assert_eq!(kept.video_button_excluded_sites, ["v.net"]);
        assert!(!kept.video_auto_capture, "web players' files are the window's to turn on");
        assert!(kept.browser_menu_hidden_firefox.is_empty());
        assert_eq!(kept.capture_force_key, settings::CaptureKey::CTRL);
    }

    #[test]
    fn only_the_window_turns_on_capture_in_private_windows() {
        let t = setup("browser_private", Duplicate::NewName);
        assert!(!t.daemon.settings.lock().unwrap().capture_private, "off by default");
        let asked = Settings { capture_private: true, ..t.daemon.settings.lock().unwrap().clone() };
        // A browser client's set_settings keeps the setting as it was...
        t.daemon.set_settings(t.daemon.with_video_button(&asked)).unwrap();
        assert!(!t.daemon.settings.lock().unwrap().capture_private);
        // ...while the window's goes through.
        t.daemon.set_settings(asked).unwrap();
        assert!(t.daemon.settings.lock().unwrap().capture_private);
        let off = Settings { capture_private: false, ..Settings::default() };
        assert!(t.daemon.with_video_button(&off).capture_private, "nor can the browser turn it off");
    }

    #[test]
    fn manual_socks_can_carry_https_and_ftp() {
        let t = setup("socks_schemes", Duplicate::NewName);
        let s = Settings {
            proxy_mode: ProxyMode::Manual,
            proxy_http: "web:8080".into(),
            proxy_socks5: "sock:1080".into(),
            proxy_socks_https: true,
            ..Settings::default()
        };
        let net = |url: &str, s: &Settings| t.daemon.net_for(&Download { url: url.into(), ..Download::default() }, s);
        let via = |url: &str, s: &Settings| net(url, s).proxy.and_then(|p| p.proxy_for(url));
        assert_eq!(via("http://x.net/a.zip", &s).as_deref(), Some("http://web:8080"));
        assert_eq!(via("https://x.net/a.zip", &s).as_deref(), Some("socks5://sock:1080"));
        let off = Settings { proxy_socks_https: false, ..s.clone() };
        assert_eq!(
            via("https://x.net/a.zip", &off).as_deref(),
            Some("http://web:8080"),
            "an empty HTTPS proxy is the HTTP one"
        );

        // ftp:// takes the SOCKS proxy (on by default) unless an FTP proxy
        // is set, or it is turned off.
        let ftp = |s: &Settings| net("ftp://x.net/a.zip", s).proxy.and_then(|p| p.socks_for_ftp("ftp://x.net/a.zip"));
        assert_eq!(ftp(&s).map(|p| p.addr).as_deref(), Some("sock:1080"));
        assert_eq!(ftp(&Settings { proxy_socks_ftp: false, ..s.clone() }), None);
        let with_ftp_proxy = Settings { proxy_ftp: "ftpproxy:21".into(), ..s.clone() };
        assert_eq!(net("ftp://x.net/a.zip", &with_ftp_proxy).ftp_proxy.as_deref(), Some("ftpproxy:21"));
    }

    #[test]
    fn properties_refuse_running_downloads_and_bad_links() {
        let t = setup("props_refuse", Duplicate::NewName);
        let running = t.daemon.engine.reserve_id();
        t.daemon.hub.publish(&Outgoing::Added { id: running, url: "https://x/a".into() });
        let describe = Properties { description: Some("x".into()), ..Properties::default() };
        assert!(t.daemon.set_properties(running, describe).is_err());
        let failed = t.failed("https://x/b");
        let bad = Properties { url: Some("file://x/b".into()), ..Properties::default() };
        assert!(t.daemon.set_properties(failed, bad).is_err());
        let rename = Properties { filename: Some("c".into()), ..Properties::default() };
        assert!(t.daemon.set_properties(failed, rename).is_err(), "only finished files move");
    }

    #[test]
    fn removing_with_files_recycles_only_what_the_download_wrote() {
        let t = setup("remove_files", Duplicate::NewName);
        let kept = t.dir.join("kept.bin");
        let id = t.finished("https://x/kept.bin", &kept);
        t.daemon.remove(id, false).unwrap();
        assert!(kept.exists(), "a plain remove keeps the file");

        let gone = t.dir.join("gone.bin");
        let id = t.finished("https://x/gone.bin", &gone);
        let sidecar = t.dir.join("gone.bin.uidmpart");
        std::fs::write(&sidecar, b"{}").unwrap();
        t.daemon.remove(id, true).unwrap();
        assert!(!gone.exists() && !sidecar.exists());
        assert!(t.daemon.entry(id).is_none());

        // A download that never started only proposed its path.
        let other = t.dir.join("someone_elses.bin");
        std::fs::write(&other, b"theirs").unwrap();
        let id = t.failed("https://x/someone_elses.bin");
        t.daemon
            .hub
            .update(id, |e| {
                e.path = Some(other.display().to_string());
                Ok(())
            })
            .unwrap();
        t.daemon.remove(id, true).unwrap();
        assert!(other.exists());
    }

    #[test]
    fn downloading_again_keeps_the_row_and_follows_the_file_setting() {
        let t = setup("again_rename", Duplicate::NewName);
        let url = serve();
        let path = t.dir.join("kept").join("f.bin");
        let id = t.finished(&url, &path);
        t.daemon
            .set_properties(id, Properties { description: Some("monthly".into()), ..Properties::default() })
            .unwrap();
        t.daemon.redownload(id).unwrap();
        let done = t.wait(id, Status::Done);
        assert_eq!(done.description.as_deref(), Some("monthly"));
        assert_eq!(done.path.map(PathBuf::from), Some(t.dir.join("kept").join("f (1).bin")));
        assert_eq!(std::fs::read(&path).unwrap(), b"old", "the old file is a taken name");
        assert_eq!(t.len(), 1, "still one row");

        let t = setup("again_overwrite", Duplicate::NewName);
        t.daemon.settings.lock().unwrap().file_exists = FileExists::Overwrite;
        let path = t.dir.join("f.bin");
        let id = t.finished(&url, &path);
        t.daemon.redownload(id).unwrap();
        let done = t.wait(id, Status::Done);
        assert_eq!(done.path.map(PathBuf::from), Some(path.clone()));
        assert_eq!(std::fs::read(&path).unwrap(), BODY);
    }

    #[test]
    fn only_stopped_downloads_go_again() {
        let t = setup("again_refuse", Duplicate::NewName);
        let id = t.daemon.engine.reserve_id();
        t.daemon.hub.publish(&Outgoing::Added { id, url: "https://x/a".into() });
        assert!(t.daemon.redownload(id).is_err());
        assert!(t.daemon.redownload(999).is_err());
    }

    #[test]
    fn a_download_limit_is_kept_through_a_retry() {
        let t = setup("own_limit", Duplicate::NewName);
        let id = t.failed("https://x/slow.bin");
        t.daemon.set_download_limit(id, Some(64 * 1024)).unwrap();
        assert_eq!(t.daemon.entry(id).unwrap().speed_limit, Some(64 * 1024));
        assert!(t.daemon.set_download_limit(id, Some(10)).is_err(), "below the smallest limit");
        let (new_id, _) = t.daemon.retry(id).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while t.daemon.entry(new_id).and_then(|e| e.speed_limit).is_none() {
            assert!(Instant::now() < until, "the retry lost its limit");
            thread::sleep(Duration::from_millis(20));
        }
        assert!(t.daemon.entry(new_id).unwrap().limit_remember, "remembering is on by default");
        t.daemon.cancel(new_id);
        t.daemon.set_download_limit(new_id, None).unwrap();
        assert_eq!(t.daemon.entry(new_id).unwrap().speed_limit, None);
    }

    #[test]
    fn a_limit_not_remembered_is_dropped_on_stop_and_resume() {
        let t = setup("forget_limit", Duplicate::NewName);
        let id = t.failed("https://x/slow.bin");
        t.daemon.set_download_limit(id, Some(64 * 1024)).unwrap();
        t.daemon.set_limit_remember(id, false).unwrap();
        let (new_id, _) = t.daemon.retry(id).unwrap();
        // Give the new entry time to take what its request carries.
        thread::sleep(Duration::from_millis(200));
        let e = t.daemon.entry(new_id).unwrap();
        assert_eq!(e.speed_limit, None, "the limit went with the stop");
        assert!(!e.limit_remember, "the choice stays with the download");
        t.daemon.cancel(new_id);

        // Stopping its queue and starting it again is a stop and resume too.
        let id = t.daemon.engine.reserve_id();
        t.daemon.hub.publish(&Outgoing::Added { id, url: "https://x/q.bin".into() });
        t.daemon.set_download_limit(id, Some(64 * 1024)).unwrap();
        t.daemon.set_limit_remember(id, false).unwrap();
        t.daemon.hub.held.lock().unwrap().insert(id);
        t.daemon.hub.publish(&Outgoing::Canceled { id, url: "https://x/q.bin".into() });
        let e = t.daemon.entry(id).unwrap();
        assert_eq!(e.status, Status::Waiting);
        assert_eq!(e.speed_limit, None);

        // Remembered, it stays.
        let id = t.daemon.engine.reserve_id();
        t.daemon.hub.publish(&Outgoing::Added { id, url: "https://x/r.bin".into() });
        t.daemon.set_download_limit(id, Some(64 * 1024)).unwrap();
        t.daemon.hub.held.lock().unwrap().insert(id);
        t.daemon.hub.publish(&Outgoing::Canceled { id, url: "https://x/r.bin".into() });
        assert_eq!(t.daemon.entry(id).unwrap().speed_limit, Some(64 * 1024));
    }

    #[test]
    fn the_speed_limit_can_turn_on_at_every_start() {
        let dir = std::env::temp_dir().join(format!("uidm_limit_startup_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let start = |on_startup: bool| {
            let s = Settings {
                speed_limit_enabled: false,
                speed_limit_kbps: 300,
                speed_limit_on_startup: on_startup,
                speed_limit_per_file_kbps: Some(100),
                ..Settings::default()
            };
            s.save(&path).unwrap();
            let (daemon, ..) =
                Daemon::new(Paths { settings: Some(path.clone()), history: None, requests: None, quota: None });
            let s = daemon.settings.lock().unwrap().clone();
            (s, daemon.engine.per_file_limit())
        };
        let (s, per_file) = start(false);
        assert!(!s.speed_limit_enabled, "off as it was left");
        assert_eq!(per_file, None, "the limit per file is off with it");
        let (s, per_file) = start(true);
        assert!(s.speed_limit_enabled, "on, though it was left off");
        assert_eq!(s.speed_limit_bytes(), Some(300 * 1024));
        assert_eq!(per_file, Some(100 * 1024));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_limit_per_file_caps_each_download_without_its_own() {
        let t = setup("per_file", Duplicate::NewName);
        let mut s = t.daemon.settings.lock().unwrap().clone();
        s.speed_limit_enabled = true;
        s.speed_limit_kbps = 100_000;
        s.speed_limit_per_file_kbps = Some(32);
        t.daemon.set_settings(s.clone()).unwrap();
        assert_eq!(t.daemon.engine.per_file_limit(), Some(32 * 1024));

        // 48 KiB at 32 KiB/s takes about 1.5s; unlimited, a fraction of that.
        let started = Instant::now();
        let id = t.add(&serve_ranges(Arc::new(vec![3u8; 48 * 1024]), Arc::new(AtomicU64::new(0))));
        t.wait(id, Status::Done);
        let secs = started.elapsed().as_secs_f64();
        assert!(secs >= 1.0, "took {secs:.2}s; the limit per file did not hold");

        // It only applies while the speed limit is on.
        s.speed_limit_enabled = false;
        t.daemon.set_settings(s).unwrap();
        assert_eq!(t.daemon.engine.per_file_limit(), None);
    }

    #[test]
    fn a_download_does_what_it_asked_once_finished() {
        let t = setup("on_done", Duplicate::NewName);
        let id = t.failed("https://x/a.bin");
        let asked = OnDone { dialog: true, then: WhenDone::Nothing, force_close: false };
        t.daemon.set_on_done(id, Some(asked)).unwrap();
        assert_eq!(t.daemon.entry(id).unwrap().on_done, Some(asked));
        t.daemon.after_download(id);
        assert_eq!(t.daemon.entry(id).unwrap().on_done, Some(asked), "a failed download keeps it for its retry");

        t.daemon
            .hub
            .update(id, |e| {
                e.status = Status::Done;
                Ok(())
            })
            .unwrap();
        assert!(t.daemon.set_on_done(id, None).is_err(), "too late once finished");
        t.daemon.after_download(id);
        assert_eq!(t.daemon.entry(id).unwrap().on_done, None, "it runs once");
    }

    #[test]
    fn choosing_nothing_clears_the_choice() {
        let t = setup("on_done_clear", Duplicate::NewName);
        let id = t.failed("https://x/a.bin");
        t.daemon.set_on_done(id, Some(OnDone { dialog: false, then: WhenDone::ShutDown, force_close: true })).unwrap();
        t.daemon.set_on_done(id, Some(OnDone::default())).unwrap();
        assert_eq!(t.daemon.entry(id).unwrap().on_done, None);
    }

    #[test]
    fn the_save_as_folder_is_remembered_for_its_category() {
        let t = setup("remember_folder", Duplicate::NewName);
        t.daemon.settings.lock().unwrap().categories[0].remember_folder = true;
        let (id, _) = t.daemon.add_later(Download { url: "https://x/a.zip".into(), ..Download::default() });
        let chosen = t.dir.join("chosen");
        t.daemon
            .confirm(Answer {
                filename: Some("a.zip".into()),
                folder: Some(chosen.display().to_string()),
                start: false,
                ..Answer::start(id)
            })
            .unwrap();
        let s = t.daemon.settings.lock().unwrap().clone();
        assert_eq!(s.categories[0].last_folder.as_deref(), Some(chosen.display().to_string().as_str()));
        assert_eq!(s.folders().for_name("b.zip"), chosen);
    }

    #[test]
    fn a_download_keeps_its_referrer_and_page_apart() {
        let t = setup("page", Duplicate::NewName);
        let (id, _) = t.daemon.add_later(Download {
            url: "https://cdn.x/a.zip".into(),
            referrer: Some("https://x/frame".into()),
            page: Some("https://x/article".into()),
            ..Download::default()
        });
        let e = t.daemon.entry(id).unwrap();
        assert_eq!(e.referrer.as_deref(), Some("https://x/frame"));
        assert_eq!(e.page.as_deref(), Some("https://x/article"));
        // Only the referrer goes to the server.
        let req = t.daemon.hub.requests.lock().unwrap().get(&id).cloned().unwrap();
        let headers = t.daemon.request_for(req).headers;
        let sent: Vec<&str> = headers.iter().filter(|(k, _)| k == "Referer").map(|(_, v)| v.as_str()).collect();
        assert_eq!(sent, ["https://x/frame"]);
        assert!(!headers.iter().any(|(_, v)| v.contains("article")));

        // A source that knows only the referrer found the link on it.
        let (id, _) = t.daemon.add_later(Download {
            url: "https://x/b.zip".into(),
            referrer: Some("https://x/list".into()),
            ..Download::default()
        });
        assert_eq!(t.daemon.entry(id).unwrap().page.as_deref(), Some("https://x/list"));
    }

    #[test]
    fn the_save_as_dialog_keeps_a_category_and_description() {
        let t = setup("category", Duplicate::NewName);
        t.daemon.settings.lock().unwrap().categories[2].remember_folder = true;
        let (id, _) = t.daemon.add_later(Download { url: "https://x/a.zip".into(), ..Download::default() });
        let chosen = t.dir.join("tunes");
        t.daemon
            .confirm(Answer {
                filename: Some("a.zip".into()),
                folder: Some(chosen.display().to_string()),
                start: false,
                category: Some(" Music ".into()),
                description: Some(" the album ".into()),
                ..Answer::start(id)
            })
            .unwrap();
        let e = t.daemon.entry(id).unwrap();
        assert_eq!(e.category.as_deref(), Some("Music"));
        assert_eq!(e.description.as_deref(), Some("the album"));
        // The chosen category, not the file's type, remembers the folder.
        let s = t.daemon.settings.lock().unwrap().clone();
        assert_eq!(s.categories[2].last_folder.as_deref(), Some(chosen.display().to_string().as_str()));
        assert_eq!(s.categories[0].last_folder, None);
        // A retry keeps both.
        let (again, _) = t.daemon.retry(id).unwrap();
        let e = t.daemon.entry(again).unwrap();
        assert_eq!(e.category.as_deref(), Some("Music"));
        assert_eq!(e.description.as_deref(), Some("the album"));
    }

    #[test]
    fn saving_a_queue_keeps_force_close() {
        let t = setup("queue_force_close", Duplicate::NewName);
        let id = t
            .daemon
            .save_queue(Queue { when_done: WhenDone::ShutDown, force_close: true, ..Queue::new(0, "Night") })
            .unwrap();
        let saved = |t: &Setup| t.daemon.hub.queues.lock().unwrap().iter().find(|q| q.id == id).cloned().unwrap();
        assert!(saved(&t).force_close, "a new queue keeps it");

        let mut edited = saved(&t);
        edited.force_close = false;
        t.daemon.save_queue(edited.clone()).unwrap();
        assert!(!saved(&t).force_close, "turning it off is kept");
        edited.force_close = true;
        t.daemon.save_queue(edited).unwrap();
        assert!(saved(&t).force_close, "turning it on is kept");

        // It survives a restart: the saved form reads back the same.
        let json = serde_json::to_string(&saved(&t)).unwrap();
        let back: Queue = serde_json::from_str(&json).unwrap();
        assert!(back.force_close);
    }

    #[test]
    fn the_save_as_dialog_remembers_its_choices() {
        let t = setup("remember_choices", Duplicate::Ask);
        let queue = t.daemon.save_queue(Queue::new(0, "Night")).unwrap();

        // "Remember this path" for General, and "Don't ask me again" for the
        // queue of a download added for later.
        let (id, _) = t.daemon.add_later(Download { url: "https://x/notes.xyz".into(), ..Download::default() });
        let chosen = t.dir.join("misc");
        t.daemon
            .confirm(Answer {
                folder: Some(chosen.display().to_string()),
                filename: Some("notes.xyz".into()),
                start: false,
                queue: Some(queue),
                remember_folder: Some(true),
                remember_queue: true,
                ..Answer::start(id)
            })
            .unwrap();
        let s = t.daemon.settings.lock().unwrap().clone();
        assert!(s.general_remember_folder);
        assert_eq!(s.folders().for_name("other.xyz"), chosen);
        assert_eq!(s.later_queue, Some(queue));
        // Download later now goes to that queue without being told.
        let (later, _) = t.daemon.add_later(Download { url: "https://x/more.xyz".into(), ..Download::default() });
        assert_eq!(t.daemon.hub.queue_of(later), Some(queue));

        // "Remember my selection" for an address already in the list.
        let url = serve();
        let old = t.finished(&url, &t.dir.join("twice.bin"));
        let again = t.add(&url);
        assert_eq!(t.wait(again, Status::Confirming).duplicate_of, Some(old));
        t.daemon
            .confirm(Answer { duplicate: Some(Duplicate::NewName), remember_duplicate: true, ..Answer::start(again) })
            .unwrap();
        assert_eq!(t.daemon.settings.lock().unwrap().if_duplicate, Duplicate::NewName);
    }

    #[test]
    fn the_quota_pauses_downloads_and_lets_them_go_again() {
        let t = setup("quota", Duplicate::NewName);
        {
            let mut s = t.daemon.settings.lock().unwrap();
            s.quota_enabled = true;
            s.quota_mb = 1;
            s.quota_hours = 1;
        }
        let url = serve();
        let id = t.failed(&url);
        // 2 MB came in this minute.
        {
            let mut q = t.daemon.hub.quota.lock().unwrap();
            q.started(99, 0);
            q.progress(99, 2 << 20, uidm::quota::now_minute());
        }
        t.daemon.check_quota();
        assert!(t.window_got(|m| matches!(m, Outgoing::Quota { paused: true, resumes_at: Some(_), .. })));
        assert!(t.daemon.quota_paused.lock().unwrap().is_some());
        // A download started while paused is stopped on the next check.
        t.daemon.retry(id).unwrap();
        t.daemon.check_quota();
        let held = t.daemon.quota_paused.lock().unwrap().clone().unwrap();
        assert!(!held.1.is_empty(), "the new download is held too");

        *t.daemon.hub.quota.lock().unwrap() = Usage::default();
        t.daemon.check_quota();
        assert!(t.daemon.quota_paused.lock().unwrap().is_none());
        assert!(t.window_got(|m| matches!(m, Outgoing::Quota { paused: false, .. })));
    }

    #[test]
    fn a_crawl_asks_the_browser_for_its_cookies() {
        let t = setup("crawl_cookies", Duplicate::NewName);
        // The browser's extension is connected; without it nobody is asked.
        let (extension, _from_daemon) = channel();
        t.daemon.hub.clients.lock().unwrap().insert(2, extension);
        let d = Arc::clone(&t.daemon);
        let asking = thread::spawn(move || d.browser_cookies("https://site.net/"));
        let request = loop {
            let frame = t.window.recv_timeout(Duration::from_secs(5)).expect("no cookies_wanted");
            if let Ok(Outgoing::CookiesWanted { request, url }) = serde_json::from_slice(&frame) {
                assert_eq!(url, "https://site.net/");
                break request;
            }
        };
        let tx = t.daemon.cookie_waits.lock().unwrap().remove(&request).unwrap();
        tx.send(Some("session=abc".into())).unwrap();
        assert_eq!(asking.join().unwrap().as_deref(), Some("session=abc"));
    }

    #[test]
    fn a_scheduled_project_adds_only_new_finds() {
        let t = setup("grab_schedule", Duplicate::NewName);
        // A one-page site linking to one PDF.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                let (status, body) = if head.starts_with("GET / ") {
                    ("200 OK", "<a href=\"/report.pdf\">r</a>")
                } else {
                    ("404 Not Found", "")
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        let request =
            GrabRequest { start: format!("http://{addr}/"), depth: 0, delay_ms: 200, ..GrabRequest::default() };
        let project: grab::Project = serde_json::from_value(serde_json::json!({
            "name": "nightly",
            "request": request,
            "run": "explore_and_download",
        }))
        .unwrap();
        let dir = t.dir.join("projects");
        t.daemon.run_scheduled_grab(&dir, project, 1);
        let saved = grab::Project::load(&dir, "nightly").unwrap();
        assert_eq!(saved.seen, [format!("http://{addr}/report.pdf")]);
        assert_eq!(t.len(), 1, "the new find was added");
        assert!(
            t.daemon.hub.queues.lock().unwrap().iter().any(|q| q.id == MAIN_QUEUE && q.max_concurrent == 3),
            "the grabber's files at once"
        );
        t.daemon.run_scheduled_grab(&dir, saved.clone(), 2);
        assert_eq!(t.len(), 1, "nothing new the second time");

        // Downloading the ticked files reads no page.
        let checked = grab::Project {
            name: "ticked".into(),
            run: grab::RunAction::DownloadChecked,
            request: GrabRequest { start: "http://127.0.0.1:1/".into(), ..request },
            files: vec![
                grab::GrabFile {
                    url: format!("http://{addr}/a.pdf"),
                    name: "a.pdf".into(),
                    page: String::new(),
                    size: None,
                    text: None,
                },
                grab::GrabFile {
                    url: format!("http://{addr}/b.pdf"),
                    name: "b.pdf".into(),
                    page: String::new(),
                    size: None,
                    text: None,
                },
            ],
            checked: vec![true, false],
            ..saved
        };
        t.daemon.run_scheduled_grab(&dir, checked, 3);
        assert_eq!(t.len(), 2, "only the ticked file was added");
    }

    #[test]
    fn crawled_pages_are_saved_with_local_links() {
        let dir = std::env::temp_dir().join(format!("uidm_save_page_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mirror = grab::Mirror { folder: dir.display().to_string(), pages: true, overwrite: false };
        let req = GrabRequest { start: "https://s/".into(), mirror: Some(mirror.clone()), ..GrabRequest::default() };
        let page = grab::Page {
            url: "https://s/docs/".into(),
            content_type: Some("text/html".into()),
            body: br#"<a href="guide.pdf">g</a>"#.to_vec(),
        };
        save_page(&req, &mirror, &page);
        let saved = dir.join("s").join("docs").join("index.html");
        assert_eq!(std::fs::read_to_string(&saved).unwrap(), r#"<a href="guide.pdf">g</a>"#);
        std::fs::write(&saved, "mine").unwrap();
        save_page(&req, &mirror, &page);
        assert_eq!(std::fs::read_to_string(&saved).unwrap(), "mine", "skipped, not replaced");
        let robots = grab::Page {
            url: "https://s/robots.txt".into(),
            content_type: Some("text/plain".into()),
            body: b"User-agent: *".to_vec(),
        };
        save_page(&req, &mirror, &robots);
        assert!(!dir.join("s").join("robots.txt.html").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_crawl_can_have_the_browser_render_a_page() {
        let t = setup("render_page", Duplicate::NewName);
        assert!(!t.daemon.hub.extension_connected(), "only a window is connected");
        let d = Arc::clone(&t.daemon);
        let asking = thread::spawn(move || d.render_in_browser("https://site.net/app"));
        let request = loop {
            let frame = t.window.recv_timeout(Duration::from_secs(5)).expect("no render_page");
            if let Ok(Outgoing::RenderPage { request, .. }) = serde_json::from_slice(&frame) {
                break request;
            }
        };
        let tx = t.daemon.render_waits.lock().unwrap().remove(&request).unwrap();
        tx.send(Some(("<a href=x.pdf>".into(), "https://site.net/app/".into(), Vec::new()))).unwrap();
        let (html, url, _) = asking.join().unwrap().unwrap();
        assert_eq!((html.as_str(), url.as_str()), ("<a href=x.pdf>", "https://site.net/app/"));
    }

    #[test]
    fn the_update_check_reports_only_newer_releases() {
        let t = setup("update_check", Duplicate::NewName);
        let serve_tag = |tag: &'static str| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            thread::spawn(move || {
                for mut s in listener.incoming().flatten() {
                    let mut buf = [0u8; 2048];
                    let _ = s.read(&mut buf);
                    let body = format!(
                        r#"{{"tag_name":"{tag}","html_url":"https://github.com/tajbinkhan/uidm/releases/tag/{tag}"}}"#
                    );
                    let _ = write!(
                        s,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            });
            format!("http://{addr}/releases/latest")
        };
        match t.daemon.check_update(&serve_tag("v99.0.0")) {
            Outgoing::Update { newer: Some(r), error: None } => assert_eq!(r.version, "99.0.0"),
            other => panic!("expected a newer release: {other:?}"),
        }
        assert!(matches!(t.daemon.check_update(&serve_tag("v0.0.1")), Outgoing::Update { newer: None, error: None }));
        assert!(matches!(t.daemon.check_update("http://127.0.0.1:1/none"), Outgoing::Update { error: Some(_), .. }));
        assert!(t.daemon.settings.lock().unwrap().last_update_check > 0, "the check was noted");
    }

    #[test]
    fn a_dropped_link_always_gets_the_save_as_dialog() {
        let t = setup("ask_flag", Duplicate::NewName);
        assert!(!t.daemon.settings.lock().unwrap().ask_where_to_save);
        let (id, _) = t.daemon.add(Download { url: serve(), ask: Some(true), ..Download::default() });
        t.wait(id, Status::Confirming);
    }

    #[test]
    fn some_types_start_without_the_dialog_and_add_only_waits() {
        let t = setup("auto_start", Duplicate::NewName);
        {
            let mut s = t.daemon.settings.lock().unwrap();
            s.ask_where_to_save = true;
            s.auto_start_types = vec!["bin".into()];
        }
        let id = t.add(&serve());
        assert_eq!(t.wait(id, Status::Done).path.map(PathBuf::from), Some(t.dir.join("f.bin")));

        t.daemon.settings.lock().unwrap().add_only = true;
        let id = t.add(&serve());
        assert_eq!(t.daemon.status(id), Some(Status::Waiting));
        assert_eq!(t.daemon.hub.queue_of(id), Some(MAIN_QUEUE));
    }

    #[test]
    fn a_download_removed_from_its_queue_stays_listed_and_waits_for_a_hand_start() {
        let t = setup("remove_from_queue", Duplicate::NewName);
        let (id, _) = t.daemon.add_later(Download { url: serve(), ..Download::default() });
        let (kept, _) = t.daemon.add_later(Download { url: format!("{}#kept", serve()), ..Download::default() });
        assert_eq!(t.daemon.hub.queue_of(id), Some(MAIN_QUEUE));

        t.daemon.remove_from_queue(id).unwrap();
        assert_eq!(t.daemon.hub.queue_of(id), None, "it left the queue");
        assert_eq!(t.daemon.status(id), Some(Status::Waiting), "and still waits in the list");
        assert!(t.daemon.remove_from_queue(id).is_err(), "it is in no queue now");

        // Starting the queue runs what is left in it, not the removed one.
        t.daemon.start_queue(MAIN_QUEUE, None).unwrap();
        t.daemon.tick();
        t.wait(kept, Status::Done);
        t.daemon.tick();
        assert_eq!(t.daemon.status(id), Some(Status::Waiting), "no queue started it");

        // Move to queue puts it back.
        t.daemon.move_to_queue(id, MAIN_QUEUE, None).unwrap();
        assert_eq!(t.daemon.hub.queue_of(id), Some(MAIN_QUEUE));
        t.daemon.remove_from_queue(id).unwrap();

        // Starting it by hand still works.
        t.daemon.confirm(Answer::start(id)).unwrap();
        t.wait(id, Status::Done);
    }

    /// A server whose one file is `body` with ETag `etag`, both changeable
    /// meanwhile; with no ETag set it answers 500.
    fn serve_versions(file: Arc<Mutex<(Vec<u8>, Option<String>)>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let file = Arc::clone(&file);
                thread::spawn(move || {
                    let mut req = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buf) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => req.extend_from_slice(&buf[..n]),
                        }
                    }
                    let (body, etag) = file.lock().unwrap().clone();
                    let Some(etag) = etag else {
                        let _ =
                            stream.write_all(b"HTTP/1.1 500 Oops\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        return;
                    };
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: {etag}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    if !req.starts_with(b"HEAD") {
                        let _ = stream.write_all(&body);
                    }
                });
            }
        });
        format!("http://{addr}/mirror.bin")
    }

    impl Setup {
        /// Start queue `q` and run the scheduler until the run ends.
        fn run_queue(&self, q: u64) {
            self.daemon.start_queue(q, None).unwrap();
            let until = Instant::now() + Duration::from_secs(10);
            while self.daemon.hub.queues.lock().unwrap().iter().any(|x| x.id == q && x.running) {
                assert!(Instant::now() < until, "the run never ended");
                self.daemon.tick();
                thread::sleep(Duration::from_millis(20));
            }
        }
    }

    #[test]
    fn a_synchronizing_queue_downloads_again_only_what_changed() {
        let t = setup("sync_queue", Duplicate::NewName);
        let file = Arc::new(Mutex::new((b"version 1".to_vec(), Some("\"v1\"".to_string()))));
        let url = serve_versions(Arc::clone(&file));
        let id = t.add(&url);
        let first = t.wait(id, Status::Done);
        let path = PathBuf::from(first.path.clone().unwrap());
        assert_eq!(first.validator.as_ref().and_then(|v| v.etag.as_deref()), Some("\"v1\""), "the ETag is kept");

        // A one-time queue takes only what is still to do.
        assert!(t.daemon.move_to_queue(id, MAIN_QUEUE, None).is_err());
        let mut mirror = Queue::new(0, "Mirror");
        mirror.kind = queue::QueueKind::Synchronize;
        let q = t.daemon.save_queue(mirror.clone()).unwrap();
        t.daemon.move_to_queue(id, q, None).unwrap();
        assert_eq!(t.daemon.status(id), Some(Status::Done), "it stays finished in the queue");

        // Unchanged: left alone.
        t.run_queue(q);
        assert_eq!(t.daemon.status(id), Some(Status::Done));
        assert_eq!(std::fs::read(&path).unwrap(), b"version 1");
        assert_eq!(t.daemon.hub.queue_of(id), Some(q), "a finished download stays in its queue");

        // Same size, new ETag: downloaded again over the same file, in the
        // same row.
        *file.lock().unwrap() = (b"version 2".to_vec(), Some("\"v2\"".to_string()));
        t.run_queue(q);
        let again = t.wait(id, Status::Done);
        assert_eq!(again.path, first.path, "the same file");
        assert_eq!(std::fs::read(&path).unwrap(), b"version 2");
        assert_eq!(again.validator.as_ref().and_then(|v| v.etag.as_deref()), Some("\"v2\""));
        assert_eq!(std::fs::read_dir(&t.dir).unwrap().count(), 1, "no second copy or leftover");
        assert_eq!(t.daemon.hub.queue_of(id), Some(q), "and it stays in the queue");

        // A check that fails keeps the file.
        file.lock().unwrap().1 = None;
        t.run_queue(q);
        let kept = t.wait(id, Status::Done);
        assert_eq!(std::fs::read(&path).unwrap(), b"version 2");
        assert!(kept.note.is_some_and(|n| n.contains("newer copy")), "it says why");

        // A gone file is downloaded again.
        file.lock().unwrap().1 = Some("\"v2\"".to_string());
        std::fs::remove_file(&path).unwrap();
        t.run_queue(q);
        t.wait(id, Status::Done);
        assert_eq!(std::fs::read(&path).unwrap(), b"version 2");

        // Made one-time again, it lets its finished downloads go.
        mirror.id = q;
        mirror.kind = queue::QueueKind::OneTime;
        t.daemon.save_queue(mirror).unwrap();
        assert_eq!(t.daemon.hub.queue_of(id), None);
    }

    #[test]
    fn a_synchronizing_project_downloads_changed_finds_again() {
        let t = setup("sync_project", Duplicate::Existing);
        let file = Arc::new(Mutex::new((b"manual 1".to_vec(), Some("\"m1\"".to_string()))));
        let url = serve_versions(Arc::clone(&file));
        let id = t.add(&url);
        let path = PathBuf::from(t.wait(id, Status::Done).path.unwrap());
        let found =
            grab::GrabFile { url: url.clone(), name: "mirror.bin".into(), page: String::new(), size: None, text: None };
        let mut project: grab::Project = serde_json::from_value(serde_json::json!({
            "name": "manuals",
            "request": GrabRequest { start: "http://127.0.0.1:1/".into(), ..GrabRequest::default() },
            "run": "download_checked",
        }))
        .unwrap();
        project.files = vec![found];
        project.checked = vec![true];
        let dir = t.dir.join("projects");

        // One-time: what is in the list already is not looked at.
        *file.lock().unwrap() = (b"manual 2!".to_vec(), Some("\"m2\"".to_string()));
        t.daemon.run_scheduled_grab(&dir, project.clone(), 1);
        assert_eq!(t.len(), 1);
        assert_eq!(t.daemon.status(id), Some(Status::Done));
        assert_eq!(std::fs::read(&path).unwrap(), b"manual 1");

        // Synchronizing: it is checked and downloaded again in its row, even
        // where a duplicate would get a new name.
        t.daemon.settings.lock().unwrap().if_duplicate = Duplicate::NewName;
        project.kind = queue::QueueKind::Synchronize;
        t.daemon.run_scheduled_grab(&dir, project, 2);
        assert_eq!(t.len(), 1, "no new row");
        let until = Instant::now() + Duration::from_secs(10);
        while std::fs::read(&path).unwrap() != b"manual 2!" {
            assert!(Instant::now() < until, "never downloaded again: {:?}", t.daemon.entry(id));
            t.daemon.tick();
            thread::sleep(Duration::from_millis(20));
        }
        t.wait(id, Status::Done);
        assert_eq!(
            grab::Project::load(&dir, "manuals").unwrap().kind,
            queue::QueueKind::Synchronize,
            "the type is saved"
        );
    }

    #[test]
    fn a_scheduled_project_stops_its_downloads_at_its_stop_time() {
        let t = setup("grab_stop_time", Duplicate::NewName);
        let url = serve_ranges(Arc::new(vec![7u8; 4 * 1024 * 1024]), Arc::new(AtomicU64::new(0)));
        let today = queue::local_now().day;
        let mut project: grab::Project = serde_json::from_value(serde_json::json!({
            "name": "slow",
            "request": GrabRequest { start: "http://127.0.0.1:1/".into(), ..GrabRequest::default() },
            "run": "download_checked",
            "schedule": { "start": "00:00", "stop": "01:00" },
        }))
        .unwrap();
        project.files =
            vec![grab::GrabFile { url, name: "slow.bin".into(), page: String::new(), size: None, text: None }];
        project.checked = vec![true];
        let dir = t.dir.join("projects");
        // Tomorrow's run, so its stop time is still ahead.
        t.daemon.run_scheduled_grab(&dir, project, today + 1);
        let main = || t.daemon.hub.queues.lock().unwrap().iter().find(|q| q.id == MAIN_QUEUE).cloned().unwrap();
        assert_eq!(main().stop_at_once, Some((today + 1) * 1440 + 60), "the queue's run has the project's stop time");
        assert!(main().running);
        let id = *main().order.first().unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while t.daemon.status(id) != Some(Status::Downloading) {
            assert!(Instant::now() < until, "never started: {:?}", t.daemon.entry(id));
            t.daemon.tick();
            thread::sleep(Duration::from_millis(20));
        }

        // The stop time comes: the queue stops, its download waits again.
        t.daemon
            .hub
            .update_queue(MAIN_QUEUE, |q| {
                q.stop_at_once = Some(queue::local_now().total());
                Ok(())
            })
            .unwrap();
        t.daemon.tick();
        assert!(!main().running, "the queue stopped");
        assert_eq!(main().stop_at_once, None, "the stop time was for that run only");
        t.wait(id, Status::Waiting);
        assert!(t.daemon.countdown.lock().unwrap().is_none(), "a stopped run does not do its when-done action");
    }

    #[test]
    fn a_scheduled_project_leaves_a_run_the_user_started_alone() {
        let t = setup("grab_user_run", Duplicate::NewName);
        let url = serve_ranges(Arc::new(vec![7u8; 4 * 1024 * 1024]), Arc::new(AtomicU64::new(0)));
        let today = queue::local_now().day;
        let mut project: grab::Project = serde_json::from_value(serde_json::json!({
            "name": "joins",
            "request": GrabRequest { start: "http://127.0.0.1:1/".into(), ..GrabRequest::default() },
            "run": "download_checked",
            "schedule": { "start": "00:00", "stop": "01:00" },
            "force_close": true,
        }))
        .unwrap();
        project.files =
            vec![grab::GrabFile { url, name: "joins.bin".into(), page: String::new(), size: None, text: None }];
        project.checked = vec![true];
        t.daemon.start_queue(MAIN_QUEUE, None).unwrap();
        t.daemon.run_scheduled_grab(&t.dir.join("projects"), project, today + 1);
        let main = t.daemon.hub.queues.lock().unwrap().iter().find(|q| q.id == MAIN_QUEUE).cloned().unwrap();
        assert!(main.running);
        assert_eq!(main.order.len(), 1, "the project's file joined the run");
        assert_eq!(main.stop_at_once, None, "the user's run keeps going past the project's stop time");
        assert_eq!(main.force_close_once, None);
        assert_eq!(main.when_done_once, None);
    }

    #[test]
    fn a_scheduled_project_force_closes_when_asked() {
        // Never really shut down, even if a check below fails.
        std::env::set_var(POWER_DRY_RUN_ENV, "1");
        let t = setup("grab_force_close", Duplicate::NewName);
        let dir = t.dir.join("projects");
        let mut project: grab::Project = serde_json::from_value(serde_json::json!({
            "name": "shutdown",
            "request": GrabRequest { start: "http://127.0.0.1:1/".into(), ..GrabRequest::default() },
            "run": "download_checked",
            "when_done": "shut_down",
            "force_close": true,
        }))
        .unwrap();
        assert!(project.force_close, "saved with the project");
        let countdown_forces = |t: &Setup| {
            let forced = t.daemon.countdown.lock().unwrap().as_ref().map(|c| (c.action, c.force_close));
            t.daemon.cancel_countdown();
            forced
        };

        // Nothing to download: done at once.
        t.daemon.run_scheduled_grab(&dir, project.clone(), 1);
        assert_eq!(countdown_forces(&t), Some((WhenDone::ShutDown, true)));

        // Done once the queue is, which does not force on its own.
        let file = Arc::new(Mutex::new((b"data".to_vec(), Some("\"d1\"".to_string()))));
        let url = serve_versions(file);
        project.files = vec![grab::GrabFile { url, name: "d.bin".into(), page: String::new(), size: None, text: None }];
        project.checked = vec![true];
        t.daemon.run_scheduled_grab(&dir, project, 2);
        assert_eq!(t.daemon.hub.queues.lock().unwrap()[0].force_close_once, Some(true));
        let until = Instant::now() + Duration::from_secs(10);
        while t.daemon.hub.queues.lock().unwrap().iter().any(|q| q.id == MAIN_QUEUE && q.running) {
            assert!(Instant::now() < until, "the run never ended");
            t.daemon.tick();
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(countdown_forces(&t), Some((WhenDone::ShutDown, true)));
        let main = t.daemon.hub.queues.lock().unwrap()[0].clone();
        assert!(
            !main.force_close && main.force_close_once.is_none(),
            "the queue's own choice is back for its next run"
        );
    }

    /// A download that starts while its dialog is open, the dialog then
    /// answered before or after it finishes: either way the file ends up
    /// where the answer says, whole, and its bytes come over once.
    #[test]
    fn a_download_started_while_asking_keeps_its_bytes() {
        const LEN: usize = 1024 * 1024;
        for finish_first in [false, true] {
            let t = setup(if finish_first { "early_done" } else { "early" }, Duplicate::NewName);
            {
                let mut s = t.daemon.settings.lock().unwrap();
                s.ask_where_to_save = true;
                s.start_while_asking = true;
                s.connections = 2;
            }
            let body: Arc<Vec<u8>> = Arc::new((0..LEN).map(|i| (i % 251) as u8).collect());
            let sent = Arc::new(AtomicU64::new(0));
            let id = t.add(&serve_ranges(Arc::clone(&body), Arc::clone(&sent)));
            let asked = t.wait(id, Status::Confirming);
            let wanted = if finish_first { LEN as u64 } else { LEN as u64 / 4 };
            let until = Instant::now() + Duration::from_secs(30);
            while t.daemon.entry(id).unwrap().done < wanted {
                assert!(Instant::now() < until, "the early download never got going");
                thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(t.daemon.entry(id).unwrap().status, Status::Confirming, "still asking");
            assert!(!Path::new(asked.path.as_deref().unwrap()).exists(), "nothing written where it was proposed");

            let chosen = t.dir.join("chosen");
            t.daemon
                .confirm(Answer {
                    folder: Some(chosen.display().to_string()),
                    filename: Some("big.bin".into()),
                    ..Answer::start(id)
                })
                .unwrap();
            let done = t.wait(id, Status::Done);
            assert_eq!(done.path.map(PathBuf::from), Some(chosen.join("big.bin")));
            assert_eq!(std::fs::read(chosen.join("big.bin")).unwrap(), *body);
            // Two one-byte probes on top of the file itself.
            assert!(sent.load(Ordering::Relaxed) < LEN as u64 + 4096, "sent {}", sent.load(Ordering::Relaxed));
        }
    }

    #[test]
    fn an_early_download_stopped_after_saving_counts_as_finished() {
        let dir = std::env::temp_dir().join(format!("uidm_early_late_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("big.bin");
        let partial = dir.join("big.bin.part");
        let early = |file: &Path| Early {
            real: 1,
            dir: dir.clone(),
            file: Some(file.to_path_buf()),
            target: Some(target.clone()),
            total: Some(4),
            end: Some(None),
            discard: false,
        };

        // Stopped half way: the partial file and its resume file are left.
        std::fs::write(&partial, b"ab").unwrap();
        std::fs::write(engine::sidecar_path(&partial), b"{}").unwrap();
        assert_eq!(early_finished_anyway(&early(&partial)), None);

        // Saved before the stop landed: only the whole file is left.
        std::fs::remove_file(&partial).unwrap();
        std::fs::remove_file(engine::sidecar_path(&partial)).unwrap();
        std::fs::write(&target, b"abcd").unwrap();
        assert_eq!(early_finished_anyway(&early(&partial)), Some(target.as_path()));

        // Written in place without resume data but short: not finished.
        std::fs::write(&target, b"abc").unwrap();
        assert_eq!(early_finished_anyway(&early(&target)), None);
        // In place with its resume file: stopped, not finished.
        std::fs::write(&target, b"abcd").unwrap();
        std::fs::write(engine::sidecar_path(&target), b"{}").unwrap();
        assert_eq!(early_finished_anyway(&early(&target)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn canceling_the_dialog_drops_the_early_download() {
        let t = setup("early_cancel", Duplicate::NewName);
        {
            let mut s = t.daemon.settings.lock().unwrap();
            s.ask_where_to_save = true;
            s.start_while_asking = true;
        }
        let body: Arc<Vec<u8>> = Arc::new(vec![7u8; 1024 * 1024]);
        let id = t.add(&serve_ranges(body, Arc::new(AtomicU64::new(0))));
        t.wait(id, Status::Confirming);
        let until = Instant::now() + Duration::from_secs(30);
        while t.daemon.hub.early_of(id).is_none_or(|s| t.daemon.hub.early.lock().unwrap()[&s].file.is_none()) {
            assert!(Instant::now() < until, "the early download never started");
            thread::sleep(Duration::from_millis(20));
        }
        let shadow = t.daemon.hub.early_of(id).unwrap();
        let dir = t.daemon.hub.early.lock().unwrap()[&shadow].dir.clone();
        t.daemon.cancel(id);
        t.wait(id, Status::Canceled);
        let until = Instant::now() + Duration::from_secs(10);
        while dir.exists() || t.daemon.hub.early.lock().unwrap().contains_key(&shadow) {
            assert!(Instant::now() < until, "the early download was not dropped");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_finished_video_converts_into_a_row_of_its_own() {
        let Some(ffmpeg) = Settings::default().ffmpeg() else {
            eprintln!("no ffmpeg; skipped");
            return;
        };
        let t = setup("convert", Duplicate::NewName);
        let clip = t.dir.join("talk.mkv");
        let id = t.finished("https://x/talk.mkv", &clip);
        let made = uidm::platform::no_window(&mut Command::new(&ffmpeg))
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=160x120:rate=10",
            ])
            .args(["-f", "lavfi", "-i", "sine=duration=1", "-c:v", "libx264", "-c:a", "aac", "-shortest"])
            .arg(&clip)
            .status()
            .unwrap();
        assert!(made.success());
        t.daemon.convert(id, uidm::media::convert::Target::M4a, true).unwrap();
        let new = t.daemon.hub.table.lock().unwrap().max_id();
        let done = t.wait(new, Status::Done);
        let out = PathBuf::from(done.path.unwrap());
        assert_eq!(out, t.dir.join("talk.m4a"));
        assert!(std::fs::metadata(&out).unwrap().len() > 0);
        assert_eq!(done.note.as_deref(), Some("Converted from talk.mkv"));
        assert!(t.daemon.entry(id).is_none() && !clip.exists(), "the original went, as asked");
        assert!(t.daemon.convert(new + 100, uidm::media::convert::Target::Mp3, false).is_err());
    }

    /// Each request's path and the Cookie header it brought.
    type CookieLog = Arc<Mutex<Vec<(String, Option<String>)>>>;

    /// A page server that serves `pages` (path, body) and keeps the Cookie
    /// header each request brought, by path.
    fn serve_pages(pages: Vec<(&'static str, String)>) -> (u16, CookieLog) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut req = Vec::new();
                let mut buf = [0u8; 1024];
                while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => req.extend_from_slice(&buf[..n]),
                    }
                }
                let text = String::from_utf8_lossy(&req).to_string();
                let path = text.split_whitespace().nth(1).unwrap_or("/").to_string();
                let cookie = text
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("cookie:"))
                    .map(|l| l[7..].trim().to_string());
                log.lock().unwrap().push((path.clone(), cookie));
                let (status, body) = match pages.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => ("200 OK", body.clone()),
                    None => ("404 Not Found", String::new()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body.as_bytes());
            }
        });
        (port, seen)
    }

    #[test]
    fn a_crawl_sends_the_login_sites_cookies_to_that_site_only() {
        let (other, other_seen) = serve_pages(vec![("/elsewhere", "<p>other</p>".to_string())]);
        let site_pages = vec![
            ("/", format!("<a href=\"/members\">m</a> <a href=\"http://localhost:{other}/elsewhere\">o</a>")),
            ("/members", "<a href=\"a.pdf\">a</a>".to_string()),
        ];
        let (site, site_seen) = serve_pages(site_pages);
        let t = setup("grab_login_cookies", Duplicate::NewName);
        let req = GrabRequest {
            start: format!("http://127.0.0.1:{site}/"),
            depth: 2,
            same_site: false,
            delay_ms: 200,
            login_url: Some(format!("http://127.0.0.1:{site}/login")),
            // No extension is connected, so the pasted cookies are used.
            login_cookies: Some("Cookie: session=abc".into()),
            ..GrabRequest::default()
        };
        let stop = AtomicBool::new(false);
        let login = t.daemon.login_cookies(&req);
        let out = t.daemon.run_crawl(&req, login.as_deref(), &stop, |_| {});
        assert_eq!(out.files.len(), 1, "the members page was read: {:?}", out.files);
        let site_seen = site_seen.lock().unwrap().clone();
        let pages: Vec<_> = site_seen.iter().filter(|(p, _)| p != "/robots.txt").collect();
        assert!(pages.len() >= 2 && pages.iter().all(|(_, c)| c.as_deref() == Some("session=abc")), "{site_seen:?}");
        let other_seen = other_seen.lock().unwrap().clone();
        assert!(other_seen.iter().any(|(p, _)| p == "/elsewhere"), "the other site was read: {other_seen:?}");
        assert!(other_seen.iter().all(|(_, c)| c.is_none()), "no cookies to another host: {other_seen:?}");
        // The files it adds carry them on the login site only.
        let elsewhere = grab::GrabFile { url: format!("http://localhost:{other}/b.pdf"), ..out.files[0].clone() };
        let ids = t.daemon.download_in_queue(
            &req,
            login.as_deref(),
            &[out.files[0].clone(), elsewhere],
            queue::MAIN_QUEUE,
            false,
        );
        let requests = t.daemon.hub.requests.lock().unwrap();
        let cookies: Vec<_> = ids.iter().map(|id| requests.get(id).and_then(|d| d.cookies.clone())).collect();
        assert_eq!(cookies, [Some("session=abc".to_string()), None]);
    }
}
