//! The daemon's background threads. Each takes what it runs as a closure,
//! which the daemon's wiring passes in, and owns only the timing: when to
//! run, how long to wait, and what the operating system must hear.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use pullbay_domain::queue::Moment;
use tracing::{info, warn};

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
pub fn spawn_idle_monitor(idle: impl Fn() -> bool + Send + 'static, before_exit: impl FnOnce() + Send + 'static) {
    thread::spawn(move || {
        let mut idle_since: Option<Instant> = None;
        loop {
            thread::sleep(IDLE_POLL);
            match (idle(), idle_since) {
                (true, None) => idle_since = Some(Instant::now()),
                (true, Some(t)) if t.elapsed() >= IDLE_EXIT => {
                    info!(seconds = IDLE_EXIT.as_secs(), "idle, exiting");
                    before_exit();
                    std::process::exit(0);
                }
                (false, _) => idle_since = None,
                _ => {}
            }
        }
    });
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
