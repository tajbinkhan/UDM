//! A thread that watches the window from outside, since work that is stuck
//! cannot notice it from inside (AGENTS.md section 7.3):
//!
//! - The state thread marks when it starts and ends each message
//!   ([`Beats::work`]). One that has not ended after [`FREEZE`] is a
//!   freeze: the thread writes a report at once, so a window then closed by
//!   force still leaves one, and offers to keep it once the work ends.
//! - Each window sends a heartbeat while it is visible ([`Beats::beat`]).
//!   A visible window silent for [`WINDOW_FREEZE`] stopped answering, and
//!   is reported the same way.
//! - While connected, the thread pings the daemon every [`PING_EVERY`]. A
//!   ping unanswered after [`SILENT`] means the daemon stopped answering
//!   with its connection still open: the window offers to save a report.
//!
//! Whether something stalled is `udm_domain::report::Watch`'s rule; writing
//! the report is `udm_application::report::SaveReport`'s.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use udm_application::daemon::{Command, DaemonClient};
use udm_application::report::SaveReport;
use udm_domain::ports::{ProblemReports, Timestamps};
use udm_domain::report::{Kind, Program, SavedReport, Stall, Watch};
use udm_domain::sync::LockExt;

use crate::actor::Inbox;
use crate::state::UiMessage;

/// State-thread work that has not ended after this long is a freeze.
pub const FREEZE: Duration = Duration::from_secs(10);
/// A visible window silent this long stopped answering. Longer than
/// [`FREEZE`]: a webview may slow the timers of a window behind others.
pub const WINDOW_FREEZE: Duration = Duration::from_secs(20);
/// A ping unanswered for this long means the daemon stopped answering.
pub const SILENT: Duration = Duration::from_secs(30);
/// How often the daemon is pinged while connected.
pub const PING_EVERY: Duration = Duration::from_secs(5);
/// How often the thread looks.
const LOOK_EVERY: Duration = Duration::from_secs(1);

/// What the state thread, the windows and the connection tell the watching
/// thread. Times are milliseconds since `start`, plus one, so 0 can mean
/// none.
pub struct Beats {
    start: Instant,
    /// When the state thread's work under way began.
    work: AtomicU64,
    /// When the ping awaiting its answer was sent.
    ping: AtomicU64,
    connected: AtomicBool,
    /// Each visible window's last heartbeat, by label.
    windows: Mutex<HashMap<String, u64>>,
}

impl Default for Beats {
    fn default() -> Self {
        Beats {
            start: Instant::now(),
            work: AtomicU64::new(0),
            ping: AtomicU64::new(0),
            connected: AtomicBool::new(false),
            windows: Mutex::new(HashMap::new()),
        }
    }
}

/// Marks the state thread's work as under way until it is dropped.
pub struct Work<'a>(&'a Beats);

impl Drop for Work<'_> {
    fn drop(&mut self) {
        self.0.work.store(0, Ordering::SeqCst);
    }
}

impl Beats {
    fn now(&self) -> u64 {
        self.start.elapsed().as_millis() as u64 + 1
    }

    /// The state thread's work begins; it ends when the guard is dropped.
    pub fn work(&self) -> Work<'_> {
        self.work.store(self.now(), Ordering::SeqCst);
        Work(self)
    }

    /// The connection was made, or went.
    pub fn connected(&self, up: bool) {
        self.connected.store(up, Ordering::SeqCst);
        self.ping.store(0, Ordering::SeqCst);
    }

    /// The daemon answered a ping.
    pub fn answered(&self) {
        self.ping.store(0, Ordering::SeqCst);
    }

    /// The window `label` answers, and is `visible` or not; a hidden one is
    /// not watched, since its webview may stop its timers.
    pub fn beat(&self, label: &str, visible: bool) {
        let now = self.now();
        let mut windows = self.windows.lock_or_recover();
        if visible {
            windows.insert(label.to_string(), now);
        } else {
            windows.remove(label);
        }
    }

    /// The window `label` closed.
    pub fn forget(&self, label: &str) {
        self.windows.lock_or_recover().remove(label);
    }

    /// The oldest heartbeat among the visible windows.
    fn quietest(&self) -> Option<u64> {
        self.windows.lock_or_recover().values().min().copied()
    }

    fn busy(at: &AtomicU64) -> Option<u64> {
        Some(at.load(Ordering::SeqCst)).filter(|&t| t != 0)
    }
}

