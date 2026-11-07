//! UIDM daemon: the long-lived process that owns the download engine.
//!
//! Clients (a host per browser connection, and the desktop window) connect over
//! loopback TCP and speak the same length-prefixed JSON frames as the browser.
//! Each connection starts with a mutual handshake over the token this daemon
//! wrote at start (see `uidm_infrastructure::ipc`); until the client passes it, it gets no
//! events and its requests are not read.
//!
//! Replies to a request (`hello`, `pong`, `ack`, `downloads`, `added_many`,
//! `media_info`, the site grabber's `grab_progress` and `grab_done`, `error`)
//! go to the client that asked. Download events (`added`, `started`, `progress`, `done`,
//! `failed`, `canceled`, `removed`) go to every connected client. The daemon
//! also applies each event to its download store (`uidm_infrastructure::store`),
//! so a client that connects late can ask for the whole list.
//!
//! Because downloads live here rather than in the host, they keep running when
//! the browser closes. The daemon exits on its own once it has had no clients
//! and no downloads for `IDLE_EXIT`.
//!
//! The list survives restarts: every change is written to the history file
//! (debounced, and again on exit), and the requests that may still be retried
//! go to a DPAPI-encrypted file next to it (see `uidm_infrastructure::history`). Settings are
//! loaded at start and changed with `set_settings` (see `uidm_infrastructure::settings`).
//!
//! With the save-as dialog on, a new download is probed first and waits as
//! `confirming` until a window answers with `confirm`. If no window is open
//! the daemon starts one; if none connects within `front::CONFIRM_FALLBACK`, the
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

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use tracing::{error, info, warn};
use uidm_application::downloads::{
    AddLater, AddMany, AfterDownload, CancelDownload, ConfirmAnswer, ConfirmWarning, Confirmed, ConvertDownload,
    Existing, Held, HoldForAnswer, PauseAll, Probed, Redownload, RefreshLink, Refreshed, RemoveDownload, ResumeAll,
    RetryDownload, SetLimitRemember, SetOnDone, SetProperties, SetSpeedLimit, StartOrWait, UseExisting,
};
use uidm_application::grab::RunGrabSchedules;
use uidm_application::power::{CancelCountdown, CountdownNow, RunWhenDone};
use uidm_application::queues::{
    DeleteQueue, MoveToQueue, QueueEvent, RemoveFromQueue, RunQueues, RunSchedules, SaveQueue, Scheduled, StartQueue,
    StopQueue,
};
use uidm_application::quota::{CheckQuota, QuotaEvent, QuotaStatus};
use uidm_application::update::{CheckForUpdate, ClaimWeeklyCheck, InstallUpdate};
use uidm_domain::download::Scan;
use uidm_domain::download::{Answer, Download, DownloadEntry, DuplicateStep, OnDone, Properties, Status};
use uidm_domain::ports::{DownloadControl, DownloadList};
use uidm_domain::queue::{
    self, PowerRequest, PowerStep, Queue, ScheduleStep, StopReason, WhenDone, COUNTDOWN_SECS, MAIN_QUEUE,
};
use uidm_domain::quota::QuotaLimit;
use uidm_infrastructure::app::MAX_MESSAGE_BYTES;
use uidm_infrastructure::clock::{LocalClock, StoppedClock};
use uidm_infrastructure::connect::ProxyRetries;
use uidm_infrastructure::conversions::Conversions;
use uidm_infrastructure::engine::DownloadRequest;
use uidm_infrastructure::engine::{self, Engine};
use uidm_infrastructure::files::DiskFiles;
use uidm_infrastructure::front::{self, AnswerWait, Front, Hook, Notifier, Windows};
use uidm_infrastructure::grab::{self, GrabRequest};
use uidm_infrastructure::history;
use uidm_infrastructure::ipc::{self, Handshake, AUTH_TIMEOUT, DAEMON_LABEL, HOST_LABEL};
use uidm_infrastructure::live_settings::{LiveSettings, QueueChimes};
use uidm_infrastructure::logging;
use uidm_infrastructure::net::{self, Login};
use uidm_infrastructure::power;
use uidm_infrastructure::power_control::{self, PowerOff, RunEnds};
use uidm_infrastructure::protocol::{read_message, write_frame, CommandLine, Incoming, Outgoing};
use uidm_infrastructure::queue_runs::{PauseBook, RunBook};
use uidm_infrastructure::quota::QuotaHold;
#[cfg(windows)]
use uidm_infrastructure::registry;
use uidm_infrastructure::scan;
use uidm_infrastructure::settings::{self, Duplicate, FileExists, Settings};
use uidm_infrastructure::store::{check_body, DownloadStore, Ended, Intake, StoreEvents, StoreFiles};
use uidm_infrastructure::transfers::{self, EarlyHandOver, GoOn, SyncChecker, Transfers};
use uidm_infrastructure::update::GitHub;

/// Exit after this long with no clients and no queued or running downloads.
const IDLE_EXIT: Duration = Duration::from_secs(60);
const IDLE_POLL: Duration = Duration::from_secs(5);
/// Collect changes for this long before writing the history.
const HISTORY_DEBOUNCE: Duration = Duration::from_secs(1);

/// Links the link picker's probes ask about at once.
const LINK_PROBES: usize = 4;

/// A failure is announced after this long, so a download the browser
/// extension takes back (and removes) right away is not announced at all.
const FAILED_GRACE: Duration = Duration::from_secs(1);
/// The icon Windows shows on UIDM's notifications, written next to the
/// history so the registry can point at it.
#[cfg(windows)]
const ICON_PNG: &[u8] = include_bytes!("../../../../extension/icons/128.png");
/// The `client` a desktop window names in its `hello`.
const WINDOW_CLIENT: &str = "window";

/// How long a command line waits for the window its `uidm-gui` opens. One
/// older than this belonged to a window that never connected.
const COMMAND_LINE_WAIT: Duration = Duration::from_secs(60);
/// How often the scheduler looks at the clock when nothing wakes it.
const SCHEDULER_POLL: Duration = Duration::from_secs(5);
/// How often a running daemon moves logs over `logging::MAX_BYTES` aside.
const LOG_TRIM_EVERY: Duration = Duration::from_secs(60 * 60);
/// How often the keep-awake thread looks for running work.
const AWAKE_POLL: Duration = Duration::from_secs(2);
/// How the site grabber names itself to servers.
const GRABBER_AGENT: &str = "Mozilla/5.0 (compatible; UIDM site grabber; obeys robots.txt)";

/// This copy's version.
const VERSION: &str = env!("CARGO_PKG_VERSION");

static NEXT_CLIENT: AtomicU64 = AtomicU64::new(1);

/// The connected clients, and how the store's changes reach them: each
/// change is framed and sent to every client, and endings go to the
/// notification thread. Connections stay in the daemon until the protocol
/// moves behind an adapter (#196).
struct Clients {
    /// Connected clients by id, each with a channel drained by its writer.
    clients: Mutex<HashMap<u64, Sender<Vec<u8>>>>,
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
    /// Wakes the scheduler when a download ends or a queue changes.
    wake: Sender<()>,
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

impl Windows for Clients {
    fn window_open(&self) -> bool {
        !self.windows().is_empty()
    }
}

impl StoreEvents for Clients {
    fn announce(&self, event: &Outgoing) {
        self.broadcast(event);
    }

    fn ended(&self, how: Ended) {
        let note = match how {
            Ended::Finished(id) => Note::Finished(id),
            Ended::Failed(id) => Note::Failed(id),
            Ended::NameTaken(id) => Note::NameTaken(id),
        };
        let _ = self.notes.send(note);
    }

    fn wake(&self) {
        let _ = self.wake.send(());
    }
}

impl Clients {
    /// The clients that are windows. Only single inserts and removals change
    /// the set, so a lock poisoned by a panicking thread still holds a
    /// usable one.
    fn windows(&self) -> MutexGuard<'_, HashSet<u64>> {
        self.windows.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether a client other than a window is connected: the browser
    /// extension, through its host.
    fn extension_connected(&self) -> bool {
        let windows = self.windows().len();
        self.clients.lock().unwrap().len() > windows
    }

    fn window_open(&self) -> bool {
        !self.windows().is_empty()
    }

