//! What a client of the daemon asks it to do (`Command`), what it hears back
//! (`Event`), and the ports on each side: `DaemonClient` is the desktop
//! window's connection, and `Requests` is what the daemon's server hands
//! each client's commands to, with a `Reply` for the answers.
//!
//! These are plain values: how they travel (the framing, the JSON shapes,
//! the handshake) is the adapters' business, in `uidm-infrastructure`.

mod table;

pub use table::{apply, change_of};

use std::sync::Arc;

use uidm_domain::download::{
    Answer, CommandLine, Download, DownloadEntry, OnDone, PageLink, Properties, SegmentInfo, Validator, ZipEntry,
};
use uidm_domain::grab::{GrabFile, GrabRequest};
use uidm_domain::media::{MediaInfo, Target};
use uidm_domain::queue::{Queue, WhenDone};
use uidm_domain::quota::QuotaNotice;
use uidm_domain::settings::Settings;
use uidm_domain::update::Release;

/// A request to the daemon.
#[derive(Debug, Clone)]
// Each command lives only while it is sent, so boxing the big variants would add churn for no gain.
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// A client introduces itself. The desktop window names itself
    /// `window`; the browser extension, through its host, names nothing.
    Hello {
        version: Option<String>,
        client: Option<String>,
    },
    /// Answered with `Pong`.
    Ping,
    /// A command line from `uidm-gui`; answered with `CommandLine`.
    CommandLine(CommandLine),
    /// Read a stream's playlist; answered with `MediaInfo`.
    ProbeMedia {
        request: u64,
        download: Download,
    },
    /// Links from a page, for the link picker.
    PickLinks {
        page: String,
        links: Vec<PageLink>,
        user_agent: Option<String>,
    },
    /// The browser's answer to `RenderPage`: the page's HTML after its
    /// scripts ran and its address then, or nothing, and the addresses its
    /// scripts opened in new windows.
    Rendered {
        request: u64,
        html: Option<String>,
        url: Option<String>,
        popups: Vec<String>,
    },
    /// The browser's answer to `CookiesWanted`.
    Cookies {
        request: u64,
        cookies: Option<String>,
    },
    /// A download to hand off to the engine.
    Add(Download),
    /// Many downloads at once, each added to wait in `queue` (the main
    /// queue when `None`) without starting. Answered with `AddedMany`.
    AddMany {
        items: Vec<Download>,
        queue: Option<u64>,
    },
    /// Crawl a site for files; progress comes as `GrabProgress` and the
    /// result as `GrabDone`.
    Grab(GrabRequest),
    /// Stop this client's crawl; `GrabDone` follows with what it found.
    CancelGrab,
    /// Ask each link's server for its type and size; each answer comes as
    /// `LinkInfo`.
    ProbeLinks {
        request: u64,
        links: Vec<Download>,
    },
    /// Stop asking about the links of the last `ProbeLinks`.
    StopProbeLinks,
    /// Cancel a queued or running download.
    Cancel {
        id: u64,
    },
    /// Start a failed or canceled download again from its partial file.
    Retry {
        id: u64,
    },
    /// Download a finished, failed or canceled download again from the
    /// start.
    Redownload {
        id: u64,
    },
    /// Forget a download, and with `delete_files` recycle its files.
    Remove {
        id: u64,
        delete_files: bool,
    },
    /// Ask for every download, the queues and the download waiting for a
    /// new link.
    List,
    /// Stop every queued or running download, keeping its partial file.
    PauseAll,
    /// Start again what `PauseAll` stopped, and any interrupted download.
    ResumeAll,
    /// Answer the save-as dialog of a `confirming` download, or start a
    /// `waiting` one.
    Confirm(Answer),
    /// Create a queue (`id` 0) or change one.
    SaveQueue {
        queue: Queue,
    },
    /// Delete a queue; its waiting downloads move to the main queue.
    DeleteQueue {
        id: u64,
    },
    StartQueue {
        id: u64,
    },
    /// Stop a queue; its running downloads wait in it again.
    StopQueue {
        id: u64,
    },
    /// What happens when this queue's current run ends, this time only.
    SetQueueOnce {
        id: u64,
        when_done: Option<WhenDone>,
    },
    /// Put a download in a queue (or move it within one) at `position`, or
    /// at the end.
    MoveToQueue {
        id: u64,
        queue: u64,
        position: Option<usize>,
    },
    /// Take a download out of its queue but keep it in the list.
    RemoveFromQueue {
        id: u64,
    },
    /// Stop the countdown before sleeping or shutting down.
    CancelCountdown,
    /// Give a download a new link: `download` now, or the next one the
    /// browser hands over. `restart` starts over when the link is for
    /// another file.
    RefreshLink {
        id: u64,
        download: Option<Download>,
        restart: bool,
    },
    /// Stop waiting for a new link from the browser.
    CancelRefresh,
    /// Look for a newer UIDM release now; answered with `Update`.
    CheckUpdate,
    /// Download and run the newest release's Setup; failures come back as
    /// `UpdateFailed`.
    InstallUpdate,
    /// Convert a finished video with ffmpeg.
    Convert {
        id: u64,
        to: Target,
        delete_original: bool,
    },
    /// List the files inside a waiting zip download; answered with
    /// `ZipPreview`.
    PreviewZip {
        id: u64,
    },
    /// Change a stopped or finished download's link, referrer,
    /// description, login, or its file's name and folder.
    SetProperties {
        id: u64,
        changes: Properties,
    },
    /// Set or clear one download's own speed limit, and whether it stays
    /// through a stop.
    SetSpeedLimit {
        id: u64,
        bytes_per_sec: Option<u64>,
        remember: Option<bool>,
    },
    /// Stop the countdown and sleep, hibernate or shut down now.
    CountdownNow,
    /// Set or clear what one unfinished download does once it finishes.
    SetOnDone {
        id: u64,
        on_done: Option<OnDone>,
    },
    /// Ask for the current settings; answered with `Settings`.
    GetSettings,
    /// Replace the settings; answered with `Settings` or `Error`.
    SetSettings {
        settings: Settings,
    },
}

