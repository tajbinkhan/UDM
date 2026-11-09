//! Keeping the download quota's count in `%LOCALAPPDATA%\Pullbay\quota.json`
//! (the minutes of the longest window), so it survives a restart. The
//! counting rules are `pullbay_domain::quota`.
//!
//! `QuotaHold` is the adapter for `QuotaState`: it reads the count the
//! download store keeps, remembers what the quota stopped, and tells the
//! clients the quota's state.

use pullbay_domain::bytes::ByteCount;
use pullbay_domain::id::{DownloadId, QueueId};
use pullbay_domain::sync::LockExt;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use pullbay_application::daemon::Event;
use pullbay_domain::ports::QuotaState;
use pullbay_domain::quota::QuotaNotice;
use pullbay_domain::quota::Usage;

use crate::protocol::Outgoing;
use crate::settings::write_atomic;
use crate::store::{DownloadStore, StoreEvents};

/// The count saved at `path`, or none when it is missing or unreadable.
pub fn load(path: &Path) -> Usage {
    fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Save `usage` at `path` through a temporary file and a rename.
pub fn save(usage: &Usage, path: &Path) -> io::Result<()> {
    let bytes = serde_json::to_vec(usage).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_atomic(path, &bytes)
}

/// `quota.json` in Pullbay's data folder (`%LOCALAPPDATA%\Pullbay` on Windows).
pub fn quota_path() -> io::Result<PathBuf> {
    Ok(crate::paths::data_dir()?.join("quota.json"))
}

/// Minutes since 1970.
pub fn now_minute() -> u64 {
    crate::state::unix_now() / 60
}

/// The `quota` event that tells the clients `notice`.
pub fn notice_event(n: QuotaNotice) -> Event {
    Event::from(n)
}

/// The `quota` message that tells the clients `notice`.
pub fn notice_message(n: QuotaNotice) -> Outgoing {
    Outgoing::Quota { used: n.used, limit: n.limit, paused: n.paused, resumes_at: n.resumes_at }
}

/// What the quota stopped (queues and downloads), while it holds them.
type Held = Option<(Vec<QueueId>, HashSet<DownloadId>)>;

/// The quota's count, what it stopped, and whether it warned.
pub struct QuotaHold {
    store: Arc<DownloadStore>,
    events: Arc<dyn StoreEvents>,
    held: Mutex<Held>,
    /// The nearly-reached warning was sent for the current window.
    warned: AtomicBool,
}

impl QuotaHold {
    /// A hold over `store`'s count that announces through `events`.
    pub fn new(store: Arc<DownloadStore>, events: Arc<dyn StoreEvents>) -> Self {
        QuotaHold { store, events, held: Mutex::new(None), warned: AtomicBool::new(false) }
    }

    /// Each change replaces or extends the held lists whole, so a lock
    /// poisoned by a panicking thread still holds a usable value.
    fn held(&self) -> MutexGuard<'_, Held> {
        self.held.lock_or_recover()
    }
}

impl QuotaState for QuotaHold {
    fn used(&self, now: u64, window: u64) -> ByteCount {
        self.store.quota().used(now, window)
    }

    fn frees_at(&self, now: u64, window: u64, limit: ByteCount) -> u64 {
        self.store.quota().frees_at(now, window, limit)
    }

    fn paused(&self) -> bool {
        self.held().is_some()
    }

    fn hold(&self, queues: &[QueueId], downloads: &[DownloadId]) {
        let mut held = self.held();
        let (q, d) = held.get_or_insert_with(Default::default);
        q.extend(queues);
        d.extend(downloads);
    }

    fn release(&self) -> (Vec<QueueId>, HashSet<DownloadId>) {
        self.held().take().unwrap_or_default()
    }

    fn warned(&self) -> bool {
        self.warned.load(Ordering::Relaxed)
    }

    fn set_warned(&self, warned: bool) {
        self.warned.store(warned, Ordering::Relaxed);
    }

    fn announce(&self, notice: QuotaNotice) {
        self.events.announce(&notice_message(notice));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Ended, StoreFiles};

    #[derive(Default)]
    struct Heard(Mutex<Vec<Outgoing>>);

    impl StoreEvents for Heard {
        fn announce(&self, event: &Outgoing) {
            self.0.lock().unwrap().push(event.clone());
        }
        fn ended(&self, _: Ended) {}
        fn wake(&self) {}
    }

    #[test]
    fn the_hold_keeps_what_it_stopped_until_released() {
        let heard = Arc::new(Heard::default());
        let (store, ..) = DownloadStore::open(StoreFiles::default(), Arc::clone(&heard) as _);
        let hold = QuotaHold::new(Arc::new(store), Arc::clone(&heard) as _);
        assert!(!hold.paused());
        hold.hold(&[QueueId::new(1)], &[DownloadId::new(10)]);
        hold.hold(&[QueueId::new(2)], &[DownloadId::new(10), DownloadId::new(11)]);
        assert!(hold.paused());
        assert_eq!(
            hold.release(),
            (vec![QueueId::new(1), QueueId::new(2)], HashSet::from([10, 11].map(DownloadId::new)))
        );
        assert!(!hold.paused());
        assert_eq!(hold.release(), (vec![], HashSet::new()));
        assert_eq!(hold.used(100, 60), ByteCount::ZERO);

        hold.set_warned(true);
        assert!(hold.warned());
        hold.announce(QuotaNotice {
            used: ByteCount::new(5),
            limit: ByteCount::new(9),
            paused: true,
            resumes_at: Some(3),
        });
        let said = heard.0.lock().unwrap().clone();
        assert!(
            matches!(said.as_slice(), [Outgoing::Quota { used, limit, paused: true, resumes_at: Some(3) }] if used.get() == 5 && limit.get() == 9)
        );
    }

    #[test]
    fn survives_a_save_and_load() {
        let mut u = Usage::default();
        u.started(DownloadId::new(1), ByteCount::ZERO);
        u.progress(DownloadId::new(1), ByteCount::new(42), 5);
        let path = std::env::temp_dir().join(format!("pullbay_quota_{}.json", std::process::id()));
        save(&u, &path).unwrap();
        let mut back = load(&path);
        assert_eq!(back.used(5, 60), ByteCount::new(42));
        let _ = fs::remove_file(&path);
        assert_eq!(load(&path), Usage::default(), "a missing file is no usage");
    }
}
