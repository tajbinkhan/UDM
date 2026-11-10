//! yt-dlp and ffmpeg: installing their official builds, checked against
//! the checksums their projects publish, keeping yt-dlp up to date when
//! the user asked for it, and saying what is installed where.

mod install_tool;
mod list_tools;
mod update_yt_dlp;

pub use install_tool::InstallTool;
pub use list_tools::ListTools;
pub use update_yt_dlp::UpdateYtDlp;