    /// Send `msg` to the desktop windows only.
    fn to_windows(&self, msg: &Outgoing) {
        let Ok(frame) = serde_json::to_vec(msg) else {
            return;
        };
        let windows = self.windows();
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
        *self.browser_seen.lock().unwrap() = Some(uidm_infrastructure::state::unix_now());
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
        self.windows().remove(&id);
        let last = {
            let mut browsers = self.browsers.lock().unwrap();
            browsers.remove(&id) && browsers.is_empty()
        };
        if last {
            *self.browser_seen.lock().unwrap() = Some(uidm_infrastructure::state::unix_now());
            self.to_windows(&self.browser_link());
        }
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
    store: StoreFiles,
}

impl Paths {
    fn from_env() -> Self {
        let warn = |what: &str, r: io::Result<PathBuf>| r.map_err(|e| warn!(file = what, error = %e, "no file")).ok();
        Paths {
            settings: warn("settings", settings::settings_path()),
            store: StoreFiles {
                history: warn("history", history::history_path()),
                requests: warn("saved requests", history::requests_path()),
                quota: warn("quota", uidm_infrastructure::quota::quota_path()),
            },
        }
    }
}

struct Daemon {
    /// The engine, which `transfers` owns.
    engine: Arc<Engine>,
    /// The download list, the kept requests and the queues.
    store: Arc<DownloadStore>,
    clients: Arc<Clients>,
    settings: Arc<LiveSettings>,
    /// Starts, stops and checks downloads, and applies settings.
    transfers: Arc<Transfers>,
    /// Notifications for finished and failed downloads.
    notifier: Arc<Notifier>,
    /// The windows and notifications, for the use cases.
    front: Arc<Front>,
    /// Save-as dialogs waiting for a window.
    answers: AnswerWait,
    /// Downloads started while their dialog is open.
    early: Arc<EarlyHandOver>,
    /// Conversions with ffmpeg.
    conversions: Conversions,
    /// A synchronizing queue's checks.
    sync: SyncChecker,
    /// What a queue's run does when it is done.
    run_ends: RunEnds,
    /// Keeps two scheduler passes from running at once.
    tick_lock: Mutex<()>,
    /// Each client's running site crawl, by its stop flag.
    grabs: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    /// Each client's link picker probes, by their stop flag.
    link_probes: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    /// Links from a page, held for the window that is being opened.
    picked_links: Mutex<Option<Outgoing>>,
    /// A command line held for the window its `uidm-gui` is opening, and
    /// when it came.
    command_line: Mutex<Option<(CommandLine, Instant)>>,
    /// Crawls waiting for the browser's cookies, by request.
    cookie_waits: Mutex<HashMap<u64, Sender<Option<String>>>>,
    /// Crawls waiting for a page the browser renders: its HTML and address.
    render_waits: Mutex<HashMap<u64, Sender<Rendered>>>,
    /// Grabber projects whose scheduled run is going on, by name.
    grab_runs: Arc<Mutex<HashSet<String>>>,
    /// When the projects' schedules were last looked at.
    grab_checked: Mutex<Option<Instant>>,
    /// Addresses retried through the browser's proxy after failing through
    /// UIDM's own proxy setting; they keep using it.
    browser_proxy_retries: Arc<ProxyRetries>,
    /// Each started queue's current run, and its retries.
    run_book: RunBook,
    /// What `pause_all` stopped, for `resume_all`.
    pause_book: PauseBook,
    /// The download quota's count and what it stopped.
    quota: QuotaHold,
    /// Closing UIDM and the countdown before a power action.
    power: Arc<PowerOff>,
    /// The queue sounds.
    chimes: QueueChimes,
}

impl Daemon {
    /// Load settings, history and saved requests, and start the engine with
    /// them. Returns the receivers the history writer and the notification
    /// thread wait on.
    fn new(paths: Paths) -> (Self, Receiver<()>, Receiver<Note>, Receiver<()>) {
        let mut settings = match &paths.settings {
            Some(path) => settings::load(path).unwrap_or_else(|e| {
                warn!(path = %path.display(), error = %e, "ignoring the settings file");
                Settings::default()
            }),
            None => Settings::default(),
        };
        // Settings from before each proxy had its own login: the HTTPS and
        // SOCKS proxies keep the shared password they used.
        if std::mem::take(&mut settings.proxy_logins_split) {
            let saved = net::split_proxy_password().map_err(|e| e.to_string()).and_then(|()| {
                paths.settings.as_ref().map_or(Ok(()), |p| settings::save(&settings, p).map_err(|e| e.to_string()))
            });
            if let Err(e) = saved {
                warn!(error = %e, "could not give each proxy its own login");
            }
        }
        // IDM's "Always turn on Speed Limiter on startup": the limit is on
        // after every start, even if it was turned off last time.
        if settings.speed_limit_on_startup {
            settings.speed_limit_enabled = true;
        }
        let (notes, notes_rx) = channel();
        let (wake, wake_rx) = channel();
        let clients = Arc::new(Clients {
            clients: Mutex::new(HashMap::new()),
            windows: Mutex::new(HashSet::new()),
            browsers: Mutex::new(HashSet::new()),
            browser_seen: Mutex::new(None),
            notes,
            wake,
        });
        let (store, dirty_rx, next_id) = DownloadStore::open(paths.store.clone(), Arc::clone(&clients) as _);
        let store = Arc::new(store);
        let settings = Arc::new(LiveSettings::new(settings, paths.settings.clone()));
        let retries = Arc::new(ProxyRetries::default());
        let events: Arc<dyn StoreEvents> = Arc::clone(&clients) as _;
        let windows: Arc<dyn Windows> = Arc::clone(&clients) as _;
        let transfers = Arc::new(Transfers::new(
            Arc::clone(&store),
            Arc::clone(&events),
            Arc::clone(&settings),
            Arc::clone(&retries),
            next_id,
        ));
        let engine = Arc::clone(transfers.engine());
        let chimes = QueueChimes(Arc::clone(&settings));
        let quota = QuotaHold::new(Arc::clone(&store), Arc::clone(&events));
        let power = {
            let (saver, shown) = (Arc::clone(&transfers), Arc::clone(&windows));
            Arc::new(PowerOff::new(
                Arc::clone(&events),
                Box::new(move || saver.save_history()),
                Box::new(move || front::show_window(&*shown)),
            ))
        };
        let retry: Hook<u64> = {
            let (store, transfers) = (Arc::clone(&store), Arc::clone(&transfers));
            Arc::new(move |id| {
                if let Err(e) = RetryDownload::new(&*store, &*store, &*transfers).run(id) {
                    warn!(download_id = id, error = %e, "could not retry from a notification");
                }
            })
        };
        let notifier = Notifier::new(Arc::clone(&store), Arc::clone(&windows), retry);
        let front = Arc::new(Front::new(Arc::clone(&notifier), Arc::clone(&settings), Arc::clone(&windows)));
        let go_on: GoOn = {
            let (store, transfers, settings) = (Arc::clone(&store), Arc::clone(&transfers), Arc::clone(&settings));
            Arc::new(move |id, start, queue| {
                let later = settings.lock().later_queue;
                if let Err(e) = StartOrWait::new(&*store, &*store, &*transfers).run(id, start, queue, later) {
                    warn!(download_id = id, error = %e, "could not go on with the download");
                }
            })
        };
        let early = Arc::new(EarlyHandOver::new(Arc::clone(&transfers), go_on));
        let confirm: Hook<Answer> = {
            let (store, transfers, front, early) =
                (Arc::clone(&store), Arc::clone(&transfers), Arc::clone(&front), Arc::clone(&early));
            Arc::new(move |answer: Answer| {
                let id = answer.id;
                if let Err(e) = confirm_answer(&store, &transfers, &front, &early, answer) {
                    warn!(download_id = id, error = %e, "could not start");
                }
            })
        };
        let answers = AnswerWait::new(Arc::clone(&store), Arc::clone(&windows), confirm);
        let remove_original: Hook<u64> = {
            let (store, transfers) = (Arc::clone(&store), Arc::clone(&transfers));
            Arc::new(move |id| {
                if let Err(e) = RemoveDownload::new(&*store, &*store, &*transfers, &DiskFiles).run(id, true) {
                    warn!(download_id = id, error = %e, "kept the original of a converted download");
                }
            })
        };
        let conversions =
            Conversions::new(Arc::clone(&engine), Arc::clone(&store), Arc::clone(&settings), remove_original);
        let sync = SyncChecker::new(Arc::clone(&transfers));
        let run_ends = {
            let power = Arc::clone(&power);
            RunEnds::new(
                Arc::clone(&settings),
                Box::new(move |action, queue: &str, force_close| when_done(&power, action, queue, None, force_close)),
            )
        };

        let daemon = Daemon {
            engine,
            store,
            clients,
            settings,
            transfers,
            notifier,
            front,
            answers,
            early,
            conversions,
            sync,
            run_ends,
            tick_lock: Mutex::new(()),
            grabs: Mutex::new(HashMap::new()),
            link_probes: Mutex::new(HashMap::new()),
            picked_links: Mutex::new(None),
            command_line: Mutex::new(None),
            cookie_waits: Mutex::new(HashMap::new()),
            render_waits: Mutex::new(HashMap::new()),
            grab_runs: Arc::default(),
            grab_checked: Mutex::new(None),
            browser_proxy_retries: retries,
            chimes,
            run_book: RunBook::default(),
            pause_book: PauseBook::default(),
            quota,
            power,
        };
        (daemon, dirty_rx, notes_rx, wake_rx)
    }

    /// Put download `id` in queue `queue` at `position` (or at the end). A
    /// download that is not running waits there.
    fn move_to_queue(&self, id: u64, queue: u64, position: Option<usize>) -> Result<(), String> {
        MoveToQueue::new(&*self.store, &*self.store).run(id, queue, position).map_err(|e| e.to_string())
    }

    /// Take download `id` out of its queue, keeping it in the list (see
    /// `RemoveFromQueue`).
    fn remove_from_queue(&self, id: u64) -> Result<(), String> {
        RemoveFromQueue::new(&*self.store, &*self.store).run(id).map_err(|e| e.to_string())
    }

    /// Create a queue (id 0) or change one.
    fn save_queue(&self, edited: Queue) -> Result<u64, String> {
        SaveQueue::new(&*self.store, &*self.store).run(edited).map_err(|e| e.to_string())
    }

    /// Delete a queue. Its downloads move to the main queue; the running ones
    /// keep running.
    fn delete_queue(&self, id: u64) -> Result<(), String> {
        DeleteQueue::new(&*self.store, &*self.store, &self.run_book).run(id).map_err(|e| e.to_string())
    }

    /// Start a queue: its waiting downloads run in order. `scheduled_day` is
    /// the day whose scheduled run this is, or `None` when started by hand.
    fn start_queue(&self, id: u64, scheduled_day: Option<i64>) -> Result<(), String> {
        StartQueue::new(&*self.store, &self.run_book, &LocalClock, &self.chimes)
            .run(id, scheduled_day)
            .map_err(|e| e.to_string())
    }

    /// Stop a queue. Its running downloads stop and wait in it again, with
    /// their progress kept.
    fn stop_queue(&self, id: u64) -> Result<(), String> {
        StopQueue::new(&*self.store, &*self.store, &self.run_book, &*self.transfers).run(id).map_err(|e| e.to_string())
    }

    /// One pass of the scheduler: start and stop queues on their schedules,
    /// start waiting downloads in started queues, retry failed ones, and end
    /// the runs that are done.
    fn tick(self: &Arc<Self>) {
        let _guard = self.tick_lock.lock().unwrap();
        self.check_quota();
        let scheduled =
            RunSchedules::new(&*self.store, &*self.store, &self.run_book, &*self.transfers, &LocalClock, &self.chimes)
                .run();
        for Scheduled { queue, step } in scheduled {
            let why = match step {
                ScheduleStep::Start { .. } => "schedule starts",
                ScheduleStep::Stop(StopReason::ScheduleEnded) => "schedule stops",
                ScheduleStep::Stop(StopReason::GrabberStopTime) => "the grabber project's stop time stops",
            };
            info!(queue = %queue, why, "schedule step");
        }

        let events =
            RunQueues::new(&*self.store, &*self.store, &self.run_book, &*self.transfers, &self.sync, &self.run_ends)
                .run();
        for event in events {
            match event {
                QueueEvent::Retried { queue, id, attempt, of } => {
                    info!(queue = %queue, download_id = id, attempt, of, "queue retries a download")
                }
                QueueEvent::Finished { queue } => info!(queue = %queue, "queue is done"),
            }
        }
    }

    /// Check the finished downloads at `urls` for changes, and put those
    /// whose server copy changed in `queue` to be downloaded again, starting
    /// it. Returns how many changed, and the addresses it found finished in
    /// the list (which need no new row).
    fn resync(&self, urls: &[String], queue: u64) -> (usize, HashSet<String>) {
        let urls: HashSet<&str> = urls.iter().map(String::as_str).collect();
        let (ids, listed): (Vec<u64>, HashSet<String>) = {
            let table = self.store.table();
            table
                .entries()
                .filter(|e| e.status == Status::Done && urls.contains(e.url.as_str()))
                .map(|e| (e.id, e.url.clone()))
                .unzip()
        };
        let changed = ids
            .into_iter()
            .filter(|id| {
                matches!(self.transfers.check_for_change(*id), Ok(true)) && self.move_to_queue(*id, queue, None).is_ok()
            })
            .count();
        if changed > 0 && !self.store.queues().iter().any(|q| q.id == queue && q.running) {
            if let Err(e) = self.start_queue(queue, None) {
                warn!(queue = %queue, error = %e, "could not start the queue");
            }
        }
        (changed, listed)
    }

