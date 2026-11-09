//! `PowerOff`, the adapter for `PowerControl`: closing Pullbay, and the
//! countdown before sleeping, hibernating or shutting down that the windows
//! show and may stop. The power actions themselves are `power::perform`.
//! `RunEnds` is the adapter for `QueueFinish`: what happens when a queue's
//! run is done.

use pullbay_domain::sync::LockExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use pullbay_domain::ports::{PowerControl, QueueFinish};
use pullbay_domain::queue::{PowerRequest, RunEnd, WhenDone};
use tracing::{info, warn};

use pullbay_application::daemon::Event;

use crate::live_settings::LiveSettings;
use crate::notify;
use crate::power;
use crate::protocol::Outgoing;
use crate::store::StoreEvents;
use pullbay_domain::settings::Sound;

/// Set to make sleep, hibernate and shut down only log what they would do
/// (for testing the countdown without losing the machine).
pub const POWER_DRY_RUN_ENV: &str = "PULLBAY_POWER_DRY_RUN";

/// How often a countdown's thread looks whether it was stopped.
const COUNTDOWN_POLL: Duration = Duration::from_millis(250);

/// A countdown before sleeping, hibernating or shutting down.
struct Countdown {
    request: PowerRequest,
    deadline: Instant,
    canceled: Arc<AtomicBool>,
}

/// Something the daemon does for a power step: save its lists, or bring a
/// window forward.
pub type Hook = Box<dyn Fn() + Send + Sync>;

/// Closing Pullbay and the power countdown.
pub struct PowerOff {
    /// Reaches every client.
    events: Arc<dyn StoreEvents>,
    /// Writes the history now: the machine may go away.
    save: Hook,
    /// Shows a window, opening one if none is open.
    show_window: Hook,
    countdown: Mutex<Option<Countdown>>,
}

impl PowerOff {
    pub fn new(events: Arc<dyn StoreEvents>, save: Hook, show_window: Hook) -> Self {
        PowerOff { events, save, show_window, countdown: Mutex::new(None) }
    }

    /// The slot is replaced or cleared whole, so a lock poisoned by a
    /// panicking thread still holds a usable value.
    fn slot(&self) -> MutexGuard<'_, Option<Countdown>> {
        self.countdown.lock_or_recover()
    }

    /// What the countdown running is for, if one runs.
    pub fn running(&self) -> Option<PowerRequest> {
        self.slot().as_ref().filter(|c| !c.canceled.load(Ordering::Relaxed)).map(|c| c.request.clone())
    }

    /// The countdown still running, as the message a window joining now
    /// needs. Forgets a countdown that ran out or was stopped.
    pub fn countdown_message(&self) -> Option<Event> {
        let mut slot = self.slot();
        let c = slot.as_ref()?;
        if c.canceled.load(Ordering::Relaxed) || Instant::now() >= c.deadline {
            *slot = None;
            return None;
        }
        Some(Event::Countdown {
            action: c.request.action,
            seconds: c.deadline.saturating_duration_since(Instant::now()).as_secs() as u32,
            queue: c.request.queue.clone(),
            file: c.request.file.clone(),
        })
    }
}

/// Carry out `action`, or with `PULLBAY_POWER_DRY_RUN` set only log it.
fn power_off(action: WhenDone, force_close: bool) {
    if std::env::var_os(POWER_DRY_RUN_ENV).is_some() {
        info!(action = action.label(), "dry run: would do it");
        return;
    }
    info!(action = action.label(), "power");
    if let Err(e) = power::perform(action, force_close) {
        warn!(action = action.label(), error = %e, "power action failed");
    }
}

impl PowerControl for PowerOff {
    fn exit(&self) {
        self.events.announce(&Outgoing::Quit);
        (self.save)();
        // Other downloads may still run; the idle monitor ends the daemon
        // once they are done and every window has closed.
    }

    fn start_countdown(&self, request: &PowerRequest, seconds: u32) {
        let canceled = Arc::new(AtomicBool::new(false));
        let deadline = Instant::now() + Duration::from_secs(seconds as u64);
        let countdown = Countdown { request: request.clone(), deadline, canceled: Arc::clone(&canceled) };
        if let Some(old) = self.slot().replace(countdown) {
            old.canceled.store(true, Ordering::Relaxed);
        }
        self.events.announce(&Outgoing::Countdown {
            action: request.action,
            seconds,
            queue: request.queue.clone(),
            file: request.file.clone(),
        });
        (self.show_window)();
        // Save now: the machine may go away when the countdown ends.
        (self.save)();
        let (action, force_close) = (request.action, request.force_close);
        thread::spawn(move || {
            while Instant::now() < deadline {
                if canceled.load(Ordering::Relaxed) {
                    return;
                }
                thread::sleep(COUNTDOWN_POLL);
            }
            if canceled.load(Ordering::Relaxed) {
                return;
            }
            power_off(action, force_close);
        });
    }

