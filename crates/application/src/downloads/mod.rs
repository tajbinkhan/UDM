//! Acting on downloads: remove one, retry one, stop everything that runs,
//! pause all and resume all.

mod pause_all;
mod remove_download;
mod resume_all;
mod retry_download;
mod stop_running;

pub use pause_all::PauseAll;
pub use remove_download::RemoveDownload;
pub use resume_all::{ResumeAll, Resumed};
pub use retry_download::RetryDownload;
pub use stop_running::{StopRunning, Stopped};
