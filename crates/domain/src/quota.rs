//! The download quota: "no more than N MB every N hours". The daemon counts
//! the bytes every download receives, per minute; these are the counting
//! rules and the decision what the count asks for. Keeping the count across
//! restarts is `uidm_infrastructure::quota`.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

/// Bytes received, by minute since 1970.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    minutes: VecDeque<(u64, u64)>,
    /// Each running download's progress at its last report, so only new
    /// bytes count. Not saved: a restart starts every download afresh.
    #[serde(skip)]
    last: HashMap<u64, u64>,
}

impl Usage {
    /// A download (re)started with `done` bytes already in hand; those do
    /// not count again.
    pub fn started(&mut self, id: u64, done: u64) {
        self.last.insert(id, done);
    }

    /// Download `id` reports `done` bytes at minute `now`: count what is new.
    pub fn progress(&mut self, id: u64, done: u64, now: u64) {
        let before = self.last.insert(id, done).unwrap_or(done);
        let new = done.saturating_sub(before);
        if new == 0 {
            return;
        }
        match self.minutes.back_mut() {
            Some((m, bytes)) if *m == now => *bytes += new,
            _ => self.minutes.push_back((now, new)),
        }
    }

    /// The download stopped; forget its progress.
    pub fn ended(&mut self, id: u64) {
        self.last.remove(&id);
    }

    /// Bytes received in the `window` minutes up to `now`. Older minutes are
    /// dropped.
    pub fn used(&mut self, now: u64, window: u64) -> u64 {
        let start = now.saturating_sub(window.saturating_sub(1));
        while self.minutes.front().is_some_and(|(m, _)| *m < start) {
            self.minutes.pop_front();
        }
        self.minutes.iter().map(|(_, b)| b).sum()
    }

    /// The first minute after `now` at which the window holds less than
    /// `limit` bytes again, if no more arrive.
    pub fn frees_at(&mut self, now: u64, window: u64, limit: u64) -> u64 {
        let mut used = self.used(now, window);
        for (m, b) in &self.minutes {
            if used < limit {
                break;
            }
            used -= b;
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
    pub bytes: u64,
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
pub fn nearly_reached(used: u64, limit: u64) -> bool {
    used >= limit / 10 * 9
}

/// What the quota asks for with `used` bytes received against `limit`,
/// when downloads are `paused` for it or not and the nearly-reached warning
/// was `warned` already.
pub fn step(used: u64, limit: QuotaLimit, paused: bool, warned: bool) -> QuotaStep {
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
    pub used: u64,
    pub limit: u64,
    /// Downloads are paused for the quota.
    pub paused: bool,
    /// When they resume, in seconds since 1970.
    pub resumes_at: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: QuotaLimit = QuotaLimit { bytes: 1000, window: 60, warn: true };

    #[test]
    fn the_quota_pauses_warns_and_resumes() {
        assert_eq!(step(500, LIMIT, false, false), QuotaStep::Keep);
        assert_eq!(step(900, LIMIT, false, false), QuotaStep::Warn);
        assert_eq!(step(900, LIMIT, false, true), QuotaStep::Keep, "once per window");
        assert_eq!(step(900, QuotaLimit { warn: false, ..LIMIT }, false, false), QuotaStep::Keep);
        assert_eq!(step(1000, LIMIT, false, false), QuotaStep::Pause { first: true });
        assert_eq!(step(1200, LIMIT, true, false), QuotaStep::Pause { first: false });
        assert_eq!(step(999, LIMIT, true, false), QuotaStep::Resume);
    }

    #[test]
    fn counts_only_new_bytes_in_the_window() {
        let mut u = Usage::default();
        u.started(1, 500);
        u.progress(1, 700, 10);
        u.progress(1, 1000, 10);
        u.started(2, 0);
        u.progress(2, 50, 11);
        assert_eq!(u.used(11, 60), 550, "resumed bytes do not count");
        assert_eq!(u.used(70, 60), 50, "minute 10 left the window");
        assert_eq!(u.used(200, 60), 0);
    }

    #[test]
    fn says_when_the_window_frees_up() {
        let mut u = Usage::default();
        u.started(1, 0);
        u.progress(1, 600, 0);
        u.progress(1, 1000, 30);
        // 1000 bytes in an hour with a limit of 800: minute 0 must leave.
        assert_eq!(u.frees_at(40, 60, 800), 60);
        assert_eq!(u.frees_at(40, 60, 2000), 40, "already under the limit");
    }
}