    fn take_countdown(&self) -> Option<PowerRequest> {
        let c = self.slot().take()?;
        c.canceled.store(true, Ordering::Relaxed);
        Some(c.request)
    }

    fn countdown_canceled(&self) {
        self.events.announce(&Outgoing::CountdownCanceled);
    }

    fn power_off(&self, action: WhenDone, force_close: bool) {
        power_off(action, force_close);
    }
}

/// Does what a finished queue, or one finished download, asks of the
/// computer (through the use case): the action, what it is for (the
/// queue's name), and whether a shutdown may close programs.
pub type WhenDoneHook = Box<dyn Fn(WhenDone, &str, bool) + Send + Sync>;

/// Open the program or file a queue names for the end of its run, as
/// Explorer would on a double-click.
pub fn open_after_run(file: &str) {
    pullbay_domain::ports::FileOpener::open(&crate::platform::Desktop, file);
}

/// The end of a queue's run: its sound, the file it opens, and its
/// when-done action.
pub struct RunEnds {
    settings: Arc<LiveSettings>,
    when_done: WhenDoneHook,
}

impl RunEnds {
    pub fn new(settings: Arc<LiveSettings>, when_done: WhenDoneHook) -> Self {
        RunEnds { settings, when_done }
    }
}

impl QueueFinish for RunEnds {
    fn run_ended(&self, queue: &str, end: &RunEnd) {
        let settings = self.settings.get();
        if settings.sound_on_queue_done {
            notify::play(Sound::QueueDone, settings.sound_file(Sound::QueueDone).as_deref());
        }
        if let Some(file) = &end.open {
            open_after_run(file);
        }
        (self.when_done)(end.action, queue, end.force_close);
    }
}

impl pullbay_application::daemon::Countdowns for PowerOff {
    fn countdown(&self) -> Option<Event> {
        self.countdown_message()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Ended;
    use std::sync::atomic::AtomicUsize;

    #[derive(Default)]
    struct Heard {
        said: Mutex<Vec<Outgoing>>,
        saves: AtomicUsize,
        shown: AtomicUsize,
    }

    struct Listener(Arc<Heard>);

    impl StoreEvents for Listener {
        fn announce(&self, event: &Outgoing) {
            self.0.said.lock().unwrap().push(event.clone());
        }
        fn ended(&self, _: Ended) {}
        fn wake(&self) {}
    }

    fn power() -> (PowerOff, Arc<Heard>) {
        let heard = Arc::new(Heard::default());
        let (a, b) = (Arc::clone(&heard), Arc::clone(&heard));
        let power = PowerOff::new(
            Arc::new(Listener(Arc::clone(&heard))),
            Box::new(move || {
                a.saves.fetch_add(1, Ordering::Relaxed);
            }),
            Box::new(move || {
                b.shown.fetch_add(1, Ordering::Relaxed);
            }),
        );
        (power, heard)
    }

    fn request(action: WhenDone) -> PowerRequest {
        PowerRequest { action, queue: "Main".into(), file: None, force_close: false }
    }

    #[test]
    fn a_finished_run_does_what_it_asks() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let heard = Arc::clone(&asked);
        let settings = pullbay_domain::settings::Settings { sound_on_queue_done: false, ..Default::default() };
        let ends = RunEnds::new(
            Arc::new(LiveSettings::new(settings, None)),
            Box::new(move |action, queue: &str, force| heard.lock().unwrap().push((action, queue.to_string(), force))),
        );
        let end = RunEnd { action: WhenDone::Hibernate, open: None, force_close: true };
        ends.run_ended("Main", &end);
        assert_eq!(*asked.lock().unwrap(), [(WhenDone::Hibernate, "Main".to_string(), true)]);
    }

    #[test]
    fn exiting_tells_the_windows_and_saves() {
        let (power, heard) = power();
        power.exit();
        assert!(matches!(heard.said.lock().unwrap().as_slice(), [Outgoing::Quit]));
        assert_eq!(heard.saves.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_countdown_shows_a_window_and_can_be_taken_back() {
        let (power, heard) = power();
        // Far off, and taken back at once: this test must not sleep the machine.
        power.start_countdown(&request(WhenDone::Sleep), 3600);
        assert_eq!(heard.shown.load(Ordering::Relaxed), 1);
        assert_eq!(heard.saves.load(Ordering::Relaxed), 1);
        assert_eq!(power.running().map(|r| r.action), Some(WhenDone::Sleep));
        assert!(matches!(power.countdown_message(), Some(Event::Countdown { action: WhenDone::Sleep, .. })));

        // A second countdown replaces the first.
        power.start_countdown(&request(WhenDone::ShutDown), 3600);
        assert_eq!(power.take_countdown().map(|r| r.action), Some(WhenDone::ShutDown));
        assert!(power.running().is_none());
        assert!(power.countdown_message().is_none());
        assert!(power.take_countdown().is_none());
        power.countdown_canceled();
        assert!(matches!(heard.said.lock().unwrap().last(), Some(Outgoing::CountdownCanceled)));
    }
}
