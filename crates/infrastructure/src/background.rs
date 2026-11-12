//! The daemon's background threads. Each takes what it runs as a closure,
//! which the daemon's wiring passes in, and owns only the timing: when to
//! run, how long to wait, and what the operating system must hear.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use tracing::{info, warn};
use udm_domain::queue::Moment;

use crate::{logging, power};

/// How often the scheduler runs when nothing wakes it.
const SCHEDULER_POLL: Duration = Duration::from_secs(5);
/// How often the scheduler moves an overgrown log aside.
const LOG_TRIM_EVERY: Duration = Duration::from_secs(60 * 60);
/// How long the history writer waits for a burst of changes to end.
const HISTORY_DEBOUNCE: Duration = Duration::from_secs(1);
/// How often the awake keeper looks again.
const AWAKE_POLL: Duration = Duration::from_secs(2);
/// How often the idle monitor looks again.
const IDLE_POLL: Duration = Duration::from_secs(5);
/// The daemon exits after this long with nothing to do.
pub const IDLE_EXIT: Duration = Duration::from_secs(60);

/// Run the scheduler: `pass` whenever something wakes it and every few
/// seconds for the clock. `pass` returns when the computer should wake for
/// the next scheduled start, which the scheduler sets the wake timer to.
/// The thread ends when every sender of `wake` is gone.
pub fn spawn_scheduler(wake: Receiver<()>, mut pass: impl FnMut() -> Option<Moment> + Send + 'static) {
    thread::spawn(move || {
        // Wakes the computer for the next scheduled start; kept here so the
        // handle lives as long as the scheduler.
        let mut timer = power::WakeTimer::new();
        // The daemon trimmed the logs as it won the bind; this keeps them
        // bounded while it runs for days.
        let mut trimmed = Instant::now();
        loop {
            match wake.recv_timeout(SCHEDULER_POLL) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => {
                    while wake.try_recv().is_ok() {}
                    let next = pass();
                    if trimmed.elapsed() >= LOG_TRIM_EVERY {
                        logging::trim();
                        trimmed = Instant::now();
                    }
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
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

/// Run `handle` on each item `items` brings, on a thread of its own, so
/// slow work (a toast, a virus scan) never holds up the sender.
pub fn spawn_each<T: Send + 'static>(items: Receiver<T>, mut handle: impl FnMut(T) + Send + 'static) {
    thread::spawn(move || {
        while let Ok(item) = items.recv() {
            handle(item);
        }
    });
}

/// Run `save` shortly after `dirty` hears of a change, batching bursts of
/// changes (progress, a cleared list) into one run.
pub fn spawn_debounced(dirty: Receiver<()>, mut save: impl FnMut() + Send + 'static) {
    thread::spawn(move || {
        while dirty.recv().is_ok() {
            thread::sleep(HISTORY_DEBOUNCE);
            while dirty.try_recv().is_ok() {}
            save();
        }
    });
}

/// Hold the computer awake while `wanted` says so. One thread owns the
/// request, since Windows ties it to the thread that made it.
pub fn spawn_awake_keeper(wanted: impl Fn() -> bool + Send + 'static) {
    thread::spawn(move || {
        let mut held = false;
        loop {
            let want = wanted();
            if want != held {
                power::keep_awake(want);
                held = want;
            }
            thread::sleep(AWAKE_POLL);
        }
    });
}

/// Exit the process once `idle` has said so for `IDLE_EXIT`, after running
/// `before_exit`, so a daemon started by the browser does not linger. The
/// daemon passes its service's `may_exit`, which a server's daemon
/// (`Seat::Server`) never says, so that one runs until it is stopped.
/// `joined` counts the clients that have connected so far
/// (`server::Clients::joined`), so a client that comes and goes between two
/// looks still starts the wait again.
pub fn spawn_idle_monitor(
    idle: impl Fn() -> bool + Send + 'static,
    joined: impl Fn() -> u64 + Send + 'static,
    before_exit: impl FnOnce() + Send + 'static,
) {
    thread::spawn(move || {
        let mut watch = IdleWatch::new(joined());
        loop {
            thread::sleep(IDLE_POLL);
            if watch.look(idle(), joined(), Instant::now()) {
                info!(seconds = IDLE_EXIT.as_secs(), "idle, exiting");
                before_exit();
                std::process::exit(0);
            }
        }
    });
}

/// The idle monitor's count: since when the daemon has had nothing to do, as
/// far as its looks every `IDLE_POLL` can tell.
#[derive(Debug)]
struct IdleWatch {
    since: Option<Instant>,
    joined: u64,
}

impl IdleWatch {
    fn new(joined: u64) -> Self {
        IdleWatch { since: None, joined }
    }

    /// Take one look: whether the daemon is `idle` now, and how many clients
    /// had `joined` by then. True when it has been idle for `IDLE_EXIT`. A
    /// client that joined since the last look means the daemon was busy in
    /// between, so the wait starts again from `now` even if it left already.
    fn look(&mut self, idle: bool, joined: u64, now: Instant) -> bool {
        let came = joined != self.joined;
        self.joined = joined;
        if !idle {
            self.since = None;
            return false;
        }
        match self.since {
            Some(since) if !came => now.duration_since(since) >= IDLE_EXIT,
            _ => {
                self.since = Some(now);
                false
            }
        }
    }
}

/// Exit the process cleanly, after running `before_exit`, when it is asked to
/// stop (`SIGTERM`, as `docker stop` and service managers send, or `SIGINT`,
/// Ctrl+C), for a server's daemon, which nothing else stops. Without this the
/// signal ends the process at once: the list is saved a second after each
/// change anyway, so this only keeps the last second's progress. Downloads
/// still running load as interrupted next time, as after a crash. Programs
/// the daemon starts get the default handlers back when they start. Does
/// nothing on Windows, whose servers stop the daemon some other way.
pub fn spawn_stop_on_signal(before_exit: impl FnOnce() + Send + 'static) {
    #[cfg(unix)]
    {
        use std::sync::atomic::Ordering;
        if let Err(e) = stop_signal::install() {
            warn!(error = %e, "could not listen for the stop signal");
            return;
        }
        thread::spawn(move || {
            while !stop_signal::ASKED.load(Ordering::SeqCst) {
                thread::sleep(STOP_POLL);
            }
            info!("asked to stop, exiting");
            before_exit();
            std::process::exit(0);
        });
    }
    #[cfg(not(unix))]
    drop(before_exit);
}

/// How often the stop watch looks for a stop signal.
#[cfg(unix)]
const STOP_POLL: Duration = Duration::from_millis(200);

/// The handler `spawn_stop_on_signal` installs. It only sets a flag, which
/// is all a signal handler may safely do.
#[cfg(unix)]
mod stop_signal {
    use std::io;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Set once a stop signal came.
    pub static ASKED: AtomicBool = AtomicBool::new(false);

    extern "C" fn on_signal(_: libc::c_int) {
        ASKED.store(true, Ordering::SeqCst);
    }

    /// Handle `SIGTERM` and `SIGINT` with [`on_signal`].
    pub fn install() -> io::Result<()> {
        for signal in [libc::SIGTERM, libc::SIGINT] {
            // SAFETY: `action` is zeroed and then filled in before use, the
            // handler only stores to an atomic (async-signal-safe), and
            // `sigaction` is given valid pointers for the call.
            let failed = unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
                action.sa_flags = libc::SA_RESTART;
                libc::sigemptyset(&mut action.sa_mask);
                libc::sigaction(signal, &action, std::ptr::null_mut()) != 0
            };
            if failed {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn the_daemon_exits_after_a_minute_with_nothing_to_do() {
        let start = Instant::now();
        let mut watch = IdleWatch::new(0);
        assert!(!watch.look(true, 0, start), "the first idle look starts the wait");
        assert!(!watch.look(true, 0, start + IDLE_EXIT - IDLE_POLL));
        assert!(watch.look(true, 0, start + IDLE_EXIT));
    }

    #[test]
    fn a_busy_look_starts_the_wait_again() {
        let start = Instant::now();
        let mut watch = IdleWatch::new(0);
        watch.look(true, 0, start);
        watch.look(false, 1, start + IDLE_POLL);
        assert!(!watch.look(true, 1, start + IDLE_EXIT), "idle again only since this look");
        assert!(watch.look(true, 1, start + IDLE_EXIT * 2));
    }

    #[test]
    fn a_client_that_came_and_went_between_looks_starts_the_wait_again() {
        // A `udm-cli get` of a small file connects, downloads and leaves
        // within one IDLE_POLL; every look still sees the daemon idle.
        let start = Instant::now();
        let mut watch = IdleWatch::new(0);
        watch.look(true, 0, start);
        let after_client = start + IDLE_EXIT - IDLE_POLL;
        assert!(!watch.look(true, 1, after_client), "the client that joined counts as activity");
        assert!(!watch.look(true, 1, start + IDLE_EXIT), "a minute has not passed since it");
        assert!(watch.look(true, 1, after_client + IDLE_EXIT));
    }

    #[test]
    fn a_burst_of_changes_is_saved_once() {
        let (dirty, dirty_rx) = channel();
        let (saved, saves) = channel();
        spawn_debounced(dirty_rx, move || saved.send(()).unwrap());
        for _ in 0..5 {
            dirty.send(()).unwrap();
        }
        saves.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(saves.recv_timeout(HISTORY_DEBOUNCE * 2).is_err(), "one save for the burst");
    }

    #[test]
    fn the_scheduler_runs_when_woken_and_ends_with_its_senders() {
        let (wake, wake_rx) = channel();
        let (ran, runs) = channel();
        spawn_scheduler(wake_rx, move || {
            let _ = ran.send(());
            None
        });
        wake.send(()).unwrap();
        runs.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(wake);
        // With no senders left the thread returns and drops `ran`.
        let until = Instant::now() + Duration::from_secs(10);
        while runs.recv_timeout(Duration::from_millis(100)).is_ok() {
            assert!(Instant::now() < until, "the scheduler kept running");
        }
    }
}