/// Why the connection to the daemon went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lost {
    /// The daemon closed it.
    Closed,
    /// It could not be made, or broke, and why.
    Failed(String),
}

/// What a client hears: the state of its connection, and the daemon's
/// replies and announcements.
#[derive(Debug, Clone, PartialEq)]
// Each event lives only while it is handled, so boxing the big variants would add churn for no gain.
#[allow(clippy::large_enum_variant)]
pub enum Event {
    /// The connection is made and the client has introduced itself.
    Connected,
    /// The connection went; nothing more comes until `connect` again.
    Lost(Lost),
    /// The daemon's answer to `Hello`.
    Hello {
        name: String,
        version: String,
    },
    /// The answer to `Ping`.
    Pong,
    /// An `Add` or `Retry` was taken as download `id`.
    Ack {
        id: u64,
        url: String,
        status: String,
    },
    /// What a `ProbeMedia` stream offers.
    MediaInfo {
        request: u64,
        info: MediaInfo,
    },
    /// Ask the browser for its cookies for `url`; answered with `Cookies`.
    CookiesWanted {
        request: u64,
        url: String,
    },
    /// Ask the browser to render `url`; answered with `Rendered`.
    RenderPage {
        request: u64,
        url: String,
    },
    /// The answer to a `CommandLine`: whether an open window ran it.
    CommandLine {
        window_open: bool,
    },
    /// A download was queued.
    Added {
        id: u64,
        url: String,
    },
    /// A download has begun.
    Started {
        id: u64,
        url: String,
        path: String,
        total: Option<u64>,
        resumable: bool,
        connections: usize,
        partial: Option<String>,
        validator: Option<Validator>,
    },
    /// Progress of a running download.
    Progress {
        id: u64,
        url: String,
        done: u64,
        total: Option<u64>,
        parts: Option<[u64; 2]>,
        segments: Option<Vec<SegmentInfo>>,
    },
    /// A download finished; `path` is the saved file.
    Done {
        id: u64,
        url: String,
        path: String,
        note: Option<String>,
    },
    Failed {
        id: u64,
        url: String,
        error: String,
    },
    Canceled {
        id: u64,
        url: String,
    },
    /// A download left the list.
    Removed {
        id: u64,
    },
    /// A download's whole entry, replacing what the client had.
    Updated {
        item: DownloadEntry,
    },
    /// Every download, oldest first.
    Downloads {
        items: Vec<DownloadEntry>,
    },
    /// The current settings.
    Settings {
        settings: Settings,
    },
    /// Come to the front (from a notification).
    Show,
    /// Show finished download `id`'s completion dialog again.
    ShowDone {
        id: u64,
    },
    /// Every queue, the main queue first.
    Queues {
        items: Vec<Queue>,
    },
    /// The computer sleeps, hibernates or shuts down in `seconds` unless
    /// canceled. `queue` is empty and `file` names the download when one
    /// download asked for it.
    Countdown {
        action: WhenDone,
        seconds: u32,
        queue: String,
        file: Option<String>,
    },
    CountdownCanceled,
    /// UIDM exits: windows close.
    Quit,
    /// Which download waits for a new link from the browser, if any.
    RefreshWaiting {
        id: Option<u64>,
    },
    /// A download's file name is taken: it waits as `confirming` with
    /// `path`, the taken file, for the save-as dialog.
    NameTaken {
        id: u64,
        url: String,
        path: String,
        total: Option<u64>,
    },
    /// The new link of a `RefreshLink` is for another file.
    RefreshMismatch {
        id: u64,
        url: String,
        reason: String,
    },
    /// Downloads an `AddMany` added, and those skipped as duplicates.
    AddedMany {
        count: usize,
        skipped: usize,
    },
    /// What link `index` of a `ProbeLinks` request's server said.
    LinkInfo {
        request: u64,
        index: usize,
        total: Option<u64>,
        content_type: Option<String>,
        error: Option<String>,
    },
    /// Links from a page for the link picker.
    PickLinks {
        page: String,
        links: Vec<PageLink>,
        user_agent: Option<String>,
    },
    /// Where the download quota stands.
    Quota {
        used: u64,
        limit: u64,
        paused: bool,
        resumes_at: Option<u64>,
    },
    /// Whether the browser extension is connected, and when one last was.
    BrowserLink {
        connected: bool,
        last_seen: Option<u64>,
    },
    /// What the update check found: a newer release, none, or why not.
    Update {
        newer: Option<Release>,
        error: Option<String>,
    },
    UpdateFailed {
        error: String,
    },
    /// The files inside a zip download, or why they could not be listed.
    ZipPreview {
        id: u64,
        entries: Vec<ZipEntry>,
        error: Option<String>,
    },
    /// A crawl's progress.
    GrabProgress {
        pages: u64,
        bytes: u64,
        found: usize,
        explored: u64,
        added: Vec<u64>,
    },
    /// A crawl ended.
    GrabDone {
        files: Vec<GrabFile>,
        pages: u64,
        explored: u64,
        stopped: Option<String>,
        login_cookies: Option<String>,
    },
    /// A failure the client should hear about.
    Error {
        message: String,
    },
    /// An `Add` the daemon did not take, and why.
    Rejected {
        url: String,
        message: String,
    },
}

