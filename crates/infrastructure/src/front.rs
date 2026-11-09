//! What the daemon puts in front of the user: the desktop window and the
//! system's notifications.
//!
//! `Windows` is how these adapters reach the connected windows; the daemon's
//! client list implements it. `Notifier` shows a notification when a
//! download finishes or fails and handles its buttons. `Front` is the
//! adapter for `Attention`, and `AnswerWait` for `Answers`: it waits for a
//! window to answer a save-as dialog, and without one starts the download
//! as proposed.

use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use pullbay_domain::download::{Answer, DownloadEntry, Scan, Status};
use pullbay_domain::ports::{Answers, Attention};
use tracing::{info, warn};

use crate::live_settings::LiveSettings;
use crate::notify::{self, Toast};
use crate::protocol::Outgoing;
use crate::store::{DownloadStore, StoreEvents};
use pullbay_domain::settings::Sound;
use pullbay_domain::settings::{FileExists, Notify, Settings};

/// How long a download waits for a window to show its save-as dialog before
/// it starts with the proposed name and folder.
pub const CONFIRM_FALLBACK: Duration = Duration::from_secs(20);
/// Notifications kept alive so their buttons keep working.
const KEEP_TOASTS: usize = 32;

/// Something the daemon does in answer, through its use cases.
pub type Hook<T> = Arc<dyn Fn(T) + Send + Sync>;

/// The connected clients, as these adapters need them: `announce` reaches
/// every client.
pub trait Windows: StoreEvents {
    /// Whether a desktop window is connected.
    fn window_open(&self) -> bool;
}

