//! The state actor (AGENTS.md section 7.1): the one thread that owns
//! [`AppState`]. Commands, the daemon connection, the tray and window
//! events send it [`Inbox`] messages; it applies them, opens and closes the
//! windows the state asks for, and sends each subscribed window its
//! snapshot and then its patches, at most one every 200 ms while the
//! daemon's progress streams in.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tauri::ipc::Channel;
use tauri::AppHandle;
use udm_application::daemon::{Command, DaemonClient, Event};
use udm_application::window::{GrabLibrary, ListFiles, ListFormat, SystemServices, WindowFiles};
use udm_domain::download::{OnDone, Status};
use udm_domain::grab::{project_login_target, GrabRequest};
use udm_domain::id::{DownloadId, QueueId};
use udm_domain::package::Packaging;
use udm_domain::ports::{Clock, FileOpener, GrabDownloads, SaveFolders, Timestamps};
use udm_domain::settings::Notify;

use crate::contract::{Look, OpenHow, Patch, Update, View};
use crate::projection::{self, main, Ctx};
use crate::state::{apply, AppState, UiMessage};
use crate::windows::{self, Label};
use crate::{actions, contract};

/// The fastest a window gets patches while the list's progress changes.
const PATCH_EVERY: Duration = Duration::from_millis(200);
/// How long after losing the daemon the shell connects again.
const RECONNECT_AFTER: Duration = Duration::from_secs(3);

