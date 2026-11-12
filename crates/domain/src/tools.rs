//! The programs UDM can install: yt-dlp, which reads video pages,
//! ffmpeg, which joins and converts what it fetches, and gallery-dl, which
//! lists the pictures of image galleries. Where each official build comes
//! from, how its published checksum is read, and when yt-dlp is due for an
//! update. Downloading, checking and unpacking them are
//! `udm_infrastructure::video_tools`.

use serde::{Deserialize, Serialize};

/// A program UDM can install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    YtDlp,
    Ffmpeg,
    GalleryDl,
}

impl Tool {
    pub const ALL: [Tool; 3] = [Tool::YtDlp, Tool::Ffmpeg, Tool::GalleryDl];

    /// Its program's name, without `.exe`.
    pub fn name(self) -> &'static str {
        match self {
            Tool::YtDlp => "yt-dlp",
            Tool::Ffmpeg => "ffmpeg",
            Tool::GalleryDl => "gallery-dl",
        }
    }
}

/// The site a project publishes its releases on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forge {
    /// github.com, read through its API at api.github.com.
    GitHub,
    /// codeberg.org (Forgejo), read through its API at codeberg.org/api/v1.
    Codeberg,
}

/// The operating system a build is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Windows,
    MacOs,
    Linux,
    Other,
}

/// The processor a build is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cpu {
    X86_64,
    Aarch64,
    X86,
    Other,
}

/// The system UDM runs on, which picks the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub system: System,
    pub cpu: Cpu,
}

/// How a release file holds the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Packing {
    /// The file is the program.
    Program,
    /// A zip or tar.xz archive with the program (and ffprobe) in `bin/`.
    Archive,
}

/// Where a tool's official build for a platform comes from: a repository's
/// latest release on GitHub or Codeberg, the file in it, and the file of
/// SHA-256 checksums the project publishes beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSource {
    pub forge: Forge,
    /// `owner/name` on the forge.
    pub repository: &'static str,
    pub asset: &'static str,
    /// The checksum file in the same release, in `sha256sum`'s format.
    pub sums: &'static str,
    pub packing: Packing,
}

/// yt-dlp's own releases, as its README lists them.
pub const YT_DLP_REPOSITORY: &str = "yt-dlp/yt-dlp";
/// The ffmpeg builds the yt-dlp project makes and its README points to
/// for Windows and Linux. ffmpeg itself publishes source only.
pub const FFMPEG_REPOSITORY: &str = "yt-dlp/FFmpeg-Builds";
/// gallery-dl's releases. Its development moved from GitHub to Codeberg in
/// 2026; the GitHub releases since then carry no files and no checksums.
pub const GALLERY_DL_REPOSITORY: &str = "mikf/gallery-dl";

/// The official build of `tool` for `platform`, or `None` where UDM
/// installs none: ffmpeg on macOS (no build there comes with a published
/// checksum), gallery-dl on macOS and on ARM processors (its project builds
/// programs for x86 Windows and x86-64 Linux only), and systems or
/// processors no build is made for.
pub fn source(tool: Tool, platform: Platform) -> Option<ToolSource> {
    use Cpu::*;
    use System::*;
    let (forge, repository, sums, packing) = match tool {
        Tool::YtDlp => (Forge::GitHub, YT_DLP_REPOSITORY, "SHA2-256SUMS", Packing::Program),
        Tool::Ffmpeg => (Forge::GitHub, FFMPEG_REPOSITORY, "checksums.sha256", Packing::Archive),
        Tool::GalleryDl => (Forge::Codeberg, GALLERY_DL_REPOSITORY, "SHA256SUMS", Packing::Program),
    };
    let asset = match (tool, platform.system, platform.cpu) {
        (Tool::YtDlp, Windows, X86_64) => "yt-dlp.exe",
        (Tool::YtDlp, Windows, Aarch64) => "yt-dlp_arm64.exe",
        (Tool::YtDlp, Windows, X86) => "yt-dlp_x86.exe",
        // One universal program for Intel and Apple processors.
        (Tool::YtDlp, MacOs, X86_64 | Aarch64) => "yt-dlp_macos",
        (Tool::YtDlp, Linux, X86_64) => "yt-dlp_linux",
        (Tool::YtDlp, Linux, Aarch64) => "yt-dlp_linux_aarch64",
        (Tool::Ffmpeg, Windows, X86_64) => "ffmpeg-master-latest-win64-gpl.zip",
        (Tool::Ffmpeg, Windows, Aarch64) => "ffmpeg-master-latest-winarm64-gpl.zip",
        (Tool::Ffmpeg, Windows, X86) => "ffmpeg-master-latest-win32-gpl.zip",
        (Tool::Ffmpeg, Linux, X86_64) => "ffmpeg-master-latest-linux64-gpl.tar.xz",
        (Tool::Ffmpeg, Linux, Aarch64) => "ffmpeg-master-latest-linuxarm64-gpl.tar.xz",
        (Tool::GalleryDl, Windows, X86_64) => "gallery-dl.exe",
        (Tool::GalleryDl, Windows, X86) => "gallery-dl_x86.exe",
        // An x86-64 program; nothing for ARM.
        (Tool::GalleryDl, Linux, X86_64) => "gallery-dl.bin",
        _ => return None,
    };
    Some(ToolSource { forge, repository, asset, sums, packing })
}