/// Start the desktop window from this program's folder.
pub fn open_window() -> io::Result<()> {
    crate::platform::spawn_reaped(
        crate::platform::program_command("pullbay-gui")?
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
}

/// Bring the window forward: ask an open one to show itself, or start one.
pub fn show_window(windows: &dyn Windows) {
    if windows.window_open() {
        windows.announce(&Outgoing::Show);
    } else if let Err(e) = open_window() {
        warn!(error = %e, "could not open the window");
    }
}

/// The notifications shown for finished and failed downloads.
pub struct Notifier {
    store: Arc<DownloadStore>,
    windows: Arc<dyn Windows>,
    /// Recent notifications; dropping one can stop its buttons working.
    toasts: Mutex<VecDeque<notify::Shown>>,
    /// Retries a download, from a notification's button.
    retry: Hook<u64>,
}

impl Notifier {
    pub fn new(store: Arc<DownloadStore>, windows: Arc<dyn Windows>, retry: Hook<u64>) -> Arc<Self> {
        Arc::new(Notifier { store, windows, toasts: Mutex::new(VecDeque::new()), retry })
    }

    /// Play the sound the settings choose for download `id` finishing (or
    /// with `failed`, failing), and show a notification when they ask for
    /// one, or when the download asked for its completion dialog and no
    /// window is open to show it.
    pub fn notify(self: &Arc<Self>, id: u64, failed: bool, settings: &Settings) {
        let Some(entry) = self.store.table().get(id).cloned() else {
            return;
        };
        if failed && settings.sound_on_failed {
            notify::play(Sound::Failed, settings.sound_file(Sound::Failed).as_deref());
        }
        if !failed && settings.sound_on_done {
            notify::play(Sound::Done, settings.sound_file(Sound::Done).as_deref());
        }
        let dialog = !failed && entry.on_done.is_some_and(|o| o.dialog);
        let toast = match settings.notify {
            _ if dialog => !self.windows.window_open(),
            Notify::Off => false,
            Notify::Toast => true,
            // An open window shows its own dialog.
            Notify::Dialog => !self.windows.window_open(),
        };
        if !toast {
            return;
        }
        let me = Arc::clone(self);
        match notify::show(&toast_for(&entry, failed), move |arg| me.on_toast(&arg)) {
            Ok(t) => {
                let mut toasts = self.toasts();
                toasts.push_back(t);
                while toasts.len() > KEEP_TOASTS {
                    toasts.pop_front();
                }
            }
            Err(e) => warn!(error = %e, "could not show a notification"),
        }
    }

    /// Only pushes and pops change the list, so a poisoned lock still holds
    /// a usable one.
    fn toasts(&self) -> MutexGuard<'_, VecDeque<notify::Shown>> {
        self.toasts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Act on a click on a notification or one of its buttons.
    fn on_toast(&self, arg: &str) {
        let (action, id) = match arg.split_once(':') {
            Some((a, id)) => (a, id.parse::<u64>().ok()),
            None => (arg, None),
        };
        let entry = id.and_then(|id| self.store.table().get(id).cloned());
        let path = entry.as_ref().and_then(|e| e.path.clone());
        match (action, path) {
            // A file with a threat is opened only from the window, which asks.
            ("open", Some(_)) if matches!(entry.as_ref().and_then(|e| e.scan.as_ref()), Some(Scan::Threat { .. })) => {
                show_window(&*self.windows)
            }
            ("open", Some(path)) => {
                let _ = crate::platform::open(path.as_ref());
            }
            ("show", Some(path)) => {
                let _ = crate::platform::reveal(Path::new(&path));
            }
            ("retry", _) => {
                if let Some(id) = id {
                    (self.retry)(id);
                }
            }
            _ => show_window(&*self.windows),
        }
    }
}

/// What a notification says about `e`.
pub fn toast_for(e: &DownloadEntry, failed: bool) -> Toast {
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
            lines: vec![name, "The virus scan found a threat. Open Pullbay to decide what to do.".into()],
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

/// The windows and notifications.
pub struct Front {
    notifier: Arc<Notifier>,
    settings: Arc<LiveSettings>,
    windows: Arc<dyn Windows>,
}

impl Front {
    pub fn new(notifier: Arc<Notifier>, settings: Arc<LiveSettings>, windows: Arc<dyn Windows>) -> Self {
        Front { notifier, settings, windows }
    }
}

impl Attention for Front {
    fn show_done(&self, id: u64) {
        if self.windows.window_open() {
            self.windows.announce(&Outgoing::ShowDone { id });
        } else {
            // Without a window, a notification stands in for the dialog,
            // whatever the notification setting.
            let mut settings = self.settings.get();
            settings.notify = Notify::Toast;
            settings.sound_on_done = false;
            self.notifier.notify(id, false, &settings);
        }
    }

    fn show_window(&self) {
        show_window(&*self.windows);
    }
}

/// Save-as dialogs waiting for a window, each with a timer of its own.
pub struct AnswerWait {
    store: Arc<DownloadStore>,
    windows: Arc<dyn Windows>,
    /// Answers a dialog, through the use case.
    confirm: Hook<Answer>,
    /// How long a dialog waits for a window.
    patience: Duration,
}

impl AnswerWait {
    pub fn new(store: Arc<DownloadStore>, windows: Arc<dyn Windows>, confirm: Hook<Answer>) -> Self {
        AnswerWait { store, windows, confirm, patience: CONFIRM_FALLBACK }
    }

    /// Wait for a window to answer download `id`'s save-as dialog, opening
    /// one if none is open. If none connects in time, the download starts as
    /// proposed: `fallback` decides what happens if its name is taken,
    /// `None` leaving that to the setting.
    pub fn wait(&self, id: u64, fallback: Option<FileExists>) {
        if !self.windows.window_open() {
            if let Err(e) = open_window() {
                warn!(error = %e, "could not open the window");
            }
        }
        let (store, windows, confirm, patience) =
            (Arc::clone(&self.store), Arc::clone(&self.windows), Arc::clone(&self.confirm), self.patience);
        thread::spawn(move || {
            thread::sleep(patience);
            let status = store.table().get(id).map(|e| e.status);
            if status == Some(Status::Confirming) && !windows.window_open() {
                info!(download_id = id, "no window to confirm; starting it as proposed");
                // A "download later" asked only about a duplicate keeps waiting.
                let request = store.requests().get(&id).cloned().unwrap_or_default();
                confirm(request.fallback_answer(id, fallback));
            }
        });
    }
}

impl Answers for AnswerWait {
    fn await_answer(&self, id: u64) {
        self.wait(id, None);
    }

    fn await_new_name(&self, id: u64) {
        self.wait(id, Some(FileExists::Rename));
    }
}

/// The `Notifications` adapter: the shared `Notifier`.
pub struct Notices(pub Arc<Notifier>);

impl pullbay_domain::ports::Notifications for Notices {
    fn notify(&self, id: u64, failed: bool, settings: &Settings) {
        self.0.notify(id, failed, settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Ended, StoreFiles};
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct FakeWindows {
        open: AtomicBool,
        said: Mutex<Vec<Outgoing>>,
    }

    impl StoreEvents for FakeWindows {
        fn announce(&self, event: &Outgoing) {
            self.said.lock().unwrap().push(event.clone());
        }
        fn ended(&self, _: Ended) {}
        fn wake(&self) {}
    }

    impl Windows for FakeWindows {
        fn window_open(&self) -> bool {
            self.open.load(Ordering::Relaxed)
        }
    }

    fn store(windows: &Arc<FakeWindows>) -> Arc<DownloadStore> {
        Arc::new(DownloadStore::open(StoreFiles::default(), Arc::clone(windows) as _).0)
    }

    #[test]
    fn an_open_window_shows_the_completion_dialog_itself() {
        let windows = Arc::new(FakeWindows { open: AtomicBool::new(true), ..FakeWindows::default() });
        let store = store(&windows);
        let notifier = Notifier::new(Arc::clone(&store), Arc::clone(&windows) as _, Arc::new(|_| {}));
        let front =
            Front::new(notifier, Arc::new(LiveSettings::new(Settings::default(), None)), Arc::clone(&windows) as _);
        front.show_done(7);
        front.show_window();
        let said = windows.said.lock().unwrap().clone();
        assert!(matches!(said.as_slice(), [Outgoing::ShowDone { id: 7 }, Outgoing::Show]));
    }

    #[test]
    fn a_dialog_nobody_answers_starts_as_proposed() {
        let windows = Arc::new(FakeWindows { open: AtomicBool::new(true), ..FakeWindows::default() });
        let store = store(&windows);
        store.publish(&Outgoing::Added { id: 3, url: "http://x/a".into() });
        let _ = store.update(3, |e| {
            e.status = Status::Confirming;
            true
        });
        let answered = Arc::new(Mutex::new(Vec::new()));
        let heard = Arc::clone(&answered);
        let mut wait = AnswerWait::new(
            Arc::clone(&store),
            Arc::clone(&windows) as _,
            Arc::new(move |a: Answer| heard.lock().unwrap().push(a.id)),
        );
        wait.patience = Duration::from_millis(10);
        // The window that was open closes before answering.
        wait.await_answer(3);
        windows.open.store(false, Ordering::Relaxed);
        for _ in 0..200 {
            if !answered.lock().unwrap().is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(*answered.lock().unwrap(), [3]);
    }

    #[test]
    fn notifications_name_the_file_and_offer_its_buttons() {
        let windows = Arc::new(FakeWindows::default());
        let store = store(&windows);
        store.publish(&Outgoing::Added { id: 4, url: "http://x/a.zip".into() });
        let e = store
            .update(4, |e| {
                e.path = Some(std::env::temp_dir().join("a.zip").display().to_string());
                e.total = Some(2048);
                true
            })
            .unwrap();
        let done = toast_for(&e, false);
        assert_eq!(done.title, "Download complete");
        assert_eq!(done.lines[0], "a.zip");
        assert!(done.lines[1].starts_with("2.0 KB in "));
        assert_eq!(done.buttons[0].1, "open:4");
        let failed = toast_for(&DownloadEntry { error: Some("gone".into()), ..e }, true);
        assert_eq!(failed.buttons, [("Retry".to_string(), "retry:4".to_string())]);
        assert_eq!(human_bytes(10), "10 B");
    }
}