    /// Do what a finished queue, or with `file` one finished download, asks
    /// (see `RunWhenDone`).
    fn when_done(&self, action: WhenDone, queue: &str, file: Option<&str>, force_close: bool) {
        when_done(&self.power, action, queue, file, force_close);
    }

    /// Run what download `id` asked to happen once it finished, then clear
    /// it so downloading it again does not repeat it.
    fn after_download(&self, id: u64) {
        AfterDownload::new(&*self.store, &*self.power).run(id);
    }

    /// Do what `uidm-gui`'s command line asked. `/q` counts only when that
    /// call opened the window (`started`), as in IDM, where it works only
    /// for the first copy. Problems go to the windows, since the caller may
    /// be gone.
    fn run_command_line(self: &Arc<Self>, cmd: CommandLine, started: bool) {
        if let Some(mut download) = cmd.download {
            download.limit_browser_request();
            let (id, url) = self.add(download);
            info!(download_id = id, url = %url, "add from the command line");
            if cmd.quit_after && started {
                let exit = OnDone { then: WhenDone::Exit, ..OnDone::default() };
                if let Err(e) = self.set_on_done(id, Some(exit)) {
                    warn!(download_id = id, error = %e, "could not set /q");
                }
            }
        }
        if cmd.start_queue {
            match self.start_queue(MAIN_QUEUE, None) {
                Ok(()) => {
                    info!("start the main queue from the command line");
                    let _ = self.clients.wake.send(());
                }
                Err(message) => self.clients.to_windows(&Outgoing::Error { message }),
            }
        }
    }

    /// Set or clear what download `id` does once it finishes.
    fn set_on_done(&self, id: u64, on_done: Option<OnDone>) -> Result<(), String> {
        SetOnDone::new(&*self.store).run(id, on_done).map_err(|e| e.to_string())
    }

    /// End the countdown early and carry out its action now.
    fn countdown_now(&self) {
        CountdownNow::new(&*self.power).run();
    }

    fn cancel_countdown(&self) {
        if CancelCountdown::new(&*self.power).run() {
            info!("countdown canceled");
        }
    }

    /// The countdown still running, as the message a window joining now needs.
    fn countdown_message(&self) -> Option<Outgoing> {
        self.power.countdown_message()
    }

