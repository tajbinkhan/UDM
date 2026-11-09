//! Newer UIDM releases: what a release is, whether one is newer than this
//! copy, and when to look again. Asking GitHub and checking the Setup
//! program it offers is `uidm_infrastructure::update`.

use serde::{Deserialize, Serialize};

/// How often the window's check runs on its own: once a week.
pub const CHECK_EVERY_SECS: u64 = 7 * 24 * 60 * 60;

/// A release on GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    /// The version, without a leading `v`.
    pub version: String,
    /// Its page on GitHub.
    pub url: String,
    /// Its Setup program, when one is attached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<SetupAsset>,
}

/// A release's `UIDM-Setup-<version>.exe`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupAsset {
    pub name: String,
    /// Where GitHub serves it, under this repository's release downloads.
    pub url: String,
    pub size: u64,
    /// Its SHA-256 in hex, when GitHub reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// `1.2.3` (or `v1.2.3`, `1.2.3-beta`) as numbers to compare; missing parts
/// count as 0.
fn numbers(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['.', '-', '+'])
        .take(3)
        .map(|p| p.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0))
        .collect()
}

/// Whether `candidate` is a later version than `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (mut a, mut b) = (numbers(candidate), numbers(current));
    a.resize(3, 0);
    b.resize(3, 0);
    a > b
}

/// Whether the weekly check is due: it is `on`, and a week has passed
/// since the `last` one, at `now` (seconds since 1970).
pub fn check_due(on: bool, last: u64, now: u64) -> bool {
    on && now.saturating_sub(last) >= CHECK_EVERY_SECS
}

/// Why looking for or installing an update failed. The messages are shown
/// to the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum UpdateError {
    #[error("Could not reach GitHub: {0}")]
    Unreachable(String),
    /// GitHub's answer could not be read; the message says how.
    #[error("{0}")]
    Answer(String),
    #[error("This is the newest version.")]
    NewestAlready,
    #[error("This release has no Setup program; download it from its page.")]
    NoSetup,
    #[error("Could not download the update: {0}")]
    Download(String),
    /// The Setup program is not what GitHub said it is.
    #[error("{0}")]
    BadSetup(String),
    #[error("Could not save the update to {path}: {reason}")]
    Save { path: String, reason: String },
    #[error("Could not start {path}: {reason}")]
    Start { path: String, reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.5"));
        assert!(is_newer("1.0", "0.99.99"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0-beta", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn the_weekly_check_waits_a_week() {
        assert!(check_due(true, 0, CHECK_EVERY_SECS));
        assert!(!check_due(true, 100, CHECK_EVERY_SECS));
        assert!(!check_due(false, 0, CHECK_EVERY_SECS), "the user turned it off");
    }
}