impl From<QuotaNotice> for Event {
    /// The `Quota` event that tells the clients `notice`.
    fn from(n: QuotaNotice) -> Self {
        Event::Quota { used: n.used, limit: n.limit, paused: n.paused, resumes_at: n.resumes_at }
    }
}

/// Why a one-off request to the daemon failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DaemonError {
    #[error("could not reach the daemon: {0}")]
    Connect(String),
    #[error("the daemon closed the connection")]
    Closed,
    #[error("{0}")]
    Refused(String),
}

/// The connection to the daemon.
pub trait DaemonClient: Send + Sync {
    /// Connect in the background, first restarting the daemon if `restart`,
    /// and hand every event to `on_event` until the connection is lost
    /// (which is the last event). Once connected the client introduces
    /// itself and asks for the list and the settings.
    fn connect(&self, restart: bool, on_event: Box<dyn FnMut(Event) + Send>);

    /// Send `command`. Returns false when not connected or sending failed.
    fn send(&self, command: Command) -> bool;

    /// Hand a command line to the daemon on a connection of its own,
    /// starting the daemon if needed. `Ok(true)` means an open window ran
    /// it; `Ok(false)` that it waits for the window about to open.
    fn run_command_line(&self, command: CommandLine) -> Result<bool, DaemonError>;
}

/// Where the daemon's answers to one client go. Cloned freely, so work on
/// other threads (a crawl, a probe) can answer when it is done.
#[derive(Clone)]
pub struct Reply(Arc<dyn Fn(Event) -> bool + Send + Sync>);

impl Reply {
    /// A reply that hands each event to `send`, which returns false once
    /// the client is gone.
    pub fn new(send: impl Fn(Event) -> bool + Send + Sync + 'static) -> Self {
        Reply(Arc::new(send))
    }

    /// Send `event` to the client. False when the client is gone.
    pub fn send(&self, event: Event) -> bool {
        (self.0)(event)
    }
}

/// The daemon's connected clients: which are windows and which the browser
/// extension, and how an event reaches them.
pub trait ClientList: Send + Sync {
    /// Whether no client is connected.
    fn is_empty(&self) -> bool;

    /// Whether a desktop window is connected.
    fn window_open(&self) -> bool;

    /// Whether client `client` is a desktop window.
    fn is_window(&self, client: u64) -> bool;

    /// Note that client `client` is a desktop window.
    fn add_window(&self, client: u64);

    /// Note that client `client` is the browser extension; the windows
    /// hear of it when it is the only one.
    fn browser_connected(&self, client: u64);

    /// Whether the browser extension is connected, and when one last was.
    fn browser_link(&self) -> Event;

    /// Send `event` to every client.
    fn broadcast(&self, event: Event);

    /// Send `event` to the desktop windows only.
    fn to_windows(&self, event: Event);
}

/// What the daemon does with its clients' commands: the port its server
/// drives.
pub trait Requests: Send + Sync {
    /// Handle `command` from client `client`; a direct answer goes to
    /// `reply`.
    fn handle(&self, client: u64, command: Command, reply: &Reply);

    /// Client `client` disconnected.
    fn gone(&self, client: u64);
}