    /// The next scheduled start, of a queue or a grabber project, whose
    /// schedule asks to wake the computer.
    fn next_wake(&self, now: queue::Moment) -> Option<queue::Moment> {
        let queues = self.store.queues().clone();
        let from_queues = queues.iter().filter(|q| !q.running).filter_map(|q| {
            q.schedule.as_ref().filter(|s| s.wake).and_then(|s| s.next_run(now, q.last_scheduled_start))
        });
        let projects = grab::projects_dir()
            .map(|dir| {
                grab::recent_projects(&dir)
                    .iter()
                    .filter_map(|name| grab::load_project(&dir, name).ok())
                    .filter_map(|p| {
                        p.schedule.as_ref().filter(|s| s.wake).and_then(|s| s.next_run(now, p.last_scheduled_start))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        from_queues.chain(projects).filter(|m| *m > now).min()
    }

    /// Whether work is running that sleep would interrupt: a download, a
    /// site grab or a conversion.
    fn is_working(&self) -> bool {
        self.engine.active_count() > 0
            || !self.grabs.lock().unwrap().is_empty()
            || !self.grab_runs.lock().unwrap().is_empty()
            || self.conversions.any_running()
    }

    /// Whether the daemon must stay up for a queue: one is started, one has a
    /// schedule ahead, or a countdown runs.
    fn queues_need_daemon(&self) -> bool {
        let now = uidm_infrastructure::clock::local_now();
        let projects = || {
            let Ok(dir) = grab::projects_dir() else {
                return false;
            };
            !self.grab_runs.lock().unwrap().is_empty()
                || grab::recent_projects(&dir).iter().any(|name| {
                    grab::load_project(&dir, name)
                        .ok()
                        .and_then(|p| p.schedule)
                        .is_some_and(|s| s.next_start(now).is_some() || s.open_run(now).is_some())
                })
        };
        self.countdown_message().is_some()
            || projects()
            || self.store.queues().iter().any(|q| {
                q.running
                    || q.schedule.as_ref().is_some_and(|s| s.next_start(now).is_some() || s.open_run(now).is_some())
            })
    }

    fn entry(&self, id: u64) -> Option<DownloadEntry> {
        self.store.table().get(id).cloned()
    }

    /// Bring the window forward: ask an open one to show itself, or start one.
    fn show_window(&self) {
        front::show_window(&*self.clients);
    }

    /// Stop every queued or running download, remembering them for
    /// `resume_all`.
    fn pause_all(&self) {
        PauseAll::new(&*self.store, &*self.store, &self.run_book, &*self.transfers, &self.pause_book).run();
    }

    /// The quota the settings ask for, if it is on.
    fn quota_limit(&self) -> Option<QuotaLimit> {
        let s = self.settings.lock();
        s.quota().map(|(bytes, window)| QuotaLimit { bytes, window, warn: s.quota_warn })
    }

    /// The quota's state for a window that just connected, if downloads are
    /// paused for it or it is nearly reached.
    fn quota_message(&self) -> Option<Outgoing> {
        let now = uidm_infrastructure::quota::now_minute();
        QuotaStatus::new(&self.quota).run(self.quota_limit(), now).map(uidm_infrastructure::quota::notice_message)
    }

    /// Pause everything when the download quota is reached, warn shortly
    /// before, and resume what it stopped once the window allows (see
    /// `CheckQuota`). Runs with the scheduler.
    fn check_quota(&self) {
        let now = uidm_infrastructure::quota::now_minute();
        let check = CheckQuota::new(
            &*self.store,
            &*self.store,
            &self.run_book,
            &*self.transfers,
            &LocalClock,
            &self.chimes,
            &self.quota,
        );
        match check.run(self.quota_limit(), now) {
            Some(QuotaEvent::Reached { used, limit }) => {
                info!(used, limit, "quota reached")
            }
            Some(QuotaEvent::Resumed { failed }) => {
                info!("quota allows downloads again");
                for (id, e) in failed {
                    warn!(download_id = id, error = %e, "could not resume");
                }
            }
            None => {}
        }
    }

    /// Start again what `pause_all` stopped, and every interrupted download.
    fn resume_all(&self) {
        let resumed = ResumeAll::new(
            &*self.store,
            &*self.store,
            &self.run_book,
            &*self.transfers,
            &self.pause_book,
            &LocalClock,
            &self.chimes,
        )
        .run();
        for (id, e) in resumed.failed {
            warn!(download_id = id, error = %e, "could not resume");
        }
    }

    /// Handle one note: scan a finished download first when asked, then play
    /// the sound and notify.
    fn on_note(self: &Arc<Self>, note: Note) {
        let settings = self.settings.lock().clone();
        match note {
            Note::Finished(id) if settings.scan_after_download => {
                let scanning = self.store.update(id, |e| {
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
                    info!(download_id = id, result = ?result, "scanned");
                    let _ = daemon.store.update(id, |e| {
                        e.scan = Some(result);
                        Ok(())
                    });
                    let _ = daemon.clients.notes.send(Note::Scanned(id));
                });
            }
            Note::Finished(id) | Note::Scanned(id) => {
                self.notifier.notify(id, false, &settings);
                self.after_download(id);
            }
            Note::Failed(id) => {
                thread::sleep(FAILED_GRACE);
                if self.entry(id).is_some_and(|e| e.status == Status::Failed)
                    && !self.retry_through_browser_proxy(id, &settings)
                {
                    self.notifier.notify(id, true, &settings);
                }
            }
            // Without a window to ask, a taken name gets a new one.
            Note::NameTaken(id) => self.answers.wait(id, Some(FileExists::Rename)),
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
            let choice = download.duplicate.unwrap_or(self.settings.lock().if_duplicate);
            info!(url = %download.url, download_id = old.id, choice = ?choice, "already in the list");
            if choice == Duplicate::Ask {
                return self.ask(download, Some(old.id));
            }
            if let Some(kept) = self.settle_duplicate(&mut download, &old, choice) {
                return kept;
            }
        }
        let settings = self.settings.lock().clone();
        if download.start == Some(false) || (download.start.is_none() && settings.add_only) {
            return self.add_later(download);
        }
        let name = engine::Inspection::unprobed(&download.url, download.filename.as_deref()).filename;
        // A folder UIDM may not write to asks first, so the dialog can offer
        // administrator rights or another folder.
        let folder = match download.folder.as_deref().map(Path::new).filter(|f| f.is_absolute()) {
            Some(f) => f.to_path_buf(),
            None => settings
                .folders_for(&download.url, &uidm_infrastructure::settings::default_downloads_dir())
                .for_name(&name),
        };
        let denied = !download.elevate && !uidm_infrastructure::elevate::can_write(&folder);
        if denied || download.ask.unwrap_or(settings.ask_where_to_save && !settings.starts_without_asking(&name)) {
            self.ask(download, None)
        } else {
            self.start(download, None)
        }
    }

    /// The newest download in the list with `download`'s address, unless it
    /// cannot be a duplicate (a form post or a stream).
    fn duplicate_of(&self, download: &Download) -> Option<DownloadEntry> {
        if !download.may_be_duplicate() {
            return None;
        }
        self.store.find_last(|e| uidm_domain::download::same_address(&e.url, &download.url))
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
        match download.settle_duplicate(old, choice) {
            DuplicateStep::Add => None,
            DuplicateStep::UseExisting => self.use_existing(old),
            DuplicateStep::Replace => {
                if let Err(e) = self.remove(old.id, false) {
                    warn!(download_id = old.id, error = %e, "could not remove");
                }
                None
            }
        }
    }

    /// Point the user at download `old` instead of adding its address again
    /// (see `UseExisting`). `None` when the finished file is gone, so the new
    /// download should be added after all.
    fn use_existing(self: &Arc<Self>, old: &DownloadEntry) -> Option<(u64, String)> {
        match UseExisting::new(&*self.store, &*self.store, &*self.transfers, &DiskFiles, &*self.front).run(old) {
            Existing::Gone => None,
            Existing::Shown { id, url } | Existing::Resumed { id, url } => Some((id, url)),
            Existing::NotResumed { id, url, error } => {
                warn!(download_id = id, error = %error, "could not resume");
                Some((id, url))
            }
        }
    }

    /// Add `download` to a queue (the one it names, else the one Download
    /// later goes to, else the main one) without starting it or opening a
    /// connection.
    fn add_later(&self, download: Download) -> (u64, String) {
        let later = self.settings.lock().later_queue;
        AddLater::new(&*self.store, &self.intake()).run(download, later)
    }

    /// The adapter new downloads go into the list through: waiting, in no
    /// queue yet.
    fn intake(&self) -> Intake<'_> {
        Intake { store: &self.store, engine: &self.engine }
    }

    /// Add `items` to wait in `queue` (see `AddMany`). Returns how many were
    /// added and how many were skipped as duplicates.
    fn add_many(self: &Arc<Self>, items: Vec<Download>, queue: Option<u64>) -> (usize, usize) {
        let (ids, skipped) = self.add_many_ids(items, queue);
        (ids.len(), skipped)
    }

    /// `add_many`, answering with the new downloads' ids.
    fn add_many_ids(self: &Arc<Self>, items: Vec<Download>, queue: Option<u64>) -> (Vec<u64>, usize) {
        let choice = self.settings.lock().if_duplicate;
        let added = AddMany::new(&*self.store, &*self.store, &*self.transfers, &DiskFiles, &self.intake())
            .run(items, queue, choice);
        for (id, e) in added.not_replaced {
            warn!(download_id = id, error = %e, "could not remove");
        }
        (added.ids, added.skipped)
    }

    /// Read a stream's playlist on another thread and answer `reply` with
    /// what it offers.
    fn probe_media(&self, request: u64, download: Download, reply: Sender<Vec<u8>>) {
        let req = self.transfers.request_for(download);
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
            info!(client, url = %req.start, "crawl");
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
            info!(client, pages = out.pages, files = out.files.len(), "crawl done");
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
        let settings = self.settings.lock().clone();
        let items = files.iter().filter_map(|f| grab::download_for(req, f, &settings, login)).collect();
        let (added, _) = self.add_many_ids(items, Some(queue));
        if first {
            let at_once = req.download_at_once as usize;
            let _ = self.store.update_queue(queue, |q| {
                q.max_concurrent = at_once;
                Ok(())
            });
        }
        if !added.is_empty() && !self.store.queues().iter().any(|q| q.id == queue && q.running) {
            if let Err(e) = self.start_queue(queue, None) {
                warn!(error = %e, "could not start the grabber's queue");
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
        let settings = self.settings.lock().clone();
        let start = Download { url: req.start.clone(), ..Download::default() };
        let mut net = self.transfers.net_for(&start, &settings);
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
        let render = AtomicBool::new(req.render_scripts && self.clients.extension_connected());
        let fetch = |url: &str| {
            // A page as the browser shows it, when asked and the browser
            // can; otherwise as the server sends it.
            let rendering = render.load(Ordering::Relaxed) && !url.ends_with("/robots.txt");
            let rendered = rendering.then(|| self.render_in_browser(url)).flatten();
            // A browser that does not answer once is not asked again.
            if rendering && rendered.is_none() {
                info!(url = %url, "the browser did not render the page; reading pages as sent");
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
        self.run_grab_schedules(uidm_infrastructure::clock::local_now());
    }

    /// Start the saved grabber projects whose scheduled run is due, each on
    /// its own thread. Runs with the scheduler.
    fn run_grab_schedules(self: &Arc<Self>, now: queue::Moment) {
        let Ok(dir) = grab::projects_dir() else {
            return;
        };
        let runs = {
            let (daemon, dir) = (Arc::clone(self), dir.clone());
            grab::ScheduledGrabs::new(
                Arc::clone(&self.grab_runs),
                Arc::new(move |project, day| daemon.run_scheduled_grab(&dir, project, day)),
            )
        };
        for started in RunGrabSchedules::new(&grab::ProjectFiles(dir), &runs, &StoppedClock(now)).run() {
            if let Some(e) = started.not_saved {
                warn!(project = %started.project, error = %e, "could not save the grabber project");
            }
        }
    }

    /// One scheduled run of a grabber project, as its `run` says: crawl
    /// until done or until the schedule's stop time and keep what it found,
    /// then maybe add the new finds to the project's queue and start it; or
    /// only add the ticked files and start the queue. The stop time ends the
    /// queue's run too, and what the project asks when done (with its own
    /// force-close choice) happens after the crawl, or once the queue is done.
    fn run_scheduled_grab(self: &Arc<Self>, dir: &Path, mut project: grab::Project, day: i64) {
        info!(project = %project.name, run = ?project.run, "schedule runs a grabber project");
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
                    if end.is_some_and(|end| uidm_infrastructure::clock::local_now() >= end) {
                        stop.store(true, Ordering::Relaxed);
                    }
                });
                resync = project.to_resync(&out.files);
                let new = project.record_run(out.files);
                info!(project = %project.name, files = new.len(), "grabber project found new files");
                if run == grab::RunAction::ExploreAndDownload {
                    new
                } else {
                    Vec::new()
                }
            }
        };
        if let Err(e) = grab::save_project(&project, dir) {
            warn!(project = %project.name, error = %e, "could not save the grabber project");
        }
        // Set before the queue starts, so a run that ends at once still
        // does it. A queue already running is a run someone else started,
        // such as the user: the project's files join it, but its stop time
        // and when-done action stay that run's own.
        let mut owns_run = false;
        let _ = self.store.update_queue(project.queue, |q| {
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
        info!(project = %project.name, added, changed, "grabber project queued files");
        let added = added + changed;
        if added == 0 {
            // Nothing to wait for: done now.
            if owns_run {
                let _ = self.store.update_queue(project.queue, |q| {
                    q.when_done_once = None;
                    q.open_when_done_once = None;
                    q.force_close_once = None;
                    q.stop_at_once = None;
                    Ok(())
                });
            }
            if let Some(file) = &project.open_when_done {
                power_control::open_after_run(file);
            }
            self.when_done(project.when_done, &project.name, None, project.force_close);
        }
    }
    /// Convert finished download `id` on another thread (see
    /// `ConvertDownload`). The new file is a row of its own: added and
    /// started now, with ffmpeg's progress, done with a note, or failed with
    /// ffmpeg's complaint.
    fn convert(
        self: &Arc<Self>,
        id: u64,
        to: uidm_infrastructure::media::convert::Target,
        delete_original: bool,
    ) -> Result<(), String> {
        let output = ConvertDownload::new(&*self.store, &DiskFiles, &self.conversions)
            .run(id, to, delete_original)
            .map_err(|e| e.to_string())?;
        info!(download_id = id, output = %output.display(), "converting");
        Ok(())
    }

    /// Ask GitHub (at `url`) for the latest release, through the settings'
    /// proxy, and note when that happened.
    fn check_update(&self, url: &str) -> Outgoing {
        let github = GitHub::new(Arc::clone(&self.settings), Arc::clone(&self.browser_proxy_retries), url);
        match CheckForUpdate::new(&github, &*self.settings, VERSION).run(uidm_infrastructure::state::unix_now()) {
            Ok(newer) => Outgoing::Update { newer, error: None },
            Err(e) => Outgoing::Update { newer: None, error: Some(e.to_string()) },
        }
    }

    /// Update now (see `InstallUpdate`), then stop the downloads, save the
    /// list and quit with the windows, so Setup can replace the files.
    /// Setup closes whatever is left.
    fn install_update(&self) -> Result<(), String> {
        let github = GitHub::new(
            Arc::clone(&self.settings),
            Arc::clone(&self.browser_proxy_retries),
            uidm_infrastructure::update::LATEST_RELEASE,
        );
        let path = InstallUpdate::new(&github, &*self.settings, VERSION)
            .run(uidm_infrastructure::state::unix_now())
            .map_err(|e| e.to_string())?;
        info!(path = %path.display(), "started the update; quitting for it");
        self.pause_all();
        self.transfers.save_history();
        self.clients.broadcast(&Outgoing::Quit);
        Ok(())
    }

    /// Once a week, when a window connects, look for a newer release on
    /// another thread and tell the windows if there is one.
    fn weekly_update_check(self: &Arc<Self>) {
        if !ClaimWeeklyCheck::new(&*self.settings).run(uidm_infrastructure::state::unix_now()) {
            return;
        }
        let daemon = Arc::clone(self);
        thread::spawn(move || {
            if let msg @ Outgoing::Update { newer: Some(_), .. } =
                daemon.check_update(uidm_infrastructure::update::LATEST_RELEASE)
            {
                daemon.clients.broadcast(&msg);
            }
        });
    }

    /// List the files inside zip download `id` on another thread, reading
    /// the end of the archive with its own request (cookies and all), and
    /// answer `reply` with `zip_preview`.
    fn preview_zip(&self, id: u64, reply: Sender<Vec<u8>>) {
        let entry = self.entry(id);
        let download = self.store.requests().get(&id).cloned();
        let answer = move |entries: Vec<uidm_infrastructure::zip::ZipEntry>, error: Option<String>| {
            if let Ok(frame) = serde_json::to_vec(&Outgoing::ZipPreview { id, entries, error }) {
                let _ = reply.send(frame);
            }
        };
        let Some(entry) = entry else {
            return answer(Vec::new(), Some(format!("no download with id {id}")));
        };
        let download = download.unwrap_or_else(|| Download { url: entry.url.clone(), ..Download::default() });
        let req = self.transfers.request_for(download);
        thread::spawn(move || {
            let total = match entry.total {
                Some(t) => Ok(t),
                None => engine::inspect(&req.url, None, &req.headers, &req.net)
                    .map_err(|e| e.to_string())
                    .and_then(|i| i.total.ok_or_else(|| "The server does not say how big the file is.".to_string())),
            };
            let listed = total.and_then(|total| {
                uidm_infrastructure::zip::list(total, |a, b| {
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
        self.clients.broadcast(&Outgoing::RenderPage { request, url: url.to_string() });
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
        if !self.clients.extension_connected() {
            return None;
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let request = NEXT.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.cookie_waits.lock().unwrap().insert(request, tx);
        self.clients.broadcast(&Outgoing::CookiesWanted { request, url: url.to_string() });
        let got = rx.recv_timeout(Duration::from_secs(5)).ok().flatten();
        self.cookie_waits.lock().unwrap().remove(&request);
        info!(found = got.is_some(), "browser cookies for the crawl");
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
        let reqs: Arc<Vec<DownloadRequest>> =
            Arc::new(links.into_iter().map(|d| self.transfers.request_for(d)).collect());
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
        self.store.requests().insert(id, download.clone());
        self.store.publish(&Outgoing::Added { id, url: url.clone() });

        let daemon = Arc::clone(self);
        thread::spawn(move || {
            let req = daemon.transfers.request_for(download.clone());
            // Probing a POST download would send a GET (or repeat the POST),
            // so its dialog proposes a name from the request alone.
            // What the browser's response said is all there is to show then.
            let probed = if uidm_infrastructure::app::post_body(&download).is_some() {
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
                    daemon.store.publish(&Outgoing::Failed { id, url: download.url, error: e.to_string() });
                    return;
                }
            };
            let path = req.folders.for_name(&found.filename).join(&found.filename);
            let probed = Probed { filename: found.filename, path, total: found.total, resumable: found.resumable };
            let hold = HoldForAnswer::new(
                &*daemon.store,
                &*daemon.transfers,
                &DiskFiles,
                &*daemon.transfers,
                &*daemon.early,
                &daemon.answers,
            );
            if let Held::Started(Err(e)) = hold.run(id, &download, probed, duplicate_of) {
                warn!(download_id = id, error = %e, "could not start");
            }
        });
        (id, url)
    }

    /// Answer the save-as dialog of a `confirming` download, or start a
    /// `waiting` one. Missing names and folders keep the proposed ones. With
    /// `start` false it waits in `queue` (the main queue by default).
    /// `file_exists`, when given, decides what happens if the name is taken,
    /// now and on later starts. For a download asked about as a duplicate,
    /// `duplicate` decides what happens to the other download first (a new
    /// name when missing). A category and description, when given, are kept
    /// with it. The dialog's "remember" boxes change the settings.
    fn confirm(&self, answer: Answer) -> Result<(), String> {
        confirm_answer(&self.store, &self.transfers, &self.front, &self.early, answer)
    }

    /// Cancel a download, whether it waits for its dialog or is in the engine.
    fn cancel(self: &Arc<Self>, id: u64) {
        CancelDownload::new(&*self.store, &*self.transfers, &self.conversions).run(id);
    }

    /// Queue `download`, remember it for retries, and announce it.
    /// `reuse_path` is a partial file an earlier attempt left behind.
    fn start(&self, download: Download, reuse_path: Option<PathBuf>) -> (u64, String) {
        self.transfers.start_resuming(download, reuse_path, None)
    }

    /// Re-queue a failed, canceled or interrupted download from its stored
    /// request, or from its URL alone when the request was not kept (see
    /// `RetryDownload`).
    fn retry(&self, id: u64) -> Result<(u64, String), String> {
        RetryDownload::new(&*self.store, &*self.store, &*self.transfers).run(id).map_err(|e| e.to_string())
    }

    /// Set or clear download `id`'s own speed limit (see `SetSpeedLimit`).
    fn set_download_limit(&self, id: u64, bytes_per_sec: Option<u64>) -> Result<(), String> {
        SetSpeedLimit::new(&*self.store, &*self.transfers).run(id, bytes_per_sec).map_err(|e| e.to_string())
    }

    /// Set whether download `id` keeps its own speed limit when it is
    /// stopped and resumed.
    fn set_limit_remember(&self, id: u64, remember: bool) -> Result<(), String> {
        SetLimitRemember::new(&*self.store).run(id, remember).map_err(|e| e.to_string())
    }

    /// Start download `id` again from the first byte, under the same id (see
    /// `Redownload`).
    fn redownload(&self, id: u64) -> Result<(), String> {
        let url = Redownload::new(&*self.store, &*self.transfers, &DiskFiles).run(id).map_err(|e| e.to_string())?;
        info!(download_id = id, url = %url, "downloading again");
        Ok(())
    }

    /// Give download `id` a new link, or wait for the browser's next one (see
    /// `RefreshLink`). Returns why the new link was refused, if it was.
    fn refresh_link(&self, id: u64, download: Option<Download>, restart: bool) -> Result<Option<String>, String> {
        match RefreshLink::new(&*self.store, &*self.store, &*self.transfers)
            .run(id, download, restart)
            .map_err(|e| e.to_string())?
        {
            Refreshed::Waiting => Ok(None),
            Refreshed::Mismatch(reason) => Ok(Some(reason)),
            Refreshed::NewLink(url) => {
                info!(download_id = id, url = %url, "new link");
                Ok(None)
            }
        }
    }

    /// Change a stopped or finished download's details (see `SetProperties`).
    fn set_properties(&self, id: u64, p: Properties) -> Result<(), String> {
        let moved = SetProperties::new(&*self.store, &DiskFiles).run(id, p).map_err(|e| e.to_string())?;
        if let Some(to) = moved {
            info!(download_id = id, to = %to.display(), "moved");
        }
        Ok(())
    }

    /// Under "the browser's proxy only after download errors", retry failed
    /// download `id` once, through the proxy its browser reported. Returns
    /// whether it did.
    fn retry_through_browser_proxy(&self, id: u64, s: &Settings) -> bool {
        if !(s.use_browser_proxy && s.browser_proxy_after_errors) {
            return false;
        }
        let url = self
            .store
            .requests()
            .get(&id)
            .filter(|d| d.proxy.is_some() || d.browser_proxy.is_some())
            .map(|d| d.url.clone());
        let Some(url) = url else {
            return false;
        };
        if !self.browser_proxy_retries.insert(url) {
            return false;
        }
        match self.retry(id) {
            Ok((new_id, _)) => {
                info!(download_id = id, new_id, "failed; retrying through the browser's proxy");
                true
            }
            Err(e) => {
                warn!(download_id = id, error = %e, "could not retry through the browser's proxy");
                false
            }
        }
    }

    /// The settings as they are, with only the video button's fields taken
    /// from `asked`. The browser extension changes nothing else, and only
    /// the window changes settings at large: a browser connection, from an
    /// extension that is not UIDM's or a bug in it, must not point the virus
    /// scanner, ffmpeg, folders or proxy somewhere else.
    fn with_video_button(&self, asked: &Settings) -> Settings {
        let mut settings = self.settings.lock().clone();
        settings.video_button = asked.video_button;
        settings.video_button_excluded_sites = asked.video_button_excluded_sites.clone();
        settings
    }

    /// Queue every interrupted download again (the "resume at startup"
    /// option).
    fn resume_interrupted(&self) {
        let ids: Vec<u64> =
            self.store.table().entries().filter(|e| e.status == Status::Interrupted).map(|e| e.id).collect();
        for id in ids {
            match self.retry(id) {
                Ok((new_id, url)) => info!(download_id = id, new_id, url = %url, "resuming"),
                Err(e) => warn!(download_id = id, error = %e, "could not resume"),
            }
        }
    }

    /// Forget a download that is no longer running. Its files stay on disk
    /// unless `delete_files`, which sends them to the Recycle Bin.
    fn remove(&self, id: u64, delete_files: bool) -> Result<(), String> {
        if delete_files {
            info!(download_id = id, "removing with its files");
        }
        RemoveDownload::new(&*self.store, &*self.store, &*self.transfers, &DiskFiles)
            .run(id, delete_files)
            .map_err(|e| e.to_string())
    }

    /// Send the whole list to one client, under the table lock so no event can
    /// slip between the snapshot and the client's next frame.
    fn send_list(&self, reply: &Sender<Vec<u8>>) {
        self.store.snapshot(|items, queues| {
            let refresh = Outgoing::RefreshWaiting { id: self.transfers.waiting_for_link() };
            for msg in [Outgoing::Downloads { items }, Outgoing::Queues { items: queues }, refresh] {
                if let Ok(frame) = serde_json::to_vec(&msg) {
                    let _ = reply.send(frame);
                }
            }
        })
    }
}

/// Do what a finished queue, or with `file` one finished download, asks
/// (see `RunWhenDone`).
fn when_done(power: &PowerOff, action: WhenDone, queue: &str, file: Option<&str>, force_close: bool) {
    let request = PowerRequest { action, queue: queue.to_string(), file: file.map(str::to_string), force_close };
    match RunWhenDone::new(power).run(&request) {
        PowerStep::None => {}
        PowerStep::Exit => info!(subject = %request.subject(), "done: exiting"),
        PowerStep::Countdown => {
            info!(subject = %request.subject(), action = action.label(), seconds = COUNTDOWN_SECS, "done: countdown")
        }
    }
}

/// Answer a save-as dialog (see `ConfirmAnswer`), logging what could not
/// be done alongside.
fn confirm_answer(
    store: &Arc<DownloadStore>,
    transfers: &Arc<Transfers>,
    front: &Front,
    early: &EarlyHandOver,
    answer: Answer,
) -> Result<(), String> {
    let id = answer.id;
    let confirm = ConfirmAnswer::new(&**store, &**store, &**transfers, &DiskFiles, front, &**transfers, early);
    let (done, warnings) = confirm.run(answer).map_err(|e| e.to_string())?;
    for w in warnings {
        match w {
            ConfirmWarning::DuplicateNotRemembered(e) => {
                warn!(error = %e, "could not remember the duplicate choice")
            }
            ConfirmWarning::ChoicesNotRemembered(e) => {
                warn!(error = %e, "could not remember the dialog's choices")
            }
            ConfirmWarning::OldNotRemoved(old, e) => warn!(download_id = old, error = %e, "could not remove"),
            ConfirmWarning::OldNotResumed(old, e) => warn!(download_id = old, error = %e, "could not resume"),
        }
    }
    if let Confirmed::UsedExisting { old, kept } = done {
        info!(download_id = id, duplicates = old, kept, "duplicate");
    }
    Ok(())
}

fn main() {
    logging::init(logging::Program::Daemon);
    // Started by a download saved with administrator rights (see
    // `uidm_infrastructure::elevate`): move its finished file into place and exit.
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, from, to] = args.as_slice() {
        if flag == uidm_infrastructure::elevate::MOVE_ARG {
            if let Err(e) = uidm_infrastructure::elevate::move_file(Path::new(from), Path::new(to)) {
                error!(from = %from, to = %to, error = %e, "could not move the file");
                std::process::exit(1);
            }
            return;
        }
    }
    if let Err(e) = run() {
        error!(error = %e, "fatal");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    // Binding is the single-instance lock.
    let addr = ipc::daemon_addr();
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            info!(addr = %addr, error = %e, "address unavailable; another daemon is likely running");
            return Ok(());
        }
    };
    // Only the daemon holding the bind moves the logs aside, so no two
    // programs move one at once.
    logging::trim();
    info!(addr = %addr, version = VERSION, "listening");

    // Publish the token only after winning the bind. A daemon that lost the
    // bind must not overwrite the running daemon's token.
    let token = ipc::new_token()?;
    ipc::write_token(&token)?;
    let token: Arc<str> = token.into();
    match uidm_infrastructure::browsers::register_found() {
        Ok(names) if !names.is_empty() => info!(browsers = %names.join(", "), "registered with the browsers"),
        Ok(_) => {}
        Err(e) => warn!(error = %e, "could not register with the browsers"),
    }

    let (daemon, dirty, notes, wake) = Daemon::new(Paths::from_env());
    transfers::clean_early_folders(&daemon.store);
    let daemon = Arc::new(daemon);
    spawn_scheduler(Arc::clone(&daemon), wake);
    spawn_history_writer(Arc::clone(&daemon), dirty);
    spawn_notifier(Arc::clone(&daemon), notes);
    #[cfg(windows)]
    if let Err(e) = register_notifications() {
        warn!(error = %e, "could not register for notifications");
    }
    // Queues set to start with UIDM.
    let launch: Vec<u64> = daemon.store.queues().iter().filter(|q| q.start_on_launch).map(|q| q.id).collect();
    for id in launch {
        if let Err(e) = daemon.start_queue(id, None) {
            warn!(queue = %id, error = %e, "could not start the queue");
        }
    }
    if daemon.settings.lock().resume_at_startup {
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
            Err(e) => warn!(error = %e, "accept failed"),
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
        warn!(client = id, peer = %peer, error = %e, "client rejected");
        return;
    }
    info!(client = id, peer = %peer, "client connected");

    let mut writer_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            warn!(client = id, error = %e, "client error");
            return;
        }
    };
    let (tx, rx) = channel::<Vec<u8>>();
    daemon.clients.clients.lock().unwrap().insert(id, tx.clone());

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
                warn!(client = id, error = %e, "client read error");
                break;
            }
        }
    }

    // Removing the map's sender and dropping ours leaves the writer's channel
    // with no senders, so it drains what is queued and ends. Downloads this
    // client started keep running.
    daemon.clients.client_gone(id);
    daemon.stop_grab(id);
    daemon.stop_probe_links(id);
    drop(tx);
    let _ = writer.join();
    info!(client = id, "client disconnected");
}

/// Run the daemon side of the handshake described in `uidm_infrastructure::ipc`.
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
    if daemon.clients.browsers.lock().unwrap().contains(&client) {
        *daemon.clients.browser_seen.lock().unwrap() = Some(uidm_infrastructure::state::unix_now());
    }
    match incoming {
        Incoming::Hello { client: kind, .. } => {
            if kind.is_none() {
                daemon.clients.browser_connected(client);
            }
            if kind.as_deref() == Some(WINDOW_CLIENT) {
                daemon.clients.windows().insert(client);
                send(&daemon.clients.browser_link());
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
            let window_open = daemon.clients.window_open();
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
            info!(download_id = id, url = %url, "add");
            send(&Outgoing::Ack { id, url, status: "accepted".to_string() });
        }
        Incoming::AddMany { items, queue } => {
            let (count, skipped) = daemon.add_many(items, queue);
            info!(count, skipped, "added downloads to wait");
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
            info!(download_id = id, "cancel");
            daemon.cancel(id);
        }
        Incoming::Confirm(answer) => {
            let (id, start) = (answer.id, answer.start);
            match daemon.confirm(answer) {
                Ok(()) => info!(download_id = id, start, "confirm"),
                Err(message) => send(&Outgoing::Error { message }),
            }
        }
        Incoming::Retry { id } => match daemon.retry(id) {
            Ok((new_id, url)) => {
                info!(download_id = id, new_id, "retry");
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
            Ok(id) => info!(queue = %id, "queue saved"),
            Err(message) => send(&Outgoing::Error { message }),
        },
        Incoming::DeleteQueue { id } => {
            if let Err(message) = daemon.delete_queue(id) {
                send(&Outgoing::Error { message });
            }
        }
        Incoming::StartQueue { id } => match daemon.start_queue(id, None) {
            Ok(()) => {
                info!(queue = %id, "start queue");
                let _ = daemon.clients.wake.send(());
            }
            Err(message) => send(&Outgoing::Error { message }),
        },
        Incoming::StopQueue { id } => match daemon.stop_queue(id) {
            Ok(()) => info!(queue = %id, "stop queue"),
            Err(message) => send(&Outgoing::Error { message }),
        },
        Incoming::SetQueueOnce { id, when_done } => {
            if let Err(message) = daemon.store.update_queue(id, |q| {
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
        Incoming::CancelRefresh => daemon.transfers.set_refresh_waiting(None),
        Incoming::PickLinks { page, mut links, user_agent } => {
            links.retain(|l| uidm_domain::batch::is_web_url(&l.url));
            links.truncate(uidm_domain::batch::MAX_BATCH);
            info!(links = links.len(), page = %page, "links to pick from");
            let msg = Outgoing::PickLinks { page, links, user_agent };
            if daemon.clients.window_open() {
                daemon.clients.broadcast(&msg);
                daemon.clients.broadcast(&Outgoing::Show);
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
                let msg = daemon.check_update(uidm_infrastructure::update::LATEST_RELEASE);
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
            info!("pause all");
            daemon.pause_all();
        }
        Incoming::ResumeAll => {
            info!("resume all");
            daemon.resume_all();
        }
        Incoming::GetSettings => send(&Outgoing::Settings { settings: daemon.settings.lock().clone() }),
        Incoming::SetSettings { settings } => {
            match daemon.transfers.set_settings(if daemon.clients.windows().contains(&client) {
                settings
            } else {
                daemon.with_video_button(&settings)
            }) {
                Ok(()) => info!("settings changed"),
                Err(message) => send(&Outgoing::Error { message }),
            }
        }
    }
}

/// Run the queue scheduler: a pass whenever something wakes it (a download
/// ended, a queue changed) and every few seconds for the clock.
fn spawn_scheduler(daemon: Arc<Daemon>, wake: Receiver<()>) {
    thread::spawn(move || {
        // Wakes the computer for the next scheduled start; kept here so the
        // handle lives as long as the scheduler.
        let mut timer = power::WakeTimer::new();
        // The daemon trimmed the logs as it won the bind; this keeps them
        // bounded while it runs for days.
        let mut trimmed = Instant::now();
        loop {
            match wake.recv_timeout(SCHEDULER_POLL) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    while wake.try_recv().is_ok() {}
                    daemon.tick();
                    if trimmed.elapsed() >= LOG_TRIM_EVERY {
                        logging::trim();
                        trimmed = Instant::now();
                    }
                    daemon.check_grab_schedules();
                    let next = daemon.next_wake(uidm_infrastructure::clock::local_now());
                    if next != timer.at() {
                        match timer.set(next) {
                            Ok(()) => match next {
                                Some(at) => info!(at = %at.display(), "wake timer set"),
                                None => info!("wake timer cleared"),
                            },
                            Err(e) => warn!(error = %e, "could not set the wake timer"),
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
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
        .and_then(|()| uidm_infrastructure::settings::write_atomic(&here, out.as_bytes()));
    if let Err(e) = written {
        warn!(path = %here.display(), error = %e, "could not save");
    }
}

/// Write the history shortly after it changes, batching bursts of events
/// (progress, a cleared list) into one write.
fn spawn_history_writer(daemon: Arc<Daemon>, dirty: Receiver<()>) {
    thread::spawn(move || {
        while dirty.recv().is_ok() {
            thread::sleep(HISTORY_DEBOUNCE);
            while dirty.try_recv().is_ok() {}
            daemon.transfers.save_history();
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
            let want = daemon.settings.lock().keep_awake && daemon.is_working();
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
            let idle = daemon.clients.clients.lock().unwrap().is_empty()
                && daemon.engine.active_count() == 0
                && !daemon.queues_need_daemon();
            match (idle, idle_since) {
                (true, None) => idle_since = Some(Instant::now()),
                (true, Some(t)) if t.elapsed() >= IDLE_EXIT => {
                    info!(seconds = IDLE_EXIT.as_secs(), "idle, exiting");
                    daemon.transfers.save_history();
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
    use uidm_domain::download::DownloadLogin;
    use uidm_domain::ports::QuotaState;
    use uidm_domain::quota::Usage;

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
        let (daemon, ..) = Daemon::new(Paths { settings: None, store: StoreFiles::default() });
        {
            let mut s = daemon.settings.lock();
            s.download_dir = Some(dir.display().to_string());
            s.ask_where_to_save = false;
            s.if_duplicate = choice;
        }
        let (tx, window) = channel();
        daemon.clients.clients.lock().unwrap().insert(1, tx);
        daemon.clients.windows.lock().unwrap().insert(1);
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
            self.daemon.store.publish(&Outgoing::Added { id, url: url.clone() });
            self.daemon.store.publish(&Outgoing::Done { id, url, path: path.display().to_string(), note: None });
            id
        }

        fn failed(&self, url: &str) -> u64 {
            let id = self.daemon.engine.reserve_id();
            let url = url.to_string();
            self.daemon.store.publish(&Outgoing::Added { id, url: url.clone() });
            self.daemon.store.publish(&Outgoing::Failed { id, url, error: "gone".into() });
            id
        }

        fn add(&self, url: &str) -> u64 {
            self.daemon.add(Download { url: url.into(), ..Download::default() }).0
        }

        fn len(&self) -> usize {
            self.daemon.store.table().len()
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
        assert!(matches!(t.daemon.clients.browser_link(), Outgoing::BrowserLink { connected: false, last_seen: None }));

        // The extension says hello with no `client`.
        let (tx, _browser) = channel();
        t.daemon.clients.clients.lock().unwrap().insert(2, tx.clone());
        dispatch(br#"{"op":"hello","version":"1"}"#, &t.daemon, 2, &tx);
        assert!(t.window_got(|m| matches!(m, Outgoing::BrowserLink { connected: true, .. })));
        assert!(t.daemon.clients.browser_seen.lock().unwrap().is_some());

        // A second one is no news; the first going is none either.
        let (tx3, _browser3) = channel();
        t.daemon.clients.clients.lock().unwrap().insert(3, tx3.clone());
        dispatch(br#"{"op":"hello"}"#, &t.daemon, 3, &tx3);
        t.daemon.clients.client_gone(2);
        assert!(!t.window_got(|m| matches!(m, Outgoing::BrowserLink { .. })));

        // Anything it sends keeps it seen.
        *t.daemon.clients.browser_seen.lock().unwrap() = Some(1);
        dispatch(br#"{"op":"ping"}"#, &t.daemon, 3, &tx3);
        assert!(t.daemon.clients.browser_seen.lock().unwrap().is_some_and(|s| s > 1));

        // The last one going is, with when.
        t.daemon.clients.client_gone(3);
        assert!(t.window_got(|m| matches!(m, Outgoing::BrowserLink { connected: false, last_seen: Some(s) } if *s > 1)));
    }

    #[test]
    fn a_command_line_runs_in_the_open_window_or_waits_for_the_new_one() {
        let t = setup("command_line", Duplicate::NewName);
        let entry = |url: &str| t.daemon.store.table().entries().find(|e| e.url == url).cloned();
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
        t.daemon.clients.windows.lock().unwrap().clear();
        let (reply, _) = ask("https://example.com/new.bin");
        assert!(matches!(reply, Outgoing::CommandLine { window_open: false }));
        assert!(entry("https://example.com/new.bin").is_none(), "held for the window");
        let (tx, _rx) = channel();
        dispatch(br#"{"op":"hello","client":"window"}"#, &t.daemon, 1, &tx);
        let e = entry("https://example.com/new.bin").expect("added once the window says hello");
        assert_eq!(e.on_done.map(|o| o.then), Some(WhenDone::Exit));

        // One held too long belonged to a window that never came.
        t.daemon.clients.windows.lock().unwrap().clear();
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
        t.daemon.store.publish(&Outgoing::Added { id: old, url: url.clone() });
        assert_eq!(t.daemon.transfers.status(old), Some(Status::Queued));
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
        let req = t.daemon.store.requests().get(&id).cloned().unwrap();
        assert_eq!(req.url, "https://y/new.bin");
        assert_eq!(req.referrer.as_deref(), Some("https://y/page"));

        // A new user name without a password keeps the old password.
        let rename = Properties {
            login: Some(DownloadLogin { username: "other".into(), password: None }),
            ..Properties::default()
        };
        t.daemon.set_properties(id, rename).unwrap();
        let req = t.daemon.store.requests().get(&id).cloned().unwrap();
        let login = req.login.clone().unwrap();
        assert_eq!((login.username.as_str(), login.password.as_deref()), ("other", Some("pw")));
        let net = t.daemon.transfers.net_for(&req, &Settings::default());
        let found = net::find_login(&net.logins, "https://y/new.bin").unwrap();
        assert_eq!(found.username, "other");

        // An empty user name removes the login.
        let clear = Properties { login: Some(DownloadLogin::default()), ..Properties::default() };
        t.daemon.set_properties(id, clear).unwrap();
        assert_eq!(t.daemon.entry(id).unwrap().login_user, None);
        assert!(t.daemon.store.requests()[&id].login.is_none());
    }

    /// A stopped download moved to another folder takes its partial file
    /// and sidecar along, and resumes and finishes there without fetching
    /// its bytes again.
    #[test]
    fn properties_move_a_stopped_download_with_its_partial_file() {
        const LEN: usize = 1024 * 1024;
        let t = setup("props_move_partial", Duplicate::NewName);
        t.daemon.settings.lock().connections = 2;
        let body: Arc<Vec<u8>> = Arc::new((0..LEN).map(|i| (i % 251) as u8).collect());
        let sent = Arc::new(AtomicU64::new(0));
        let id = t.add(&serve_ranges(Arc::clone(&body), Arc::clone(&sent)));
        let until = Instant::now() + Duration::from_secs(30);
        while t.daemon.transfers.status(id) != Some(Status::Downloading)
            || sent.load(Ordering::Relaxed) < LEN as u64 / 4
        {
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
        let req = t.daemon.store.requests().get(&id).cloned().unwrap();
        assert_eq!(req.folder.map(PathBuf::from), Some(elsewhere.clone()), "the next attempt saves there");

        // A proposed file that was never started may be somebody else's: it
        // stays where it is.
        let other = t.dir.join("theirs.bin");
        std::fs::write(&other, b"theirs").unwrap();
        t.daemon
            .store
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
        let through = |s: &Settings| t.daemon.transfers.net_for(&d, s).proxy.and_then(|p| p.proxy_for(&d.url));
        let after = Settings { use_browser_proxy: true, browser_proxy_after_errors: true, ..Settings::default() };
        assert_eq!(through(&after), None, "not before it failed");
        t.daemon.browser_proxy_retries.insert(d.url.clone());
        assert_eq!(through(&after).as_deref(), Some("http://bp:3128"), "once it failed");
        // A download that never failed is not retried through it.
        assert!(!t.daemon.retry_through_browser_proxy(999, &after));
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
        assert!(!t.daemon.settings.lock().capture_private, "off by default");
        let asked = Settings { capture_private: true, ..t.daemon.settings.lock().clone() };
        // A browser client's set_settings keeps the setting as it was...
        t.daemon.transfers.set_settings(t.daemon.with_video_button(&asked)).unwrap();
        assert!(!t.daemon.settings.lock().capture_private);
        // ...while the window's goes through.
        t.daemon.transfers.set_settings(asked).unwrap();
        assert!(t.daemon.settings.lock().capture_private);
        let off = Settings { capture_private: false, ..Settings::default() };
        assert!(t.daemon.with_video_button(&off).capture_private, "nor can the browser turn it off");
    }

    #[test]
    fn properties_refuse_running_downloads_and_bad_links() {
        let t = setup("props_refuse", Duplicate::NewName);
        let running = t.daemon.engine.reserve_id();
        t.daemon.store.publish(&Outgoing::Added { id: running, url: "https://x/a".into() });
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
            .store
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
        t.daemon.settings.lock().file_exists = FileExists::Overwrite;
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
        t.daemon.store.publish(&Outgoing::Added { id, url: "https://x/a".into() });
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
        t.daemon.store.publish(&Outgoing::Added { id, url: "https://x/q.bin".into() });
        t.daemon.set_download_limit(id, Some(64 * 1024)).unwrap();
        t.daemon.set_limit_remember(id, false).unwrap();
        t.daemon.store.hold(id);
        t.daemon.store.publish(&Outgoing::Canceled { id, url: "https://x/q.bin".into() });
        let e = t.daemon.entry(id).unwrap();
        assert_eq!(e.status, Status::Waiting);
        assert_eq!(e.speed_limit, None);

        // Remembered, it stays.
        let id = t.daemon.engine.reserve_id();
        t.daemon.store.publish(&Outgoing::Added { id, url: "https://x/r.bin".into() });
        t.daemon.set_download_limit(id, Some(64 * 1024)).unwrap();
        t.daemon.store.hold(id);
        t.daemon.store.publish(&Outgoing::Canceled { id, url: "https://x/r.bin".into() });
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
            settings::save(&s, &path).unwrap();
            let (daemon, ..) = Daemon::new(Paths { settings: Some(path.clone()), store: StoreFiles::default() });
            let s = daemon.settings.lock().clone();
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
        let mut s = t.daemon.settings.lock().clone();
        s.speed_limit_enabled = true;
        s.speed_limit_kbps = 100_000;
        s.speed_limit_per_file_kbps = Some(32);
        t.daemon.transfers.set_settings(s.clone()).unwrap();
        assert_eq!(t.daemon.engine.per_file_limit(), Some(32 * 1024));

        // 48 KiB at 32 KiB/s takes about 1.5s; unlimited, a fraction of that.
        let started = Instant::now();
        let id = t.add(&serve_ranges(Arc::new(vec![3u8; 48 * 1024]), Arc::new(AtomicU64::new(0))));
        t.wait(id, Status::Done);
        let secs = started.elapsed().as_secs_f64();
        assert!(secs >= 1.0, "took {secs:.2}s; the limit per file did not hold");

        // It only applies while the speed limit is on.
        s.speed_limit_enabled = false;
        t.daemon.transfers.set_settings(s).unwrap();
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
            .store
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
        t.daemon.settings.lock().categories[0].remember_folder = true;
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
        let s = t.daemon.settings.lock().clone();
        assert_eq!(s.categories[0].last_folder.as_deref(), Some(chosen.display().to_string().as_str()));
        assert_eq!(s.folders(&uidm_infrastructure::settings::default_downloads_dir()).for_name("b.zip"), chosen);
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
        let req = t.daemon.store.requests().get(&id).cloned().unwrap();
        let headers = t.daemon.transfers.request_for(req).headers;
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
        t.daemon.settings.lock().categories[2].remember_folder = true;
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
        let s = t.daemon.settings.lock().clone();
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
        let saved = |t: &Setup| t.daemon.store.queues().iter().find(|q| q.id == id).cloned().unwrap();
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
        let s = t.daemon.settings.lock().clone();
        assert!(s.general_remember_folder);
        assert_eq!(s.folders(&uidm_infrastructure::settings::default_downloads_dir()).for_name("other.xyz"), chosen);
        assert_eq!(s.later_queue, Some(queue));
        // Download later now goes to that queue without being told.
        let (later, _) = t.daemon.add_later(Download { url: "https://x/more.xyz".into(), ..Download::default() });
        assert_eq!(queue::queue_of(&t.daemon.store.queues(), later), Some(queue));

        // "Remember my selection" for an address already in the list.
        let url = serve();
        let old = t.finished(&url, &t.dir.join("twice.bin"));
        let again = t.add(&url);
        assert_eq!(t.wait(again, Status::Confirming).duplicate_of, Some(old));
        t.daemon
            .confirm(Answer { duplicate: Some(Duplicate::NewName), remember_duplicate: true, ..Answer::start(again) })
            .unwrap();
        assert_eq!(t.daemon.settings.lock().if_duplicate, Duplicate::NewName);
    }

    #[test]
    fn the_quota_pauses_downloads_and_lets_them_go_again() {
        let t = setup("quota", Duplicate::NewName);
        {
            let mut s = t.daemon.settings.lock();
            s.quota_enabled = true;
            s.quota_mb = 1;
            s.quota_hours = 1;
        }
        let url = serve();
        let id = t.failed(&url);
        // 2 MB came in this minute.
        {
            let mut q = t.daemon.store.quota();
            q.started(99, 0);
            q.progress(99, 2 << 20, uidm_infrastructure::quota::now_minute());
        }
        t.daemon.check_quota();
        assert!(t.window_got(|m| matches!(m, Outgoing::Quota { paused: true, resumes_at: Some(_), .. })));
        assert!(t.daemon.quota.paused());
        // A download started while paused is stopped on the next check.
        t.daemon.retry(id).unwrap();
        t.daemon.check_quota();
        let (queues, downloads) = t.daemon.quota.release();
        assert!(!downloads.is_empty(), "the new download is held too");
        t.daemon.quota.hold(&queues, &downloads.into_iter().collect::<Vec<_>>());

        *t.daemon.store.quota() = Usage::default();
        t.daemon.check_quota();
        assert!(!t.daemon.quota.paused());
        assert!(t.window_got(|m| matches!(m, Outgoing::Quota { paused: false, .. })));
    }

    #[test]
    fn a_crawl_asks_the_browser_for_its_cookies() {
        let t = setup("crawl_cookies", Duplicate::NewName);
        // The browser's extension is connected; without it nobody is asked.
        let (extension, _from_daemon) = channel();
        t.daemon.clients.clients.lock().unwrap().insert(2, extension);
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
        let saved = grab::load_project(&dir, "nightly").unwrap();
        assert_eq!(saved.seen, [format!("http://{addr}/report.pdf")]);
        assert_eq!(t.len(), 1, "the new find was added");
        assert!(
            t.daemon.store.queues().iter().any(|q| q.id == MAIN_QUEUE && q.max_concurrent == 3),
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
        assert!(!t.daemon.clients.extension_connected(), "only a window is connected");
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
        assert!(t.daemon.settings.lock().last_update_check > 0, "the check was noted");
    }

    #[test]
    fn a_dropped_link_always_gets_the_save_as_dialog() {
        let t = setup("ask_flag", Duplicate::NewName);
        assert!(!t.daemon.settings.lock().ask_where_to_save);
        let (id, _) = t.daemon.add(Download { url: serve(), ask: Some(true), ..Download::default() });
        t.wait(id, Status::Confirming);
    }

    #[test]
    fn some_types_start_without_the_dialog_and_add_only_waits() {
        let t = setup("auto_start", Duplicate::NewName);
        {
            let mut s = t.daemon.settings.lock();
            s.ask_where_to_save = true;
            s.auto_start_types = vec!["bin".into()];
        }
        let id = t.add(&serve());
        assert_eq!(t.wait(id, Status::Done).path.map(PathBuf::from), Some(t.dir.join("f.bin")));

        t.daemon.settings.lock().add_only = true;
        let id = t.add(&serve());
        assert_eq!(t.daemon.transfers.status(id), Some(Status::Waiting));
        assert_eq!(queue::queue_of(&t.daemon.store.queues(), id), Some(MAIN_QUEUE));
    }

    #[test]
    fn a_download_removed_from_its_queue_stays_listed_and_waits_for_a_hand_start() {
        let t = setup("remove_from_queue", Duplicate::NewName);
        let (id, _) = t.daemon.add_later(Download { url: serve(), ..Download::default() });
        let (kept, _) = t.daemon.add_later(Download { url: format!("{}#kept", serve()), ..Download::default() });
        assert_eq!(queue::queue_of(&t.daemon.store.queues(), id), Some(MAIN_QUEUE));

        t.daemon.remove_from_queue(id).unwrap();
        assert_eq!(queue::queue_of(&t.daemon.store.queues(), id), None, "it left the queue");
        assert_eq!(t.daemon.transfers.status(id), Some(Status::Waiting), "and still waits in the list");
        assert!(t.daemon.remove_from_queue(id).is_err(), "it is in no queue now");

        // Starting the queue runs what is left in it, not the removed one.
        t.daemon.start_queue(MAIN_QUEUE, None).unwrap();
        t.daemon.tick();
        t.wait(kept, Status::Done);
        t.daemon.tick();
        assert_eq!(t.daemon.transfers.status(id), Some(Status::Waiting), "no queue started it");

        // Move to queue puts it back.
        t.daemon.move_to_queue(id, MAIN_QUEUE, None).unwrap();
        assert_eq!(queue::queue_of(&t.daemon.store.queues(), id), Some(MAIN_QUEUE));
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
            while self.daemon.store.queues().iter().any(|x| x.id == q && x.running) {
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
        assert_eq!(t.daemon.transfers.status(id), Some(Status::Done), "it stays finished in the queue");

        // Unchanged: left alone.
        t.run_queue(q);
        assert_eq!(t.daemon.transfers.status(id), Some(Status::Done));
        assert_eq!(std::fs::read(&path).unwrap(), b"version 1");
        assert_eq!(queue::queue_of(&t.daemon.store.queues(), id), Some(q), "a finished download stays in its queue");

        // Same size, new ETag: downloaded again over the same file, in the
        // same row.
        *file.lock().unwrap() = (b"version 2".to_vec(), Some("\"v2\"".to_string()));
        t.run_queue(q);
        let again = t.wait(id, Status::Done);
        assert_eq!(again.path, first.path, "the same file");
        assert_eq!(std::fs::read(&path).unwrap(), b"version 2");
        assert_eq!(again.validator.as_ref().and_then(|v| v.etag.as_deref()), Some("\"v2\""));
        assert_eq!(std::fs::read_dir(&t.dir).unwrap().count(), 1, "no second copy or leftover");
        assert_eq!(queue::queue_of(&t.daemon.store.queues(), id), Some(q), "and it stays in the queue");

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
        assert_eq!(queue::queue_of(&t.daemon.store.queues(), id), None);
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
        assert_eq!(t.daemon.transfers.status(id), Some(Status::Done));
        assert_eq!(std::fs::read(&path).unwrap(), b"manual 1");

        // Synchronizing: it is checked and downloaded again in its row, even
        // where a duplicate would get a new name.
        t.daemon.settings.lock().if_duplicate = Duplicate::NewName;
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
            grab::load_project(&dir, "manuals").unwrap().kind,
            queue::QueueKind::Synchronize,
            "the type is saved"
        );
    }

    #[test]
    fn a_scheduled_project_stops_its_downloads_at_its_stop_time() {
        let t = setup("grab_stop_time", Duplicate::NewName);
        let url = serve_ranges(Arc::new(vec![7u8; 4 * 1024 * 1024]), Arc::new(AtomicU64::new(0)));
        let today = uidm_infrastructure::clock::local_now().day;
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
        let main = || t.daemon.store.queues().iter().find(|q| q.id == MAIN_QUEUE).cloned().unwrap();
        assert_eq!(main().stop_at_once, Some((today + 1) * 1440 + 60), "the queue's run has the project's stop time");
        assert!(main().running);
        let id = *main().order.first().unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while t.daemon.transfers.status(id) != Some(Status::Downloading) {
            assert!(Instant::now() < until, "never started: {:?}", t.daemon.entry(id));
            t.daemon.tick();
            thread::sleep(Duration::from_millis(20));
        }

        // The stop time comes: the queue stops, its download waits again.
        t.daemon
            .store
            .update_queue(MAIN_QUEUE, |q| {
                q.stop_at_once = Some(uidm_infrastructure::clock::local_now().total());
                Ok(())
            })
            .unwrap();
        t.daemon.tick();
        assert!(!main().running, "the queue stopped");
        assert_eq!(main().stop_at_once, None, "the stop time was for that run only");
        t.wait(id, Status::Waiting);
        assert!(t.daemon.power.running().is_none(), "a stopped run does not do its when-done action");
    }

    #[test]
    fn a_scheduled_project_leaves_a_run_the_user_started_alone() {
        let t = setup("grab_user_run", Duplicate::NewName);
        let url = serve_ranges(Arc::new(vec![7u8; 4 * 1024 * 1024]), Arc::new(AtomicU64::new(0)));
        let today = uidm_infrastructure::clock::local_now().day;
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
        let main = t.daemon.store.queues().iter().find(|q| q.id == MAIN_QUEUE).cloned().unwrap();
        assert!(main.running);
        assert_eq!(main.order.len(), 1, "the project's file joined the run");
        assert_eq!(main.stop_at_once, None, "the user's run keeps going past the project's stop time");
        assert_eq!(main.force_close_once, None);
        assert_eq!(main.when_done_once, None);
    }

    #[test]
    fn a_scheduled_project_force_closes_when_asked() {
        // Never really shut down, even if a check below fails.
        std::env::set_var(uidm_infrastructure::power_control::POWER_DRY_RUN_ENV, "1");
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
            let forced = t.daemon.power.running().map(|r| (r.action, r.force_close));
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
        assert_eq!(t.daemon.store.queues()[0].force_close_once, Some(true));
        let until = Instant::now() + Duration::from_secs(10);
        while t.daemon.store.queues().iter().any(|q| q.id == MAIN_QUEUE && q.running) {
            assert!(Instant::now() < until, "the run never ended");
            t.daemon.tick();
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(countdown_forces(&t), Some((WhenDone::ShutDown, true)));
        let main = t.daemon.store.queues()[0].clone();
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
                let mut s = t.daemon.settings.lock();
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
    fn canceling_the_dialog_drops_the_early_download() {
        let t = setup("early_cancel", Duplicate::NewName);
        {
            let mut s = t.daemon.settings.lock();
            s.ask_where_to_save = true;
            s.start_while_asking = true;
        }
        let body: Arc<Vec<u8>> = Arc::new(vec![7u8; 1024 * 1024]);
        let id = t.add(&serve_ranges(body, Arc::new(AtomicU64::new(0))));
        t.wait(id, Status::Confirming);
        let until = Instant::now() + Duration::from_secs(30);
        while t
            .daemon
            .transfers
            .early()
            .early_of(id)
            .is_none_or(|s| t.daemon.transfers.early().runs()[&s].file.is_none())
        {
            assert!(Instant::now() < until, "the early download never started");
            thread::sleep(Duration::from_millis(20));
        }
        let shadow = t.daemon.transfers.early().early_of(id).unwrap();
        let dir = t.daemon.transfers.early().runs()[&shadow].dir.clone();
        t.daemon.cancel(id);
        t.wait(id, Status::Canceled);
        let until = Instant::now() + Duration::from_secs(10);
        while dir.exists() || t.daemon.transfers.early().runs().contains_key(&shadow) {
            assert!(Instant::now() < until, "the early download was not dropped");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_finished_video_converts_into_a_row_of_its_own() {
        let Some(ffmpeg) = settings::ffmpeg(&Settings::default()) else {
            eprintln!("no ffmpeg; skipped");
            return;
        };
        let t = setup("convert", Duplicate::NewName);
        let clip = t.dir.join("talk.mkv");
        let id = t.finished("https://x/talk.mkv", &clip);
        let made = uidm_infrastructure::platform::no_window(&mut std::process::Command::new(&ffmpeg))
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
        t.daemon.convert(id, uidm_infrastructure::media::convert::Target::M4a, true).unwrap();
        let new = t.daemon.store.table().max_id();
        let done = t.wait(new, Status::Done);
        let out = PathBuf::from(done.path.unwrap());
        assert_eq!(out, t.dir.join("talk.m4a"));
        assert!(std::fs::metadata(&out).unwrap().len() > 0);
        assert_eq!(done.note.as_deref(), Some("Converted from talk.mkv"));
        assert!(t.daemon.entry(id).is_none() && !clip.exists(), "the original went, as asked");
        assert!(t.daemon.convert(new + 100, uidm_infrastructure::media::convert::Target::Mp3, false).is_err());
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
        let requests = t.daemon.store.requests();
        let cookies: Vec<_> = ids.iter().map(|id| requests.get(id).and_then(|d| d.cookies.clone())).collect();
        assert_eq!(cookies, [Some("session=abc".to_string()), None]);
    }
}