/// What the actor hears.
pub enum Inbox {
    Ui(UiMessage),
    /// A window's subscription, from its `subscribe` command.
    Subscribe {
        label: Label,
        channel: Channel<Update>,
    },
    /// A window closed, by the user or by the shell.
    Closed(String),
    /// A toolbar or menu action on the main window's selection.
    Act(contract::ListAction),
    /// Send a command to the daemon, telling `reply` whether it went.
    Send {
        command: Box<Command>,
        reply: Option<Sender<bool>>,
    },
    /// The Download File Info dialog's answer.
    Answer {
        answer: Box<udm_domain::download::Answer>,
        reply: Sender<bool>,
    },
    /// Remove the downloads Confirm deletion asked about, and with
    /// `dont_ask` stop asking.
    Remove {
        delete_files: bool,
        dont_ask: bool,
    },
    /// "Don't show this dialog again" in Download complete.
    NoDoneDialogs,
    /// Open a finished download's file or folder.
    Open {
        id: DownloadId,
        how: OpenHow,
    },
    /// A progress window's speed limit.
    Limit {
        id: DownloadId,
        bytes_per_sec: Option<u64>,
        remember: bool,
    },
    /// A progress window's Options on completion.
    OnDone {
        id: DownloadId,
        on_done: Option<OnDone>,
    },
    /// Open a window by a click.
    OpenWindow(contract::OpenWindow),
    /// The Scheduler's Apply for a queue.
    SaveQueue {
        draft: Box<contract::QueueDraft>,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// The Scheduler's New queue.
    CreateQueue {
        name: String,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// The Scheduler's Apply for the download limits.
    SaveQuota {
        quota: contract::QuotaDraft,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// Move a download along its queue.
    MoveQueued {
        id: DownloadId,
        offset: i32,
    },
    /// The grabber's Next on its last step: check the fields, keep a new
    /// password, and start the crawl.
    GrabStart {
        draft: Box<contract::GrabDraft>,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// The crawl after its password was kept.
    GrabBegin(Box<GrabRequest>),
    GrabStop,
    /// Add the ticked results to `queue`, and start it with `start`.
    GrabAdd {
        indices: Vec<u32>,
        queue: QueueId,
        start: bool,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// A template's fields.
    GrabTemplate {
        name: String,
        reply: Sender<Result<contract::GrabDraft, contract::InvokeError>>,
    },
    GrabOpen {
        name: String,
        reply: Sender<Result<contract::GrabOpened, contract::InvokeError>>,
    },
    GrabSave {
        name: String,
        draft: Box<contract::GrabDraft>,
        checked: Vec<bool>,
        schedule: Box<contract::GrabScheduleDraft>,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    GrabSaveTemplate {
        name: String,
        draft: Box<contract::GrabDraft>,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    GrabSaveFilters {
        filters: Vec<udm_domain::grab::Filter>,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// "Add checked files ... on closing the grabber".
    GrabAddOnClose(bool),
    /// Find file: select the next match, telling `reply` whether there was
    /// one.
    Find {
        input: Box<contract::FindInput>,
        reply: Sender<bool>,
    },
    /// Export's text for the downloads `scope` names, and how many.
    ExportText {
        scope: contract::ExportScope,
        format: contract::ExportFormat,
        reply: Sender<(String, usize)>,
    },
    /// The speed limiter's settings.
    Limiter {
        draft: contract::LimiterDraft,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// Save category `original` (a new one when `None`), or delete it when
    /// `input` is `None`.
    Category {
        original: Option<String>,
        input: Option<Box<contract::CategoryInput>>,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// Open a category's folder.
    BrowseCategory(String),
    /// Put the selected downloads in `queue`, or take them out of theirs.
    QueueSelection(Option<QueueId>),
    /// A download's properties.
    Properties {
        id: DownloadId,
        input: Box<contract::PropertiesInput>,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// Batch download's or the links window's downloads, for `queue`.
    AddItems {
        items: Vec<udm_domain::download::Download>,
        queue: QueueId,
        start: bool,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// The links window's ticked links.
    AddLinks {
        indices: Vec<u32>,
        queue: QueueId,
        start: bool,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// Show or hide the drop target.
    DropTarget(bool),
    /// The tip of the day closed: the tip to show next, and whether tips
    /// show at startup.
    TipClosed {
        next: u32,
        on_startup: bool,
    },
    /// The login prompt's OK.
    Login {
        user: String,
        password: String,
        save: bool,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// The pickers' Add, from the window `label`.
    AddPicks {
        label: Label,
        input: Box<contract::PickInput>,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// Open a package's unpacked folder.
    OpenPackage(udm_domain::id::PackageId),
    /// Close the strip offering this release.
    DismissRelease,
    /// Options' OK: the draft to check and send, and whether it went.
    SaveOptions {
        draft: Box<contract::OptionsDraft>,
        reply: Sender<Result<(), contract::InvokeError>>,
    },
    /// Connect to the daemon again.
    Reconnect,
}

/// The actor's mailbox, as the commands hold it (a managed type).
pub type Outbox = Sender<Inbox>;

/// The window's language, by its place in `i18n::LANGUAGES`, for the
/// commands' error messages (a managed type).
pub type CurrentLanguage = Arc<AtomicUsize>;

/// The ports the actor reaches.
pub struct Ports {
    /// The grabber's projects, templates and filters, when their folder can
    /// be found.
    pub library: Option<Arc<dyn GrabLibrary>>,
    /// The downloads a crawl's finds become.
    pub found: Arc<dyn GrabDownloads>,
    /// The system's credential store, for the grabber's logins.
    pub system: Arc<dyn SystemServices>,
    /// Exporting the download list.
    pub lists: Arc<dyn ListFiles>,
    /// The system's Downloads folder, for a category's folder.
    pub folders: Arc<dyn SaveFolders>,
    /// The window's own files: its layout.
    pub window_files: Arc<dyn WindowFiles>,
    pub language: CurrentLanguage,
    pub daemon: Arc<dyn DaemonClient>,
    pub clock: Arc<dyn Clock>,
    pub times: Arc<dyn Timestamps>,
    pub opener: Arc<dyn FileOpener>,
}

struct Subscriber {
    channel: Channel<Update>,
    seq: u32,
    last: Option<View>,
    look: Option<Look>,
}

struct Actor {
    app: AppHandle,
    ports: Ports,
    outbox: Outbox,
    state: AppState,
    subscribers: HashMap<Label, Subscriber>,
    /// Windows the state asked for and the actor opened.
    opened: BTreeSet<Label>,
    /// Windows the shell is closing, whose closing is not the user's.
    closing: HashSet<Label>,
    titles: HashMap<Label, String>,
    /// The layout file as last read or saved, so it is written only when it
    /// changes.
    layout: Option<Vec<u8>>,
    /// The system's Downloads folder, read once.
    downloads: std::path::PathBuf,
}

/// Start the actor and connect to the daemon. Returns its mailbox.
pub fn spawn(app: AppHandle, ports: Ports) -> Outbox {
    let (tx, rx) = mpsc::channel();
    let outbox = tx.clone();
    let spawned = thread::Builder::new().name("udm-state".into()).spawn(move || {
        let mut actor = Actor {
            app,
            ports,
            outbox,
            state: AppState::default(),
            subscribers: HashMap::new(),
            opened: BTreeSet::new(),
            closing: HashSet::new(),
            titles: HashMap::new(),
            layout: None,
            downloads: std::path::PathBuf::new(),
        };
        actor.downloads = actor.ports.folders.downloads_dir();
        // The layout file is small; reading it on this thread delays only
        // the first snapshot.
        let saved = actor.ports.window_files.read(crate::layout::FILE);
        let file = crate::layout::parse(saved.as_deref());
        apply(&mut actor.state, UiMessage::LayoutRead(Box::new(file)));
        actor.layout = crate::layout::bytes(&actor.state);
        actor.connect();
        actor.run(rx);
    });
    if let Err(e) = spawned {
        tracing::error!(error = %e, "the window's state thread did not start");
    }
    tx
}

impl Actor {
    fn connect(&self) {
        let outbox = self.outbox.clone();
        let times = Arc::clone(&self.ports.times);
        self.ports.daemon.connect(
            false,
            Box::new(move |event| {
                let lost = matches!(event, Event::Lost(_));
                let msg = UiMessage::Daemon { event: Box::new(event), at: Instant::now(), unix: times.unix_now() };
                let _ = outbox.send(Inbox::Ui(msg));
                if lost {
                    let outbox = outbox.clone();
                    thread::spawn(move || {
                        thread::sleep(RECONNECT_AFTER);
                        let _ = outbox.send(Inbox::Reconnect);
                    });
                }
            }),
        );
    }

    fn run(&mut self, rx: Receiver<Inbox>) {
        let mut last = Instant::now() - PATCH_EVERY;
        let mut dirty = false;
        loop {
            let wait = if dirty { PATCH_EVERY.saturating_sub(last.elapsed()) } else { Duration::from_secs(1) };
            let mut urgent = false;
            match rx.recv_timeout(wait) {
                Ok(msg) => {
                    urgent = !matches!(&msg, Inbox::Ui(UiMessage::Daemon { event, .. }) if matches!(**event, Event::Progress { .. }));
                    self.handle(msg);
                    dirty = true;
                }
                Err(RecvTimeoutError::Timeout) => {
                    // The countdown's seconds tick by themselves.
                    if self.state.asks.countdown.is_some() {
                        dirty = true;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if dirty && (urgent || last.elapsed() >= PATCH_EVERY) {
                self.reconcile();
                self.publish();
                self.save_layout();
                last = Instant::now();
                dirty = false;
            }
        }
    }

    fn lang(&self) -> &'static crate::i18n::Language {
        projection::language(&self.state)
    }

    fn send(&self, command: Command) -> bool {
        let sent = self.ports.daemon.send(command);
        if !sent {
            tracing::warn!("a command for the daemon was not sent: not connected");
        }
        sent
    }

    /// The login prompt's OK: save the login for the site, or give it to
    /// each download, then retry them.
    fn answer_login(&mut self, user: &str, password: &str, save: bool) -> Result<(), contract::InvokeError> {
        let lang = self.lang();
        let Some((ids, wanted)) = crate::extras::login_group(&self.state) else {
            return Err(crate::errors::gone(lang));
        };
        let view = crate::extras::login_view(&self.state);
        let save = save || view.is_some_and(|v| !v.own_site);
        let user = user.trim().to_string();
        if save {
            let Some(mut settings) = self.state.settings.clone() else {
                return Err(crate::errors::offline(lang));
            };
            let plain = ids
                .iter()
                .filter_map(|id| self.state.table.get(*id))
                .any(|e| !e.url.as_str().to_ascii_lowercase().starts_with("https:"));
            let old = settings.site_logins.iter().position(|l| l.site.eq_ignore_ascii_case(&wanted.host));
            let login = udm_domain::settings::SiteLogin {
                site: wanted.host.clone(),
                username: user.clone(),
                password: Some(password.to_string()),
                plain: plain || old.is_some_and(|i| settings.site_logins[i].plain),
            };
            match old {
                Some(i) => settings.site_logins[i] = login,
                None => settings.site_logins.push(login),
            }
            self.sent(lang, Command::SetSettings { settings })?;
        }
        for id in &ids {
            if !save {
                let login = udm_domain::download::DownloadLogin {
                    username: user.clone(),
                    password: Some(password.to_string()),
                };
                let changes = udm_domain::download::Properties { login: Some(login), ..Default::default() };
                self.send(Command::SetProperties { id: *id, changes });
            }
            self.send(Command::Retry { id: *id });
        }
        apply(&mut self.state, UiMessage::LoginAnswered(ids));
        Ok(())
    }

    /// The pickers' Add: the ticked links, videos or pictures, waiting in a
    /// queue.
    fn add_picks(&mut self, label: Label, input: &contract::PickInput) -> Result<(), contract::InvokeError> {
        let lang = self.lang();
        let queue = QueueId::new(input.queue.0);
        let settings = self.state.settings.clone().unwrap_or_default();
        let a = &self.state.asks;
        let (items, packaging) = match label {
            Label::PickLinks => {
                let l = a.links.as_ref().ok_or_else(|| crate::errors::gone(lang))?;
                crate::extras::link_downloads(l, &input.indices, input.package.as_deref(), &settings, &self.downloads)
                    .map_err(|e| crate::errors::invalid_text(e.to_string()))?
            }
            Label::PickVideos => {
                let v = a.videos.as_ref().ok_or_else(|| crate::errors::gone(lang))?;
                (crate::extras::video_downloads(v, &input.indices, input.quality), Packaging::None)
            }
            Label::Gallery => {
                let g = a.gallery.as_ref().ok_or_else(|| crate::errors::gone(lang))?;
                let items = crate::extras::gallery_downloads(g, &input.indices, &input.folder)
                    .map_err(|e| crate::errors::invalid_text(lang.tr(&e.to_string()).to_string()))?;
                (items, Packaging::None)
            }
            _ => return Err(crate::errors::gone(lang)),
        };
        if items.is_empty() {
            return Err(crate::errors::invalid(lang, "Tick the files to add."));
        }
        self.sent(lang, Command::AddMany { items, queue: Some(queue), packaging })?;
        if input.start {
            self.send(Command::StartQueue { id: queue });
        }
        let gone = match label {
            Label::PickLinks => UiMessage::WindowClosed("pick-links".into()),
            Label::PickVideos => UiMessage::WindowClosed("pick-videos".into()),
            _ => UiMessage::WindowClosed("gallery".into()),
        };
        apply(&mut self.state, gone);
        Ok(())
    }

    fn set_drop_target(&self, on: bool) {
        if let Some(mut settings) = self.state.settings.clone().filter(|s| s.drop_target != on) {
            settings.drop_target = on;
            self.send(Command::SetSettings { settings });
        }
    }

    /// Add `items` to wait in `queue`, and start it with `start`.
    fn add_items(
        &self,
        items: Vec<udm_domain::download::Download>,
        queue: QueueId,
        start: bool,
    ) -> Result<(), contract::InvokeError> {
        let lang = self.lang();
        if items.is_empty() {
            return Err(crate::errors::invalid(lang, "Tick the files to add."));
        }
        self.sent(lang, Command::AddMany { items, queue: Some(queue), packaging: Packaging::None })?;
        if start {
            self.send(Command::StartQueue { id: queue });
        }
        Ok(())
    }

    /// Save the layout file when it changed. The port writes on a thread of
    /// its own.
    fn save_layout(&mut self) {
        let bytes = crate::layout::bytes(&self.state);
        if bytes.is_some() && bytes != self.layout {
            if let Some(b) = bytes.clone() {
                self.ports.window_files.save(crate::layout::FILE, b);
            }
            self.layout = bytes;
        }
    }

    /// Read the projects folder on a worker.
    fn read_library(&self) {
        let outbox = self.outbox.clone();
        let library = self.ports.library.clone();
        thread::spawn(move || {
            let read = library.map(|l| Box::new(read(&*l)));
            let _ = outbox.send(Inbox::Ui(UiMessage::GrabLibraryRead(read)));
        });
    }

    fn grab_start(&mut self, draft: &contract::GrabDraft, reply: Sender<Result<(), contract::InvokeError>>) {
        let base = self.state.grab.library.as_ref().map_or_else(|| self.state.grab.request.clone(), |l| l.base.clone());
        let request = match crate::grabber::request(draft, &base) {
            Ok(r) => r,
            Err(e) => {
                let _ = reply.send(Err(crate::errors::grab(&e)));
                return;
            }
        };
        let lang = self.lang();
        if !self.state.connected() {
            let _ = reply.send(Err(crate::errors::offline(lang)));
            return;
        }
        let login = request.login_user.clone().filter(|_| !draft.login_password.is_empty());
        let (system, outbox, password) =
            (Arc::clone(&self.ports.system), self.outbox.clone(), draft.login_password.clone());
        // Keeping the password reaches the system's credential store, so it
        // runs on a worker before the crawl starts.
        thread::spawn(move || {
            if let Some(user) = login {
                let target = project_login_target(&request.start, &user);
                if let Err(e) = system.save_credential(&target, &user, &password) {
                    let _ = reply.send(Err(crate::errors::files(&e)));
                    return;
                }
            }
            let _ = outbox.send(Inbox::GrabBegin(Box::new(request)));
            let _ = reply.send(Ok(()));
        });
    }

    fn grab_add(&mut self, indices: &[u32], queue: QueueId, start: bool) -> Result<(), contract::InvokeError> {
        let lang = self.lang();
        let grab = &self.state.grab;
        let settings = self.state.settings.clone().unwrap_or_default();
        let login = grab.login_cookies.as_deref().or(grab.request.login_cookies.as_deref());
        let items: Vec<_> = crate::grabber::ticked(grab, indices)
            .iter()
            .filter_map(|f| self.ports.found.download_for(&grab.request, f, &settings, login))
            .collect();
        if items.is_empty() {
            return Err(crate::errors::invalid(lang, "Tick the files to add."));
        }
        let at_once = grab.request.download_at_once as usize;
        if let Some(q) = self.state.queues.iter().find(|q| q.id == queue && q.max_concurrent != at_once) {
            self.send(Command::SaveQueue { queue: udm_domain::queue::Queue { max_concurrent: at_once, ..q.clone() } });
        }
        self.sent(lang, Command::AddMany { items, queue: Some(queue), packaging: Packaging::None })?;
        if start {
            self.send(Command::StartQueue { id: queue });
        }
        Ok(())
    }

    fn grab_save(
        &mut self,
        name: &str,
        draft: &contract::GrabDraft,
        checked: &[bool],
        schedule: &contract::GrabScheduleDraft,
        reply: Sender<Result<(), contract::InvokeError>>,
    ) {
        let lang = self.lang();
        let Some(library) = self.ports.library.clone() else {
            let _ = reply.send(Err(crate::errors::no_projects(lang)));
            return;
        };
        if name.trim().is_empty() {
            let _ = reply.send(Err(crate::errors::invalid(lang, "Type a name for the project.")));
            return;
        }
        let request = match crate::grabber::request(draft, &self.state.grab.request) {
            Ok(r) => r,
            Err(e) => {
                let _ = reply.send(Err(crate::errors::grab(&e)));
                return;
            }
        };
        let project = match crate::grabber::with_schedule(
            crate::grabber::project(name, request, &self.state.grab, checked),
            schedule,
        ) {
            Ok(p) => p,
            Err(e) => {
                let _ = reply.send(Err(crate::errors::queue(&e)));
                return;
            }
        };
        let (system, outbox, password) =
            (Arc::clone(&self.ports.system), self.outbox.clone(), draft.login_password.clone());
        thread::spawn(move || {
            let result = (|| {
                if let Some(user) = project.request.login_user.clone().filter(|_| !password.is_empty()) {
                    let target = project_login_target(&project.request.start, &user);
                    system.save_credential(&target, &user, &password).map_err(|e| crate::errors::files(&e))?;
                }
                library.save(&project).map_err(|e| crate::errors::files(&e))
            })();
            if result.is_ok() {
                let _ = outbox.send(Inbox::Ui(UiMessage::GrabSaved(Box::new(project))));
                let _ = outbox.send(Inbox::Ui(UiMessage::GrabLibraryRead(Some(Box::new(read(&*library))))));
            }
            let _ = reply.send(result);
        });
    }

    /// Send `command`, as a command's result.
    fn sent(&self, lang: &'static crate::i18n::Language, command: Command) -> Result<(), contract::InvokeError> {
        if self.send(command) {
            Ok(())
        } else {
            Err(crate::errors::offline(lang))
        }
    }

    fn handle(&mut self, msg: Inbox) {
        match msg {
            Inbox::Ui(msg) => {
                // The categories panel is a setting both windows share.
                if matches!(msg, UiMessage::ToggleCategories) {
                    if let Some(mut settings) = self.state.settings.clone() {
                        settings.categories_panel = !self.state.categories_panel;
                        self.send(Command::SetSettings { settings });
                    }
                }
                if let UiMessage::Daemon { event, .. } = &msg {
                    if matches!(**event, Event::Show) {
                        windows::show_main(&self.app, self.lang());
                    }
                    if matches!(**event, Event::Quit) {
                        self.app.exit(0);
                    }
                }
                let picked = matches!(&msg, UiMessage::Daemon { event, .. } if matches!(**event, Event::PickLinks { .. } | Event::PickGallery { .. }));
                apply(&mut self.state, msg);
                // A page's links are probed for their sizes, and a gallery
                // page read, as their windows open.
                if picked {
                    if let Some(l) = self.state.asks.links.as_ref().filter(|l| l.info.is_empty()) {
                        let (request, links) = (l.request, crate::extras::link_probes(l));
                        self.send(Command::ProbeLinks { request, links });
                    }
                    if let Some(g) = self.state.asks.gallery.as_ref().filter(|g| g.result.is_none()) {
                        let (request, page, user_agent) = (g.request, g.page.clone(), g.user_agent.clone());
                        self.send(Command::ReadGallery { request, page, user_agent });
                    }
                }
            }
            Inbox::Subscribe { label, channel } => {
                tracing::info!(window = %label, "window subscribed");
                self.subscribers.insert(label, Subscriber { channel, seq: 0, last: None, look: None });
            }
            Inbox::Closed(text) => {
                let Some(label) = Label::parse(&text) else { return };
                self.subscribers.remove(&label);
                self.opened.remove(&label);
                self.titles.remove(&label);
                if self.closing.remove(&label) {
                    return;
                }
                // Closing the drop target hides it until shown again.
                if label == Label::DropTarget {
                    self.set_drop_target(false);
                }
                // Closing the countdown cancels it; closing the links stops
                // their probes.
                if label == Label::Countdown && self.state.asks.countdown.is_some() {
                    self.send(Command::CancelCountdown);
                }
                if label == Label::PickLinks {
                    self.send(Command::StopProbeLinks);
                }
                // Closing Download File Info without answering cancels the
                // download, as its Cancel button does.
                if let Label::SaveAs(id) = label {
                    let waiting = self.state.table.get(id).is_some_and(|e| e.status == Status::Confirming);
                    if waiting && !self.state.answered.contains(&id) {
                        self.send(Command::Cancel { id });
                        apply(&mut self.state, UiMessage::Answered(id));
                    }
                }
                apply(&mut self.state, UiMessage::WindowClosed(text));
            }
            Inbox::Act(action) => {
                let plan = actions::plan(&self.state, action);
                for m in plan.messages {
                    apply(&mut self.state, m);
                }
                for c in plan.commands {
                    self.send(c);
                }
            }
            Inbox::Send { command, reply } => {
                let sent = self.send(*command);
                if let Some(r) = reply {
                    let _ = r.send(sent);
                }
            }
            Inbox::Answer { answer, reply } => {
                let id = answer.id;
                let sent = self.send(udm_application::daemon::Command::Confirm(*answer));
                if sent {
                    apply(&mut self.state, UiMessage::Answered(id));
                }
                let _ = reply.send(sent);
            }
            Inbox::Remove { delete_files, dont_ask } => {
                for c in actions::remove(&self.state, delete_files) {
                    self.send(c);
                }
                apply(&mut self.state, UiMessage::RemoveClosed);
                if let Some(mut settings) = self.state.settings.clone().filter(|_| dont_ask) {
                    settings.confirm_remove = false;
                    self.send(Command::SetSettings { settings });
                }
            }
            Inbox::NoDoneDialogs => {
                if let Some(mut settings) = self.state.settings.clone() {
                    settings.notify = Notify::Toast;
                    self.send(Command::SetSettings { settings });
                }
            }
            Inbox::Open { id, how } => {
                let Some(path) = self.state.table.get(id).and_then(|e| e.path.clone()) else { return };
                let opener = Arc::clone(&self.ports.opener);
                // Opening starts another program; a worker keeps the state
                // thread free.
                thread::spawn(move || match how {
                    OpenHow::File => opener.open(&path),
                    OpenHow::Folder => match Path::new(&path).exists() {
                        true => opener.reveal(&path),
                        false => {
                            if let Some(dir) = Path::new(&path).parent() {
                                opener.open(&dir.display().to_string());
                            }
                        }
                    },
                });
            }
            Inbox::Limit { id, bytes_per_sec, remember } => {
                self.send(Command::SetSpeedLimit { id, bytes_per_sec, remember: Some(remember) });
            }
            Inbox::OnDone { id, on_done } => {
                self.send(Command::SetOnDone { id, on_done });
            }
            Inbox::OpenWindow(kind) => {
                let label = match kind {
                    contract::OpenWindow::AddUrl => Label::AddUrl,
                    contract::OpenWindow::About => Label::About,
                    // Options edits the daemon's settings, so it waits for them.
                    contract::OpenWindow::Options if self.state.settings.is_none() => return,
                    contract::OpenWindow::Options => Label::Options,
                    contract::OpenWindow::Scheduler => Label::Scheduler,
                    contract::OpenWindow::Batch => Label::Batch,
                    contract::OpenWindow::Main => Label::Main,
                    contract::OpenWindow::Grabber => {
                        if !self.state.grab.library_read {
                            self.read_library();
                        }
                        Label::Grabber
                    }
                };
                if let Err(e) = windows::open(&self.app, label, self.lang()) {
                    tracing::warn!(window = %label, error = %e, "a window did not open");
                }
            }
            Inbox::SaveOptions { draft, reply } => {
                let lang = self.lang();
                let result = match self.state.settings.as_ref().map(|s| crate::options::settings(&draft, s)) {
                    None => Err(crate::errors::offline(lang)),
                    Some(Err(e)) => Err(crate::errors::options(lang, &e)),
                    Some(Ok(settings)) => match self.send(Command::SetSettings { settings }) {
                        true => Ok(()),
                        false => Err(crate::errors::offline(lang)),
                    },
                };
                let _ = reply.send(result);
            }
            Inbox::SaveQueue { draft, reply } => {
                let lang = self.lang();
                let current = self.state.queues.iter().find(|q| q.id.get() == draft.id.0).cloned();
                let result = match current.map(|q| crate::scheduler::queue(&draft, &q)) {
                    None => Err(crate::errors::gone(lang)),
                    Some(Err(e)) => Err(crate::errors::queue(&e)),
                    Some(Ok(queue)) => self.sent(lang, Command::SaveQueue { queue }),
                };
                let _ = reply.send(result);
            }
            Inbox::CreateQueue { name, reply } => {
                let lang = self.lang();
                let queue = udm_domain::queue::Queue::new(udm_domain::id::QueueId::NEW, name.trim());
                let result = match queue.validate() {
                    Err(e) => Err(crate::errors::queue(&e)),
                    Ok(()) => self.sent(lang, Command::SaveQueue { queue }),
                };
                let _ = reply.send(result);
            }
            Inbox::SaveQuota { quota, reply } => {
                let lang = self.lang();
                let result = match self.state.settings.as_ref().map(|s| crate::scheduler::quota_settings(quota, s)) {
                    None => Err(crate::errors::offline(lang)),
                    Some(Err(e)) => Err(crate::errors::settings(&e)),
                    Some(Ok(settings)) => self.sent(lang, Command::SetSettings { settings }),
                };
                let _ = reply.send(result);
            }
            Inbox::MoveQueued { id, offset } => {
                if let Some((queue, position)) = crate::scheduler::moved(&self.state.queues, id, offset) {
                    self.send(Command::MoveToQueue { id, queue, position: Some(position) });
                }
            }
            Inbox::GrabStart { draft, reply } => self.grab_start(&draft, reply),
            Inbox::GrabBegin(request) => {
                apply(&mut self.state, UiMessage::GrabStarted(request.clone()));
                self.send(Command::Grab(*request));
            }
            Inbox::GrabStop => {
                self.send(Command::CancelGrab);
            }
            Inbox::GrabAdd { indices, queue, start, reply } => {
                let _ = reply.send(self.grab_add(&indices, queue, start));
            }
            Inbox::GrabTemplate { name, reply } => {
                let lang = self.lang();
                let template =
                    self.state.grab.library.as_ref().and_then(|l| l.templates.iter().find(|t| t.name == name));
                let _ = reply
                    .send(template.map(|t| crate::grabber::draft(&t.request)).ok_or_else(|| crate::errors::gone(lang)));
            }
            Inbox::GrabOpen { name, reply } => {
                let lang = self.lang();
                let Some(library) = self.ports.library.clone() else {
                    let _ = reply.send(Err(crate::errors::no_projects(lang)));
                    return;
                };
                let outbox = self.outbox.clone();
                thread::spawn(move || {
                    let result = match library.load(&name) {
                        Ok(project) => {
                            let opened = contract::GrabOpened {
                                name: project.name.clone(),
                                draft: crate::grabber::draft(&project.request),
                            };
                            let _ = outbox.send(Inbox::Ui(UiMessage::GrabOpened(Box::new(project))));
                            Ok(opened)
                        }
                        Err(e) => Err(crate::errors::files(&e)),
                    };
                    let _ = reply.send(result);
                });
            }
            Inbox::GrabSave { name, draft, checked, schedule, reply } => {
                self.grab_save(&name, &draft, &checked, &schedule, reply)
            }
            Inbox::GrabSaveTemplate { name, draft, reply } => {
                let lang = self.lang();
                let (Some(library), true) = (self.ports.library.clone(), !name.trim().is_empty()) else {
                    let _ = reply.send(Err(crate::errors::no_projects(lang)));
                    return;
                };
                match crate::grabber::request(&draft, &self.state.grab.request) {
                    Err(e) => {
                        let _ = reply.send(Err(crate::errors::grab(&e)));
                    }
                    Ok(request) => {
                        let outbox = self.outbox.clone();
                        thread::spawn(move || {
                            let result =
                                library.save_template(name.trim(), &request).map_err(|e| crate::errors::files(&e));
                            let _ = outbox.send(Inbox::Ui(UiMessage::GrabLibraryRead(Some(Box::new(read(&*library))))));
                            let _ = reply.send(result);
                        });
                    }
                }
            }
            Inbox::GrabSaveFilters { filters, reply } => {
                let lang = self.lang();
                let Some(library) = self.ports.library.clone() else {
                    let _ = reply.send(Err(crate::errors::no_projects(lang)));
                    return;
                };
                let outbox = self.outbox.clone();
                thread::spawn(move || {
                    let result = library.save_filters(&filters).map_err(|e| crate::errors::files(&e));
                    let _ = outbox.send(Inbox::Ui(UiMessage::GrabLibraryRead(Some(Box::new(read(&*library))))));
                    let _ = reply.send(result);
                });
            }
            Inbox::GrabAddOnClose(on) => {
                if let Some(mut settings) = self.state.settings.clone() {
                    settings.grab_add_on_close = on;
                    self.send(Command::SetSettings { settings });
                }
            }
            Inbox::Find { input, reply } => {
                let found = crate::tools::find(&self.state, &input);
                if let Some(id) = found {
                    apply(&mut self.state, UiMessage::Select(vec![id]));
                }
                let _ = reply.send(found.is_some());
            }
            Inbox::ExportText { scope, format, reply } => {
                let entries = crate::tools::export_entries(&self.state, scope);
                let format = match format {
                    contract::ExportFormat::Ef2 => ListFormat::Ef2,
                    contract::ExportFormat::Text => ListFormat::Text,
                    contract::ExportFormat::Json => ListFormat::Json,
                };
                let _ = reply.send((self.ports.lists.export(format, &entries), entries.len()));
            }
            Inbox::Limiter { draft, reply } => {
                let lang = self.lang();
                let result = match self.state.settings.as_ref().map(|s| crate::tools::limiter_settings(&draft, s)) {
                    None => Err(crate::errors::offline(lang)),
                    Some(Err(e)) => Err(crate::errors::settings(&e)),
                    Some(Ok(settings)) => self.sent(lang, Command::SetSettings { settings }),
                };
                let _ = reply.send(result);
            }
            Inbox::Category { original, input, reply } => {
                let lang = self.lang();
                let result = match self
                    .state
                    .settings
                    .as_ref()
                    .map(|s| crate::tools::with_category(s, original.as_deref(), input.as_deref()))
                {
                    None => Err(crate::errors::offline(lang)),
                    Some(Err(e)) => Err(crate::errors::category(lang, &e)),
                    Some(Ok(settings)) => self.sent(lang, Command::SetSettings { settings }),
                };
                let _ = reply.send(result);
            }
            Inbox::BrowseCategory(name) => {
                let settings = self.state.settings.clone().unwrap_or_default();
                let (folders, opener) = (Arc::clone(&self.ports.folders), Arc::clone(&self.ports.opener));
                thread::spawn(move || {
                    let folder = settings.category_folder(&name, &folders.downloads_dir());
                    opener.open(&folder.display().to_string());
                });
            }
            Inbox::QueueSelection(queue) => {
                for id in self.state.selected.clone() {
                    self.send(match queue {
                        Some(queue) => Command::MoveToQueue { id, queue, position: None },
                        None => Command::RemoveFromQueue { id },
                    });
                }
            }
            Inbox::Properties { id, input, reply } => {
                let lang = self.lang();
                let result = match self.state.table.get(id) {
                    None => Err(crate::errors::gone(lang)),
                    Some(e) => {
                        let changes = crate::tools::properties_changes(&input, e);
                        self.sent(lang, Command::SetProperties { id, changes })
                    }
                };
                let _ = reply.send(result);
            }
            Inbox::AddItems { items, queue, start, reply } => {
                let _ = reply.send(self.add_items(items, queue, start));
            }
            Inbox::AddLinks { indices, queue, start, reply } => {
                let items: Vec<_> = self
                    .state
                    .links
                    .as_ref()
                    .map(|l| indices.iter().filter_map(|i| l.items.get(*i as usize).cloned()).collect())
                    .unwrap_or_default();
                let result = self.add_items(items, queue, start);
                if result.is_ok() {
                    apply(&mut self.state, UiMessage::Links(None));
                }
                let _ = reply.send(result);
            }
            Inbox::DropTarget(on) => self.set_drop_target(on),
            Inbox::Login { user, password, save, reply } => {
                let _ = reply.send(self.answer_login(&user, &password, save));
            }
            Inbox::AddPicks { label, input, reply } => {
                let _ = reply.send(self.add_picks(label, &input));
            }
            Inbox::OpenPackage(id) => {
                let folder = self.state.asks.packages.iter().find(|p| p.id == id).and_then(|p| match &p.unpack {
                    udm_domain::package::Unpack::Done { folder, .. } => Some(folder.clone()),
                    _ => p.folder.clone(),
                });
                if let Some(folder) = folder {
                    let opener = Arc::clone(&self.ports.opener);
                    thread::spawn(move || opener.open(&folder));
                }
            }
            Inbox::DismissRelease => {
                if let Some(Ok(Some(r))) = &self.state.asks.update {
                    self.state
                        .layout_rest
                        .insert("dismissed_release".into(), serde_json::Value::String(r.version.clone()));
                }
            }
            Inbox::TipClosed { next, on_startup } => {
                apply(&mut self.state, UiMessage::TipSeen);
                if let Some(mut settings) = self.state.settings.clone() {
                    settings.next_tip = next as usize % crate::tips::TIPS.len();
                    settings.tips_on_startup = on_startup;
                    self.send(Command::SetSettings { settings });
                }
            }
            Inbox::Reconnect => self.connect(),
        }
    }

    /// Open the windows the state asks for and close those it no longer
    /// does.
    fn reconcile(&mut self) {
        let wanted = windows::wanted(&self.state);
        let lang = self.lang();
        for label in wanted.difference(&self.opened.clone()) {
            match windows::open(&self.app, *label, lang) {
                Ok(_) => {
                    self.opened.insert(*label);
                }
                Err(e) => tracing::warn!(window = %label, error = %e, "a window did not open"),
            }
        }
        let stale: Vec<Label> = self.opened.difference(&wanted).copied().collect();
        for label in stale {
            self.closing.insert(label);
            self.opened.remove(&label);
            windows::close(&self.app, label);
        }
    }

    /// The local time's offset from UTC, rounded to the quarter hour.
    fn offset(&self) -> i64 {
        let now = self.ports.clock.now();
        let local = now.day * 1440 + i64::from(now.minute);
        let utc = self.ports.times.unix_now() as i64 / 60;
        ((local - utc) as f64 / 15.0).round() as i64 * 15
    }

    fn publish(&mut self) {
        let today = {
            let (y, m, d) = udm_domain::queue::civil_from_days(self.ports.clock.now().day);
            format!("{y:04}-{m:02}-{d:02}")
        };
        let ctx = Ctx {
            lang: self.lang(),
            offset: self.offset(),
            today,
            now: Instant::now(),
            downloads: self.downloads.clone(),
        };
        let index = crate::i18n::LANGUAGES.iter().position(|l| l.code == ctx.lang.code).unwrap_or(0);
        self.ports.language.store(index, Ordering::Relaxed);
        let look = projection::look(&self.state);
        let mut gone = Vec::new();
        let mut titles = Vec::new();
        for (label, sub) in &mut self.subscribers {
            let Some(view) = projection::view(&ctx, &self.state, label) else {
                let _ = sub.channel.send(Update::Gone { seq: sub.seq });
                gone.push(*label);
                continue;
            };
            if let View::Download(d) = &view {
                titles.push((*label, d.title.clone()));
            }
            let update = match &sub.last {
                None => Some(Update::Snapshot { seq: sub.seq, look: look.clone(), view: view.clone() }),
                Some(before) => {
                    let new_look = (sub.look.as_ref() != Some(&look)).then(|| look.clone());
                    let patch = match (before, &view) {
                        (View::Main(a), View::Main(b)) => main::diff(a, b).map(Patch::Main),
                        (a, b) if a != b => Some(Patch::Whole(b.clone())),
                        _ => None,
                    };
                    match (patch, new_look) {
                        (None, None) => None,
                        (Some(patch), look) => Some(Update::Patch { seq: sub.seq, look, patch }),
                        (None, Some(look)) => {
                            Some(Update::Patch { seq: sub.seq, look: Some(look), patch: Patch::Whole(view.clone()) })
                        }
                    }
                }
            };
            if let Some(update) = update {
                if sub.channel.send(update).is_err() {
                    gone.push(*label);
                    continue;
                }
                sub.seq = sub.seq.wrapping_add(1);
            }
            sub.last = Some(view);
            sub.look = Some(look.clone());
        }
        for label in gone {
            self.subscribers.remove(&label);
        }
        for (label, title) in titles {
            if self.titles.get(&label) != Some(&title) {
                windows::set_title(&self.app, label, &title);
                self.titles.insert(label, title);
            }
        }
    }
}

/// What the projects folder holds. Reads files, so it runs on a worker.
fn read(library: &dyn GrabLibrary) -> crate::grabber::Library {
    crate::grabber::Library {
        base: library.template(),
        templates: library.templates(),
        filters: library.filters(),
        recent: library.recent(),
    }
}
