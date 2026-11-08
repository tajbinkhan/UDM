//! The window that closes to the tray and opens again (Linux).
//!
//! Linux cannot hide a window: Wayland has no hidden windows, and a hidden
//! X11 window gets no frames, which every dialog needs. So closing the
//! window with "Closing the window keeps UIDM in the tray" on really closes
//! it there, while this process stays with its tray icon and its daemon
//! connection, and `run` opens a new window when something asks for it
//! (`win::show`, `win::wake`). Each window has an egui context of its own,
//! so the threads that outlive one window reach the current one here.
//!
//! Windows cloaks its window instead (see `win.rs`), and macOS keeps the
//! window minimized, since its tray icon needs the main thread's loop.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use eframe::egui;

/// Whether closing the window can leave UIDM in the tray on this system.
pub const CLOSES_TO_TRAY: bool = cfg!(target_os = "linux");

/// The open window's context; `None` while the window is closed.
static CONTEXT: Mutex<Option<egui::Context>> = Mutex::new(None);

#[derive(Default)]
struct Parked {
    /// The window is closed and UIDM waits in the tray.
    parked: bool,
    /// Something asked for the window while it was closed.
    wanted: bool,
}

static PARKED: Mutex<Parked> = Mutex::new(Parked { parked: false, wanted: false });
static WANTED: Condvar = Condvar::new();

/// Lock `m`. Both hold plain values that a panicking thread cannot leave
/// half-changed.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The window just opened with `ctx`.
pub fn opened(ctx: &egui::Context) {
    *lock(&CONTEXT) = Some(ctx.clone());
    let mut p = lock(&PARKED);
    p.parked = false;
    p.wanted = false;
}

/// The open window's context, if a window is open.
pub fn context() -> Option<egui::Context> {
    lock(&CONTEXT).clone()
}

/// Ask the open window for a frame, so it takes what a thread sent it.
/// Nothing to do while it is closed: the waiting loop takes the news.
pub fn repaint() {
    if let Some(ctx) = context() {
        ctx.request_repaint();
    }
}

/// The window is closing to the tray; `run` waits for `want` once it is
/// gone.
pub fn park() {
    *lock(&CONTEXT) = None;
    let mut p = lock(&PARKED);
    p.parked = true;
    p.wanted = false;
}

/// Whether the window is closed and UIDM waits in the tray.
pub fn parked() -> bool {
    lock(&PARKED).parked
}

/// Ask for the window back. Returns whether it was closed, in which case
/// `run` opens a new one; otherwise the caller brings the open one forward.
pub fn want() -> bool {
    let mut p = lock(&PARKED);
    if !p.parked {
        return false;
    }
    p.wanted = true;
    WANTED.notify_all();
    true
}

/// Wait up to `timeout` for `want`. Returns whether the window is wanted.
pub fn wait(timeout: Duration) -> bool {
    let p = lock(&PARKED);
    let (p, _) = WANTED.wait_timeout_while(p, timeout, |p| !p.wanted).unwrap_or_else(PoisonError::into_inner);
    p.wanted
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test, since the state is the process's.
    #[test]
    fn a_closed_window_is_wanted_back_once_asked_for() {
        // An open window is brought forward by its caller.
        opened(&egui::Context::default());
        assert!(!want());
        assert!(context().is_some());

        park();
        assert!(parked());
        assert!(context().is_none(), "no window to repaint while closed");
        assert!(!wait(Duration::from_millis(10)), "nothing asked for it yet");

        // Asked for from another thread, as the tray and the daemon do.
        let asker = std::thread::spawn(want);
        assert!(asker.join().unwrap());
        assert!(wait(Duration::from_secs(5)));

        opened(&egui::Context::default());
        assert!(!parked());
        assert!(!wait(Duration::from_millis(10)), "the new window is not wanted again");
    }
}
