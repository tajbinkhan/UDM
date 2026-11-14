use udm_domain::ports::{ReleaseSource, UpdateChecks};
use udm_domain::update::{self, Release, UpdateError};

/// Ask for the latest release and note that UDM looked.
pub struct CheckForUpdate<'a, S, N> {
    source: &'a S,
    checks: &'a N,
    current: &'a str,
}

impl<'a, S: ReleaseSource, N: UpdateChecks> CheckForUpdate<'a, S, N> {
    /// `current` is this copy's version.
    pub fn new(source: &'a S, checks: &'a N, current: &'a str) -> Self {
        CheckForUpdate { source, checks, current }
    }

    /// Look at `now` (seconds since 1970). Returns the latest release when it
    /// is newer than this copy.
    pub fn run(&self, now: u64) -> Result<Option<Release>, UpdateError> {
        let found = self.source.latest();
        self.checks.note_checked(now, true);
        Ok(found?).map(|r| update::is_newer(&r.version, self.current).then_some(r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Checked, Releases};

    pub fn release(version: &str) -> Release {
        Release { version: version.into(), url: "https://github.com/x/y".into(), setup: None, package_manager: false }
    }

    #[test]
    fn only_a_newer_release_is_offered() {
        let checks = Checked::default();
        let newer = Releases::offering(Ok(release("1.0.0")));
        assert_eq!(CheckForUpdate::new(&newer, &checks, "0.3.0").run(7), Ok(Some(release("1.0.0"))));
        let same = Releases::offering(Ok(release("0.3.0")));
        assert_eq!(CheckForUpdate::new(&same, &checks, "0.3.0").run(8), Ok(None));
        assert_eq!(checks.noted(), [(7, true), (8, true)]);
    }

    #[test]
    fn a_failed_check_still_counts_as_a_check() {
        let checks = Checked::default();
        let down = Releases::offering(Err(UpdateError::Unreachable("timed out".into())));
        let err = CheckForUpdate::new(&down, &checks, "0.3.0").run(9).unwrap_err();
        assert_eq!(err.to_string(), "Could not reach GitHub: timed out");
        assert_eq!(checks.noted(), [(9, true)]);
    }
}