/// The ports the watching thread reaches.
pub struct Watched {
    pub beats: Arc<Beats>,
    pub daemon: Arc<dyn DaemonClient>,
    pub reports: Arc<dyn ProblemReports>,
    pub times: Arc<dyn Timestamps>,
}

/// One watch on work that should end, and the report written while it is
/// stuck.
struct Stuck {
    watch: Watch,
    draft: Option<SavedReport>,
    what: &'static str,
}

impl Stuck {
    fn new(limit: Duration, what: &'static str) -> Self {
        Stuck { watch: Watch::new(limit.as_millis() as u64), draft: None, what }
    }

    /// Look at the work; the freeze to offer once it ended.
    fn look(&mut self, now: u64, since: Option<u64>, w: &Watched) -> Option<UiMessage> {
        match self.watch.check(now, since)? {
            Stall::Began { .. } => {
                let at = w.times.unix_now() * 1000;
                // Saving logs it; one that fails is tried again if the user
                // asks for the report.
                self.draft = SaveReport::new(&*w.reports).run(Program::Window, Kind::Freeze, self.what, at).ok();
                None
            }
            Stall::Ended { lasted_ms } => Some(UiMessage::Froze { lasted_ms, draft: self.draft.take() }),
        }
    }
}

/// Start the watching thread. It sends what it sees to `outbox`.
pub fn start(w: Watched, outbox: Sender<Inbox>) {
    let spawned = thread::Builder::new().name("udm-watchdog".into()).spawn(move || {
        let mut work = Stuck::new(FREEZE, "The window's state thread had not finished a message after 10 seconds.");
        let mut windows = Stuck::new(WINDOW_FREEZE, "A visible window had not answered for 20 seconds.");
        let mut silent = Watch::new(SILENT.as_millis() as u64);
        let mut last_ping = 0;
        loop {
            thread::sleep(LOOK_EVERY);
            let beats = &w.beats;
            let now = beats.now();
            let mut messages = Vec::new();
            messages.extend(work.look(now, Beats::busy(&beats.work), &w));
            messages.extend(windows.look(now, beats.quietest(), &w));
            if beats.connected.load(Ordering::SeqCst)
                && Beats::busy(&beats.ping).is_none()
                && now.saturating_sub(last_ping) >= PING_EVERY.as_millis() as u64
            {
                last_ping = now;
                beats.ping.store(now, Ordering::SeqCst);
                if !w.daemon.send(Command::Ping) {
                    beats.ping.store(0, Ordering::SeqCst);
                }
            }
            match silent.check(now, Beats::busy(&beats.ping)) {
                Some(Stall::Began { since }) => {
                    messages.push(UiMessage::DaemonSilent(now.saturating_sub(since) / 1000))
                }
                Some(Stall::Ended { .. }) => messages.push(UiMessage::DaemonAnswering),
                None => {}
            }
            for m in messages {
                if outbox.send(Inbox::Ui(m)).is_err() {
                    return;
                }
            }
        }
    });
    if let Err(e) = spawned {
        // Without the thread the window goes on as before, unwatched.
        tracing::warn!(error = %e, "the watchdog did not start");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_is_under_way_until_its_guard_drops() {
        let beats = Beats::default();
        {
            let _work = beats.work();
            assert!(Beats::busy(&beats.work).is_some());
        }
        assert_eq!(Beats::busy(&beats.work), None);
    }

    #[test]
    fn only_visible_windows_are_watched() {
        let beats = Beats::default();
        beats.beat("main", true);
        beats.beat("options", true);
        assert!(beats.quietest().is_some());
        beats.beat("main", false);
        beats.forget("options");
        assert_eq!(beats.quietest(), None);
    }

    #[test]
    fn an_answer_or_a_new_connection_clears_the_ping() {
        let beats = Beats::default();
        beats.ping.store(5, Ordering::SeqCst);
        beats.answered();
        assert_eq!(Beats::busy(&beats.ping), None);
        beats.ping.store(5, Ordering::SeqCst);
        beats.connected(false);
        assert_eq!(Beats::busy(&beats.ping), None);
    }
}
