use std::path::PathBuf;

use pullbay_domain::bytes::ByteCount;
use pullbay_domain::checksum::Algorithm;
use pullbay_domain::ports::{FileHashes, ToolFolder, ToolReleases};
use pullbay_domain::tools::{self, Platform, Tool, ToolError, MAX_TOOL_BYTES};

/// Install button: download `tool`'s official build for this platform from
/// its project's latest GitHub release, check it against the SHA-256 the
/// release's checksum file lists (and GitHub's own, when it lists one), and
/// only then put it in Pullbay's tools folder and let it run. A file that
/// does not match is deleted.
pub struct InstallTool<'a, R: ?Sized, F: ?Sized, H: ?Sized> {
    releases: &'a R,
    folder: &'a F,
    hashes: &'a H,
    platform: Platform,
}

impl<'a, R, F, H> InstallTool<'a, R, F, H>
where
    R: ToolReleases + ?Sized,
    F: ToolFolder + ?Sized,
    H: FileHashes + ?Sized,
{
    pub fn new(releases: &'a R, folder: &'a F, hashes: &'a H, platform: Platform) -> Self {
        InstallTool { releases, folder, hashes, platform }
    }

    /// Install the latest release of `tool` at `now` (seconds since 1970).
    /// With `only_newer`, a release already installed is `UpToDate`.
    /// Returns the release and the program's path.
    pub fn run(&self, tool: Tool, only_newer: bool, now: u64) -> Result<(String, PathBuf), ToolError> {
        let source = tools::source(tool, self.platform).ok_or_else(|| {
            ToolError::NoBuild(tool.name().to_string(), tools::package_command(tool, self.platform.system).to_string())
        })?;
        let release = self.releases.latest(source.repository)?;
        if only_newer && self.folder.installed(tool).is_some_and(|t| t.version == release.version) {
            self.folder.note_checked(tool, now);
            return Err(ToolError::UpToDate(tool.name().to_string()));
        }
        let file = |name: &str| release.files.iter().find(|f| f.name == name);
        let asset = file(source.asset).ok_or_else(|| ToolError::NoAsset(source.asset.to_string()))?;
        let sums = file(source.sums).ok_or_else(|| ToolError::NoChecksum(source.asset.to_string()))?;
        let listed = tools::listed_sha256(&self.releases.text(sums)?, source.asset)
            .ok_or_else(|| ToolError::NoChecksum(source.asset.to_string()))?;
        // GitHub's own digest of the file, when it has one, must agree too.
        if asset.sha256.as_ref().is_some_and(|g| !g.eq_ignore_ascii_case(&listed)) {
            return Err(ToolError::WrongChecksum(source.asset.to_string()));
        }
        let downloaded = self.releases.download(asset, ByteCount::new(MAX_TOOL_BYTES))?;
        let got = match self.hashes.digest(&downloaded, Algorithm::Sha256) {
            Ok(d) => tools::hex(&d),
            Err(e) => {
                self.folder.discard(&downloaded);
                return Err(ToolError::Download(e.0));
            }
        };
        if got != listed {
            self.folder.discard(&downloaded);
            tracing::warn!(tool = tool.name(), file = source.asset, "download does not match its checksum");
            return Err(ToolError::WrongChecksum(source.asset.to_string()));
        }
        let path = self.folder.install(tool, &downloaded, source.packing, &release.version, now)?;
        tracing::info!(tool = tool.name(), version = %release.version, path = %path.display(), "installed");
        Ok((release.version, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Hashes, ToolShelf, ToolSite};
    use pullbay_domain::tools::{Cpu, System};

    const LINUX: Platform = Platform { system: System::Linux, cpu: Cpu::X86_64 };
    const MAC: Platform = Platform { system: System::MacOs, cpu: Cpu::Aarch64 };

    fn digest() -> Vec<u8> {
        vec![0xab; 32]
    }

    fn site(listed: &str) -> ToolSite {
        ToolSite::release(
            "2026.10.26",
            &[("yt-dlp_linux", None), ("SHA2-256SUMS", None)],
            &format!("{listed}  yt-dlp_linux\n"),
        )
    }

    #[test]
    fn a_matching_download_is_installed() {
        let (site, shelf) = (site(&"ab".repeat(32)), ToolShelf::default());
        let hashes = Hashes::with("/dl/yt-dlp_linux", digest());
        let (version, path) = InstallTool::new(&site, &shelf, &hashes, LINUX).run(Tool::YtDlp, false, 50).unwrap();
        assert_eq!(version, "2026.10.26");
        assert_eq!(path, PathBuf::from("/tools/yt-dlp"));
        assert_eq!(site.asked(), ["yt-dlp/yt-dlp"]);
        assert_eq!(shelf.installs(), [(Tool::YtDlp, "2026.10.26".to_string(), 50)]);
    }

    #[test]
    fn a_download_that_does_not_match_is_deleted_not_installed() {
        let (site, shelf) = (site(&"cd".repeat(32)), ToolShelf::default());
        let hashes = Hashes::with("/dl/yt-dlp_linux", digest());
        let err = InstallTool::new(&site, &shelf, &hashes, LINUX).run(Tool::YtDlp, false, 50).unwrap_err();
        assert_eq!(err, ToolError::WrongChecksum("yt-dlp_linux".into()));
        assert!(shelf.installs().is_empty());
        assert_eq!(shelf.discarded(), [PathBuf::from("/dl/yt-dlp_linux")]);
    }

    #[test]
    fn no_published_checksum_means_no_download() {
        let shelf = ToolShelf::default();
        let hashes = Hashes::with("/dl/yt-dlp_linux", digest());
        let unlisted = ToolSite::release("1", &[("yt-dlp_linux", None), ("SHA2-256SUMS", None)], "");
        let err = InstallTool::new(&unlisted, &shelf, &hashes, LINUX).run(Tool::YtDlp, false, 1).unwrap_err();
        assert_eq!(err, ToolError::NoChecksum("yt-dlp_linux".into()));
        let no_file = ToolSite::release("1", &[("yt-dlp_linux", None)], "");
        assert!(matches!(
            InstallTool::new(&no_file, &shelf, &hashes, LINUX).run(Tool::YtDlp, false, 1),
            Err(ToolError::NoChecksum(_))
        ));
        assert!(unlisted.downloaded().is_empty() && no_file.downloaded().is_empty());
    }

    #[test]
    fn githubs_own_digest_must_agree() {
        let shelf = ToolShelf::default();
        let hashes = Hashes::with("/dl/yt-dlp_linux", digest());
        let other = "ef".repeat(32);
        let site = ToolSite::release(
            "1",
            &[("yt-dlp_linux", Some(other.as_str())), ("SHA2-256SUMS", None)],
            &format!("{}  yt-dlp_linux\n", "ab".repeat(32)),
        );
        let err = InstallTool::new(&site, &shelf, &hashes, LINUX).run(Tool::YtDlp, false, 1).unwrap_err();
        assert_eq!(err, ToolError::WrongChecksum("yt-dlp_linux".into()));
        assert!(site.downloaded().is_empty());
    }

    #[test]
    fn ffmpeg_on_macos_points_to_the_package_manager() {
        let (site, shelf, hashes) = (site(""), ToolShelf::default(), Hashes::default());
        let err = InstallTool::new(&site, &shelf, &hashes, MAC).run(Tool::Ffmpeg, false, 1).unwrap_err();
        assert_eq!(err, ToolError::NoBuild("ffmpeg".into(), "brew install ffmpeg".into()));
        assert!(site.asked().is_empty());
    }
}
