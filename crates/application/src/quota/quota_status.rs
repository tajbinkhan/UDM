use uidm_domain::ports::QuotaState;
use uidm_domain::quota::{self, QuotaLimit, QuotaNotice};

/// The quota's state for a window that just connected: shown when
/// downloads are paused for it, or when it is nearly reached and the user
/// asked to be warned.
pub struct QuotaStatus<'a, U> {
    quota: &'a U,
}

impl<'a, U: QuotaState> QuotaStatus<'a, U> {
    pub fn new(quota: &'a U) -> Self {
        QuotaStatus { quota }
    }

    pub fn run(&self, limit: Option<QuotaLimit>, now: u64) -> Option<QuotaNotice> {
        let limit = limit?;
        let used = self.quota.used(now, limit.window);
        let paused = self.quota.paused();
        (paused || (limit.warn && quota::nearly_reached(used, limit.bytes))).then(|| QuotaNotice {
            used,
            limit: limit.bytes,
            paused,
            resumes_at: paused.then(|| self.quota.frees_at(now, limit.window, limit.bytes) * 60),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Quota;

    const LIMIT: QuotaLimit = QuotaLimit { bytes: 1000, window: 60, warn: true };

    #[test]
    fn a_new_window_hears_of_a_paused_or_nearly_reached_quota() {
        let quota = Quota::at(100, 0);
        assert_eq!(QuotaStatus::new(&quota).run(Some(LIMIT), 5), None);
        assert_eq!(QuotaStatus::new(&quota).run(None, 5), None, "the quota is off");
        let quota = Quota::at(950, 0);
        let notice = QuotaNotice { used: 950, limit: 1000, paused: false, resumes_at: None };
        assert_eq!(QuotaStatus::new(&quota).run(Some(LIMIT), 5), Some(notice));
        assert_eq!(QuotaStatus::new(&quota).run(Some(QuotaLimit { warn: false, ..LIMIT }), 5), None);
        let quota = Quota::at(1000, 70);
        quota.hold(&[], &[]);
        let paused = QuotaStatus::new(&quota).run(Some(LIMIT), 5).unwrap();
        assert_eq!((paused.paused, paused.resumes_at), (true, Some(70 * 60)));
    }
}
