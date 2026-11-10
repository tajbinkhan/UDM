use pullbay_domain::ports::ToolFolder;
use pullbay_domain::tools::{self, Platform, Tool, ToolState};

/// What Options shows about yt-dlp and ffmpeg: where each is found, the
/// release Pullbay installed, and whether Pullbay can install it here.
pub struct ListTools<'a, F: ?Sized> {
    folder: &'a F,
    platform: Platform,
}

impl<'a, F: ToolFolder + ?Sized> ListTools<'a, F> {
    pub fn new(folder: &'a F, platform: Platform) -> Self {
        ListTools { folder, platform }
    }

    pub fn run(&self) -> Vec<ToolState> {
        Tool::ALL
            .into_iter()
            .map(|tool| ToolState {
                tool,
                path: self.folder.found(tool).map(|p| p.display().to_string()),
                installed: self.folder.installed(tool).map(|t| t.version),
                installable: tools::source(tool, self.platform).is_some(),
                command: tools::package_command(tool, self.platform.system).to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::ToolShelf;
    use pullbay_domain::tools::{Cpu, InstalledTool, System};

    #[test]
    fn each_tool_says_where_it_is_and_what_installs_it() {
        let shelf = ToolShelf::with(Tool::YtDlp, InstalledTool { version: "2026.10.26".into(), checked: 0 });
        let mac = Platform { system: System::MacOs, cpu: Cpu::Aarch64 };
        let list = ListTools::new(&shelf, mac).run();
        assert_eq!(list[0].tool, Tool::YtDlp);
        assert_eq!(list[0].path.as_deref(), Some("/tools/yt-dlp"));
        assert_eq!(list[0].installed.as_deref(), Some("2026.10.26"));
        assert!(list[0].installable);
        assert_eq!((list[1].path.as_deref(), list[1].installable), (None, false));
        assert_eq!(list[1].command, "brew install ffmpeg");
    }
}
