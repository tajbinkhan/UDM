use std::path::PathBuf;

use udm_domain::ports::{FileHashes, ToolFolder, ToolReleases};
use udm_domain::tools::{self, Platform, Tool, ToolError};

use super::InstallTool;

/// "Keep yt-dlp up to date": once a day, when the user turned it on and
/// UDM installed yt-dlp, install a newer release the same checked way
/// the Install button does. A yt-dlp UDM did not install is left to
/// whatever installed it.
pub struct UpdateYtDlp<'a, R: ?Sized, F: ?Sized, H: ?Sized> {
    install: InstallTool<'a, R, F, H>,
    folder: &'a F,
}

impl<'a, R, F, H> UpdateYtDlp<'a, R, F, H>
where
    R: ToolReleases + ?Sized,
    F: ToolFolder + ?Sized,
    H: FileHashes + ?Sized,
{
    pub fn new(releases: &'a R, folder: &'a F, hashes: &'a H, platform: Platform) -> Self {
        UpdateYtDlp { install: InstallTool::new(releases, folder, hashes, platform), folder }
    }

    /// Returns the release installed, or `None` when nothing was due or
    /// the installed one is the newest.
    pub fn run(&self, auto: bool, now: u64) -> Result<Option<(String, PathBuf)>, ToolError> {
        if !tools::update_due(auto, self.folder.installed(Tool::YtDlp).as_ref(), now) {
            return Ok(None);
        }
        // A failed look waits a day too, rather than asking GitHub each pass.
        self.folder.note_checked(Tool::YtDlp, now);
        match self.install.run(Tool::YtDlp, true, now) {
            Ok(done) => Ok(Some(done)),
            Err(ToolError::UpToDate(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Hashes, ToolShelf, ToolSite};
    use udm_domain::tools::{Cpu, InstalledTool, System, UPDATE_EVERY_SECS};

    const LINUX: Platform = Platform { system: System::Linux, cpu: Cpu::X86_64 };

    fn site(version: &str) -> ToolSite {
        ToolSite::release(
            version,
            &[("yt-dlp_linux", None), ("SHA2-256SUMS", None)],
            &format!("{}  yt-dlp_linux\n", "ab".repeat(32)),
        )
    }

    #[test]
    fn a_newer_release_replaces_the_installed_one_once_a_day() {
        let shelf = ToolShelf::with(Tool::YtDlp, InstalledTool { version: "1".into(), checked: 0 });
        let hashes = Hashes::with("/dl/yt-dlp_linux", vec![0xab; 32]);
        let site = site("2");
        let update = UpdateYtDlp::new(&site, &shelf, &hashes, LINUX);
        assert_eq!(update.run(true, 10).unwrap(), None, "not due yet");
        assert!(site.asked().is_empty());
        let done = update.run(true, UPDATE_EVERY_SECS).unwrap().unwrap();
        assert_eq!(done.0, "2");
        assert_eq!(update.run(true, UPDATE_EVERY_SECS + 5).unwrap(), None, "checked just now");
    }

    #[test]
    fn nothing_happens_when_off_newest_or_not_udms() {
        let hashes = Hashes::with("/dl/yt-dlp_linux", vec![0xab; 32]);
        let site = site("1");
        let shelf = ToolShelf::with(Tool::YtDlp, InstalledTool { version: "1".into(), checked: 0 });
        let update = UpdateYtDlp::new(&site, &shelf, &hashes, LINUX);
        assert_eq!(update.run(false, UPDATE_EVERY_SECS).unwrap(), None);
        assert!(site.asked().is_empty());
        assert_eq!(update.run(true, UPDATE_EVERY_SECS).unwrap(), None);
        assert!(site.downloaded().is_empty(), "the newest is installed");
        assert_eq!(shelf.installed(Tool::YtDlp).unwrap().checked, UPDATE_EVERY_SECS);
        let none = ToolShelf::default();
        assert_eq!(UpdateYtDlp::new(&site, &none, &hashes, LINUX).run(true, UPDATE_EVERY_SECS).unwrap(), None);
    }
}
