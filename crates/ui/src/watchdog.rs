//! A thread that watches the window's frame and the daemon from outside:
//! a frame that is stuck cannot notice it from inside.
//!
//! - The frame marks when it starts and ends ([`Beats::frame`]). One that
//!   has not ended after [`FREEZE`] is a freeze: the thread writes a report
//!   at once, while the window cannot, so a window that is then closed by
//!   force still leaves one for the next start. Once the frame ends, the
//!   window hears how long it took and offers to keep the report.
//! - While connected, the thread pings the daemon every [`PING_EVERY`]. A
//!   ping unanswered after [`SILENT`] means the daemon stopped answering
//!   with its connection still open: the window offers to save a report.
//!
//! An idle window draws no frames and is never taken for a frozen one: only
//! a frame that began and did not end counts. Whether something stalled is
//! `pullbay_domain::report::Watch`'s rule; writing the report is
//! `pullbay_application::report::SaveReport`'s.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use pullbay_application::daemon::{Command, DaemonClient};
use pullbay_application::report::SaveReport;
use pullbay_domain::report::{Kind, Program, Stall, Watch};

use crate::state::UiMessage;

/// A frame that has not ended after this long is a freeze.
pub const FREEZE: Duration = Duration::from_secs(10);
/// A ping unanswered for this long means the daemon stopped answering.
pub const SILENT: Duration = Duration::from_secs(30);
/// How often the daemon is pinged while connected.
pub const PING_EVERY: Duration = Duration::from_secs(5);
/// How often the thread looks.
const LOOK_EVERY: Duration = Duration::from_secs(1);

/// What the frame and the connection tell the watching thread. Times are
/// milliseconds since `start`, plus one, so 0 can mean none.
pub struct Beats {
    start: Instant,
    /// When the frame under way began.
    frame: AtomicU64,
    /// When the ping awaiting its answer was sent.
    ping: AtomicU64,
    connected: AtomicBool,
}

impl Default for Beats {
    fn default() -> Self {
        Beats {
            start: Instant::now(),
            frame: AtomicU64::new(0),
            ping: AtomicU64::new(0),
            connected: AtomicBool::new(false),
        }
    }
}

/// Marks a frame as under way until it is dropped.
pub struct Frame<'a>(&'a Beats);

impl Drop for Frame<'_> {
    fn drop(&mut self) {
        self.0.frame.store(0, Ordering::SeqCst);
    }
}

impl Beats {
    fn now(&self) -> u64 {
        self.start.elapsed().as_millis() as u64 + 1
    }

    /// A frame begins; it ends when the guard is dropped.
    pub fn frame(&self) -> Frame<'_> {
        self.frame.store(self.now(), Ordering::SeqCst);
        Frame(self)
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

    fn busy(at: &AtomicU64) -> Option<u64> {
        Some(at.load(Ordering::SeqCst)).filter(|&t| t != 0)
    }
}

/// Start the watching thread. It sends what it sees to `outbox`.
pub fn start(beats: Arc<Beats>, daemon: Arc<dyn DaemonClient>, outbox: Sender<UiMessage>) {
    // Without the thread the window goes on as before, unwatched.
    let _ = thread::Builder::new().name("watchdog".into()).spawn(move || {
        let mut frame = Watch::new(FREEZE.as_millis() as u64);
        let mut silent = Watch::new(SILENT.as_millis() as u64);
        let mut draft = None;
        let mut last_ping = 0;
        loop {
            thread::sleep(LOOK_EVERY);
            let now = beats.now();
            match frame.check(now, Beats::busy(&beats.frame)) {
                Some(Stall::Began { .. }) => {
                    let what = format!("The window's frame had not finished after {} seconds.", FREEZE.as_secs());
                    let at = crate::ports::ports().unix_now() * 1000;
                    // Saving logs it; one that fails is tried again if the
                    // user asks for the report.
                    draft = SaveReport::new(&*crate::ports::ports().reports)
                        .run(Program::Window, Kind::Freeze, &what, at)
                        .ok();
                }
                Some(Stall::Ended { lasted_ms }) => {
                    if outbox.send(UiMessage::FrameStalled { lasted_ms, draft: draft.take() }).is_err() {
                        return;
                    }
                    crate::reopen::repaint();
                }
                None => {}
            }
            if beats.connected.load(Ordering::SeqCst)
                && Beats::busy(&beats.ping).is_none()
                && now.saturating_sub(last_ping) >= PING_EVERY.as_millis() as u64
            {
                last_ping = now;
                beats.ping.store(now, Ordering::SeqCst);
                if !daemon.send(Command::Ping) {
                    beats.ping.store(0, Ordering::SeqCst);
                }
            }
            let message = match silent.check(now, Beats::busy(&beats.ping)) {
                Some(Stall::Began { since }) => {
                    Some(UiMessage::DaemonSilent { secs: now.saturating_sub(since) / 1000 })
                }
                Some(Stall::Ended { .. }) => Some(UiMessage::DaemonAnswering),
                None => None,
            };
            if let Some(message) = message {
                if outbox.send(message).is_err() {
                    return;
                }
                crate::reopen::repaint();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_under_way_until_its_guard_drops() {
        let beats = Beats::default();
        assert_eq!(Beats::busy(&beats.frame), None);
        {
            let _frame = beats.frame();
            assert!(Beats::busy(&beats.frame).is_some());
        }
        assert_eq!(Beats::busy(&beats.frame), None);
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
        assert!(!beats.connected.load(Ordering::SeqCst));
    }
}
