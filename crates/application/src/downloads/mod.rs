//! Acting on downloads: remove one, retry one, download one again, set its
//! speed limit, stop everything that runs, pause all and resume all.

mod pause_all;
mod redownload;
mod remove_download;
mod resume_all;
mod retry_download;
mod set_speed_limit;
mod stop_running;

pub use pause_all::PauseAll;
pub use redownload::Redownload;
pub use remove_download::RemoveDownload;
pub use resume_all::{ResumeAll, Resumed};
pub use retry_download::RetryDownload;
pub use set_speed_limit::{SetLimitRemember, SetSpeedLimit};
pub use stop_running::{StopRunning, Stopped};
