//! The download quota: "no more than N MB every N hours". The daemon counts
//! the bytes every download receives, per minute; these are the counting
//! rules and the decision what the count asks for. Keeping the count across
//! restarts is `udm_infrastructure::quota`.

use std::collections::{HashMap, VecDeque};

use crate::bytes::ByteCount;
use crate::id::DownloadId;
use serde::{Deserialize, Serialize};

/// Bytes received, by minute since 1970.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    minutes: VecDeque<(u64, ByteCount)>,
    /// Each running download's progress at its last report, so only new
    /// bytes count. Not saved: a restart starts every download afresh.
    #[serde(skip)]
    last: HashMap<DownloadId, ByteCount>,
}

impl Usage {
    /// A download (re)started with `done` bytes already in hand; those do
    /// not count again.
    pub fn started(&mut self, id: DownloadId, done: ByteCount) {
        self.last.insert(id, done);
    }

    /// Download `id` reports `done` bytes at minute `now`: count what is new.
    pub fn progress(&mut self, id: DownloadId, done: ByteCount, now: u64) {
        let before = self.last.insert(id, done).unwrap_or(done);
        let new = done.saturating_sub(before);
        if new.is_zero() {
            return;
        }
        match self.minutes.back_mut() {
            Some((m, bytes)) if *m == now => *bytes += new,
            _ => self.minutes.push_back((now, new)),
        }
    }

    /// The download stopped; forget its progress.
    pub fn ended(&mut self, id: DownloadId) {
        self.last.remove(&id);
    }

    /// Bytes received in the `window` minutes up to `now`. Older minutes are
    /// dropped.
    pub fn used(&mut self, now: u64, window: u64) -> ByteCount {
        let start = now.saturating_sub(window.saturating_sub(1));
        while self.minutes.front().is_some_and(|(m, _)| *m < start) {
            self.minutes.pop_front();
        }
        self.minutes.iter().map(|(_, b)| *b).sum()
    }

    /// The first minute after `now` at which the window holds less than
    /// `limit` bytes again, if no more arrive.
    pub fn frees_at(&mut self, now: u64, window: u64, limit: ByteCount) -> u64 {
        let mut used = self.used(now, window);
        for (m, b) in &self.minutes {
            if used < limit {
                break;
            }
            used -= *b;
            // Minute `m` leaves the window `window` minutes after it.
            if used < limit {
                return m + window;
            }
        }
        now
    }
}

/// The quota the settings ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaLimit {
    /// Bytes allowed in the window.
    pub bytes: ByteCount,
    /// The window, in minutes.
    pub window: u64,
    /// Warn when nine tenths are used.
    pub warn: bool,
}

/// What the quota asks for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaStep {
    /// Nothing changes.
    Keep,
    /// The window allows downloads again: start what the quota stopped.
    Resume,
    /// The quota is reached: stop everything. `first` is the moment it was
    /// reached; later passes stop what started since.
    Pause { first: bool },
    /// Nine tenths are used, for the first time in this window.
    Warn,
}

/// Whether `used` bytes are nine tenths of `limit` or more.
pub fn nearly_reached(used: ByteCount, limit: ByteCount) -> bool {
    used.get() >= limit.get() / 10 * 9
}

/// What the quota asks for with `used` bytes received against `limit`,
/// when downloads are `paused` for it or not and the nearly-reached warning
/// was `warned` already.
pub fn step(used: ByteCount, limit: QuotaLimit, paused: bool, warned: bool) -> QuotaStep {
    if paused && used < limit.bytes {
        QuotaStep::Resume
    } else if used >= limit.bytes {
        QuotaStep::Pause { first: !paused }
    } else if limit.warn && nearly_reached(used, limit.bytes) && !warned {
        QuotaStep::Warn
    } else {
        QuotaStep::Keep
    }
}

/// The quota's state as the clients show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaNotice {
    pub used: ByteCount,
    pub limit: ByteCount,
    /// Downloads are paused for the quota.
    pub paused: bool,
    /// When they resume, in seconds since 1970.
    pub resumes_at: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(n: u64) -> ByteCount {
        ByteCount::new(n)
    }

    const LIMIT: QuotaLimit = QuotaLimit { bytes: ByteCount::new(1000), window: 60, warn: true };

    #[test]
    fn the_quota_pauses_warns_and_resumes() {
        assert_eq!(step(b(500), LIMIT, false, false), QuotaStep::Keep);
        assert_eq!(step(b(900), LIMIT, false, false), QuotaStep::Warn);
        assert_eq!(step(b(900), LIMIT, false, true), QuotaStep::Keep, "once per window");
        assert_eq!(step(b(900), QuotaLimit { warn: false, ..LIMIT }, false, false), QuotaStep::Keep);
        assert_eq!(step(b(1000), LIMIT, false, false), QuotaStep::Pause { first: true });
        assert_eq!(step(b(1200), LIMIT, true, false), QuotaStep::Pause { first: false });
        assert_eq!(step(b(999), LIMIT, true, false), QuotaStep::Resume);
    }

    #[test]
    fn counts_only_new_bytes_in_the_window() {
        let mut u = Usage::default();
        u.started(DownloadId::new(1), b(500));
        u.progress(DownloadId::new(1), b(700), 10);
        u.progress(DownloadId::new(1), b(1000), 10);
        u.started(DownloadId::new(2), b(0));
        u.progress(DownloadId::new(2), b(50), 11);
        assert_eq!(u.used(11, 60), b(550), "resumed bytes do not count");
        assert_eq!(u.used(70, 60), b(50), "minute 10 left the window");
        assert_eq!(u.used(200, 60), b(0));
    }

    /// The saved count keeps its shape: minutes and byte counts as plain
    /// numbers.
    #[test]
    fn the_saved_count_keeps_its_shape() {
        let old = r#"{"minutes":[[10,200],[11,4000000000]]}"#;
        let mut u: Usage = serde_json::from_str(old).unwrap();
        assert_eq!(serde_json::to_string(&u).unwrap(), old);
        assert_eq!(u.used(11, 60), b(4_000_000_200));
    }

    #[test]
    fn says_when_the_window_frees_up() {
        let mut u = Usage::default();
        u.started(DownloadId::new(1), b(0));
        u.progress(DownloadId::new(1), b(600), 0);
        u.progress(DownloadId::new(1), b(1000), 30);
        // 1000 bytes in an hour with a limit of 800: minute 0 must leave.
        assert_eq!(u.frees_at(40, 60, b(800)), 60);
        assert_eq!(u.frees_at(40, 60, b(2000)), 40, "already under the limit");
    }
}
