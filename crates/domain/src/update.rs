//! Newer UDM releases: what a release is, whether one is newer than this
//! copy, when to look again, and which of a release's files Update now
//! installs for the way this copy was installed. Asking GitHub, checking
//! the file against the release's signed checksums and installing it is
//! `udm_infrastructure::update`.

use crate::bytes::ByteCount;
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
    /// The file Update now installs for this copy, when the release has
    /// one: its Setup program on Windows, its disk image on macOS, its
    /// AppImage or `.tar.gz` on Linux.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<SetupAsset>,
    /// This copy came from a `.deb` or `.rpm` package, so the system's
    /// package manager updates it, not UDM.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub package_manager: bool,
}

/// The release file Update now installs (see `Installer`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupAsset {
    pub name: String,
    /// Where GitHub serves it, under this repository's release downloads.
    pub url: String,
    pub size: ByteCount,
    /// Its SHA-256 in hex, when GitHub reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// What kind of file it is. Releases before signed updates offered only
    /// Windows' Setup, so that is what a value without one is.
    #[serde(default)]
    pub installer: Installer,
    /// The release's signed list of checksums, which the file must be on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksums: Option<SignedChecksums>,
}

/// Where a release's `SHA256SUMS` and its minisign signature,
/// `SHA256SUMS.minisig`, are served.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedChecksums {
    pub list: String,
    pub signature: String,
}

/// The name of a release's list of checksums.
pub const CHECKSUMS: &str = "SHA256SUMS";
/// The name of the list's minisign signature.
pub const CHECKSUMS_SIGNATURE: &str = "SHA256SUMS.minisig";

/// The kinds of release file Update now can install.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Installer {
    /// `UDM-Setup-<version>.exe`, run to replace the installed copy.
    #[default]
    WindowsSetup,
    /// `UDM-<version>-macos.dmg`, whose `UDM.app` replaces the running one.
    MacDiskImage,
    /// `UDM-<version>-<arch>.AppImage`, which replaces the running AppImage.
    AppImage,
    /// `udm-<version>-linux-<arch>.tar.gz`, whose `install.sh` installs it
    /// over the copy in `~/.local/lib/udm`.
    LinuxTar,
}

impl Installer {
    /// Whether a release file called `name` is this kind of installer for a
    /// machine whose processor is `arch` (`x86_64`, `aarch64`, as Rust's
    /// `std::env::consts::ARCH` names it). The Windows Setup and the disk
    /// image hold every processor's programs.
    pub fn matches(self, name: &str, arch: &str) -> bool {
        if name.contains(['/', '\\', ':']) {
            return false;
        }
        let lower = name.to_ascii_lowercase();
        let arch = arch.to_ascii_lowercase();
        match self {
            Installer::WindowsSetup => lower.starts_with("udm-setup-") && lower.ends_with(".exe"),
            Installer::MacDiskImage => lower.starts_with("udm-") && lower.ends_with("-macos.dmg"),
            Installer::AppImage => lower.starts_with("udm-") && lower.ends_with(&format!("-{arch}.appimage")),
            Installer::LinuxTar => lower.starts_with("udm-") && lower.ends_with(&format!("-linux-{arch}.tar.gz")),
        }
    }
}

/// How this copy of UDM was installed, which decides what Update now does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Install {
    /// By Windows' Setup.
    WindowsSetup,
    /// As `UDM.app`, from the disk image.
    MacApp,
    /// As an AppImage.
    AppImage,
    /// From the `.tar.gz` by its `install.sh`, into `~/.local/lib/udm`.
    LinuxTar,
    /// From a `.deb` or `.rpm`, which the package manager updates.
    LinuxPackage,
    /// Some other way, such as a copy built from source: the release's page
    /// is shown.
    Other,
}

impl Install {
    /// The release file that updates this copy, if UDM installs one itself.
    pub fn installer(self) -> Option<Installer> {
        match self {
            Install::WindowsSetup => Some(Installer::WindowsSetup),
            Install::MacApp => Some(Installer::MacDiskImage),
            Install::AppImage => Some(Installer::AppImage),
            Install::LinuxTar => Some(Installer::LinuxTar),
            Install::LinuxPackage | Install::Other => None,
        }
    }
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
///
/// The variant names are part of the wire format (the daemon's
/// `error_kind`): renaming one changes what clients read.
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
    fn each_install_takes_its_own_file() {
        let files = [
            "UDM-Setup-0.6.0.exe",
            "Pullbay-Setup-0.6.0.exe",
            "UDM-0.6.0-macos.dmg",
            "UDM-0.6.0-x86_64.AppImage",
            "UDM-0.6.0-aarch64.AppImage",
            "udm-0.6.0-linux-x86_64.tar.gz",
            "udm_0.6.0_amd64.deb",
            "udm-0.6.0-1.x86_64.rpm",
            "UDM-0.6.0-windows-portable.zip",
            "SHA256SUMS",
        ];
        let pick = |install: Install, arch: &str| -> Vec<&str> {
            let Some(kind) = install.installer() else { return Vec::new() };
            files.iter().copied().filter(|f| kind.matches(f, arch)).collect()
        };
        assert_eq!(pick(Install::WindowsSetup, "x86_64"), ["UDM-Setup-0.6.0.exe"]);
        assert_eq!(pick(Install::MacApp, "aarch64"), ["UDM-0.6.0-macos.dmg"]);
        assert_eq!(pick(Install::AppImage, "x86_64"), ["UDM-0.6.0-x86_64.AppImage"]);
        assert_eq!(pick(Install::AppImage, "aarch64"), ["UDM-0.6.0-aarch64.AppImage"]);
        assert_eq!(pick(Install::LinuxTar, "x86_64"), ["udm-0.6.0-linux-x86_64.tar.gz"]);
        assert!(pick(Install::LinuxTar, "aarch64").is_empty(), "no build for that processor");
        assert!(pick(Install::LinuxPackage, "x86_64").is_empty(), "the package manager updates it");
        assert!(pick(Install::Other, "x86_64").is_empty());
        assert!(!Installer::WindowsSetup.matches("../UDM-Setup-1.exe", "x86_64"));
    }

    #[test]
    fn a_setup_without_a_kind_is_windows_setup() {
        let old: SetupAsset = serde_json::from_str(r#"{"name":"UDM-Setup-0.5.0.exe","url":"u","size":3}"#).unwrap();
        assert_eq!(old.installer, Installer::WindowsSetup);
        assert_eq!(old.checksums, None);
        let release: Release = serde_json::from_str(r#"{"version":"0.5.0","url":"u"}"#).unwrap();
        assert!(!release.package_manager);
    }

    #[test]
    fn the_weekly_check_waits_a_week() {
        assert!(check_due(true, 0, CHECK_EVERY_SECS));
        assert!(!check_due(true, 100, CHECK_EVERY_SECS));
        assert!(!check_due(false, 0, CHECK_EVERY_SECS), "the user turned it off");
    }
}