/// What the user can run instead where UDM installs nothing, or would
/// rather use the system's packages: a package manager's command.
pub fn package_command(tool: Tool, system: System) -> &'static str {
    match (tool, system) {
        (Tool::YtDlp, System::Windows) => "winget install yt-dlp.yt-dlp",
        (Tool::Ffmpeg, System::Windows) => "winget install Gyan.FFmpeg",
        (Tool::YtDlp, System::MacOs) => "brew install yt-dlp",
        (Tool::Ffmpeg, System::MacOs) => "brew install ffmpeg",
        (Tool::YtDlp, _) => "sudo apt install yt-dlp",
        (Tool::Ffmpeg, _) => "sudo apt install ffmpeg",
        // gallery-dl's README names pip first; it works on every system.
        (Tool::GalleryDl, System::Windows) => "python -m pip install -U gallery-dl",
        (Tool::GalleryDl, System::MacOs) => "brew install gallery-dl",
        (Tool::GalleryDl, _) => "python3 -m pip install -U gallery-dl",
    }
}

/// The SHA-256 (lowercase hex) that a checksum file in `sha256sum`'s
/// format (`<hex>  <name>`, or `<hex> *<name>` for binary mode) lists for
/// `name`. A name listed twice with different sums lists none.
pub fn listed_sha256(sums: &str, name: &str) -> Option<String> {
    let mut found: Option<String> = None;
    for line in sums.lines() {
        let line = line.trim_end_matches('\r');
        let Some((hash, rest)) = line.split_once(char::is_whitespace) else { continue };
        let listed = rest.trim_start().trim_start_matches('*');
        if listed != name || !is_sha256(hash) {
            continue;
        }
        let hash = hash.to_ascii_lowercase();
        match &found {
            Some(other) if *other != hash => return None,
            _ => found = Some(hash),
        }
    }
    found
}

/// Whether `s` is 64 hex digits.
pub fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `bytes` as lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A tool UDM installed: the release it came from, and when UDM
/// last looked for a newer one (seconds since 1970).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledTool {
    pub version: String,
    #[serde(default)]
    pub checked: u64,
}

/// What Options shows about a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolState {
    pub tool: Tool,
    /// Where the program UDM uses is, if it finds one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The release UDM installed, when it installed this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed: Option<String>,
    /// UDM can install it on this system.
    #[serde(default)]
    pub installable: bool,
    /// The package manager's command for it on this system.
    #[serde(default)]
    pub command: String,
}

/// How often "Keep yt-dlp up to date" looks for a newer release. Sites
/// change their pages often, and yt-dlp releases follow.
pub const UPDATE_EVERY_SECS: u64 = 24 * 60 * 60;

/// Whether yt-dlp should be looked at for an update at `now`: the user
/// turned it on, UDM installed it, and a day passed since the last look.
pub fn update_due(auto: bool, installed: Option<&InstalledTool>, now: u64) -> bool {
    auto && installed.is_some_and(|t| now.saturating_sub(t.checked) >= UPDATE_EVERY_SECS)
}

/// The largest release file UDM downloads for a tool.
pub const MAX_TOOL_BYTES: u64 = 512 * 1024 * 1024;

