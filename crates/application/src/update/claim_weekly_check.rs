use pullbay_domain::ports::UpdateChecks;
use pullbay_domain::update;

/// Decide whether the weekly look for a newer release is due, and if so
/// claim it at once, so two windows connecting together do not both look.
pub struct ClaimWeeklyCheck<'a, N> {
    checks: &'a N,
}

impl<'a, N: UpdateChecks> ClaimWeeklyCheck<'a, N> {
    pub fn new(checks: &'a N) -> Self {
        ClaimWeeklyCheck { checks }
    }

    /// Returns whether the caller should look now (`now` in seconds since
    /// 1970).
    pub fn run(&self, now: u64) -> bool {
        let (on, last) = self.checks.weekly();
        if !update::check_due(on, last, now) {
            return false;
        }
        self.checks.note_checked(now, false);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Checked;
    use pullbay_domain::update::CHECK_EVERY_SECS;

    #[test]
    fn the_first_window_after_a_week_claims_the_check() {
        let checks = Checked::weekly(true, 0);
        let claim = ClaimWeeklyCheck::new(&checks);
        assert!(claim.run(CHECK_EVERY_SECS));
        assert!(!claim.run(CHECK_EVERY_SECS + 5), "the next window finds it claimed");
        assert_eq!(checks.noted(), [(CHECK_EVERY_SECS, false)]);
        assert!(!ClaimWeeklyCheck::new(&Checked::weekly(false, 0)).run(CHECK_EVERY_SECS));
    }
}