/// Why a tool could not be installed or updated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum ToolError {
    /// UDM installs no build of it here; the package manager's
    /// command instead.
    #[error("UDM cannot install {0} on this system. Install it with: {1}")]
    NoBuild(String, String),
    /// The release site (GitHub or Codeberg) could not be reached; why.
    #[error("Could not reach the release site: {0}")]
    Unreachable(String),
    /// The release site's answer could not be read; why.
    #[error("The release site's answer could not be read: {0}")]
    Answer(String),
    /// The latest release has no file of this name.
    #[error("The latest release has no file {0}.")]
    NoAsset(String),
    /// The release's checksum file lists no checksum for this file.
    #[error("The release publishes no checksum for {0}, so UDM will not install it.")]
    NoChecksum(String),
    /// The downloaded file is not the one the checksums list.
    #[error("{0} does not match its published checksum. It was deleted and nothing was installed.")]
    WrongChecksum(String),
    /// The download failed; why.
    #[error("The download failed: {0}")]
    Download(String),
    /// The archive could not be unpacked; why.
    #[error("Could not unpack {0}: {1}")]
    Unpack(String, String),
    /// A file could not be written.
    #[error("Could not save {path}: {reason}")]
    Save { path: String, reason: String },
    /// The latest release is the one installed.
    #[error("{0} is up to date.")]
    UpToDate(String),
    /// Another install of the same tool is running.
    #[error("{0} is being installed already.")]
    Busy(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: Platform = Platform { system: System::Windows, cpu: Cpu::X86_64 };
    const MAC: Platform = Platform { system: System::MacOs, cpu: Cpu::Aarch64 };
    const LINUX_ARM: Platform = Platform { system: System::Linux, cpu: Cpu::Aarch64 };

    #[test]
    fn each_system_gets_its_official_build() {
        let yt = source(Tool::YtDlp, WIN).unwrap();
        assert_eq!(
            (yt.repository, yt.asset, yt.sums, yt.packing),
            ("yt-dlp/yt-dlp", "yt-dlp.exe", "SHA2-256SUMS", Packing::Program)
        );
        assert_eq!(source(Tool::YtDlp, MAC).unwrap().asset, "yt-dlp_macos");
        assert_eq!(source(Tool::YtDlp, LINUX_ARM).unwrap().asset, "yt-dlp_linux_aarch64");
        let ff = source(Tool::Ffmpeg, WIN).unwrap();
        assert_eq!(
            (ff.repository, ff.asset, ff.packing),
            ("yt-dlp/FFmpeg-Builds", "ffmpeg-master-latest-win64-gpl.zip", Packing::Archive)
        );
        assert_eq!(source(Tool::Ffmpeg, LINUX_ARM).unwrap().asset, "ffmpeg-master-latest-linuxarm64-gpl.tar.xz");
        assert_eq!(source(Tool::Ffmpeg, MAC), None, "no checksummed build for macOS");
        assert_eq!(source(Tool::YtDlp, Platform { system: System::Other, cpu: Cpu::X86_64 }), None);
        assert_eq!(package_command(Tool::Ffmpeg, System::MacOs), "brew install ffmpeg");
        assert_eq!(yt.forge, Forge::GitHub);
    }

    #[test]
    fn gallery_dl_comes_from_codeberg_where_a_build_is_made() {
        let win = source(Tool::GalleryDl, WIN).unwrap();
        assert_eq!(
            (win.forge, win.repository, win.asset, win.sums, win.packing),
            (Forge::Codeberg, "mikf/gallery-dl", "gallery-dl.exe", "SHA256SUMS", Packing::Program)
        );
        let x86 = Platform { system: System::Windows, cpu: Cpu::X86 };
        assert_eq!(source(Tool::GalleryDl, x86).unwrap().asset, "gallery-dl_x86.exe");
        let linux = Platform { system: System::Linux, cpu: Cpu::X86_64 };
        assert_eq!(source(Tool::GalleryDl, linux).unwrap().asset, "gallery-dl.bin");
        assert_eq!(source(Tool::GalleryDl, MAC), None);
        assert_eq!(source(Tool::GalleryDl, LINUX_ARM), None);
        assert_eq!(package_command(Tool::GalleryDl, System::MacOs), "brew install gallery-dl");
        assert_eq!(package_command(Tool::GalleryDl, System::Linux), "python3 -m pip install -U gallery-dl");
        assert_eq!(serde_json::to_string(&Tool::GalleryDl).unwrap(), r#""gallery_dl""#);
        assert_eq!(Tool::ALL.len(), 3);
    }

    #[test]
    fn checksum_files_are_read() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!("{a}  yt-dlp\n{b} *yt-dlp.exe\r\nnot a line\n{}  short\n", "c".repeat(10));
        assert_eq!(listed_sha256(&sums, "yt-dlp"), Some(a.clone()));
        assert_eq!(listed_sha256(&sums, "yt-dlp.exe"), Some("b".repeat(64)));
        assert_eq!(listed_sha256(&sums, "yt-dlp_macos"), None);
        assert_eq!(listed_sha256(&sums, "short"), None);
        let twice = format!("{a}  x\n{}  x\n", "d".repeat(64));
        assert_eq!(listed_sha256(&twice, "x"), None);
        assert_eq!(hex(&[0, 0xab, 0x10]), "00ab10");
    }

    #[test]
    fn yt_dlp_updates_once_a_day_when_asked() {
        let t = InstalledTool { version: "2026.10.26".into(), checked: 100 };
        assert!(!update_due(false, Some(&t), 100 + UPDATE_EVERY_SECS));
        assert!(!update_due(true, None, 100 + UPDATE_EVERY_SECS));
        assert!(!update_due(true, Some(&t), 99 + UPDATE_EVERY_SECS));
        assert!(update_due(true, Some(&t), 100 + UPDATE_EVERY_SECS));
    }

    #[test]
    fn errors_keep_their_wire_shape() {
        let e = ToolError::WrongChecksum("yt-dlp.exe".into());
        assert_eq!(serde_json::to_value(&e).unwrap(), serde_json::json!({"WrongChecksum": "yt-dlp.exe"}));
        assert_eq!(serde_json::to_string(&Tool::YtDlp).unwrap(), r#""yt_dlp""#);
    }
}
