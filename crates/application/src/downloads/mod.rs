//! Acting on downloads: remove one, retry one, download one again, give it a
//! new link, change its properties, speed limit or what happens when it
//! finishes, stop everything that runs, pause all and resume all.

mod after_download;
mod pause_all;
mod redownload;
mod refresh_link;
mod remove_download;
mod resume_all;
mod retry_download;
mod set_on_done;
mod set_properties;
mod set_speed_limit;
mod stop_running;

pub use after_download::AfterDownload;
pub use pause_all::PauseAll;
pub use redownload::Redownload;
pub use refresh_link::{RefreshLink, Refreshed};
pub use remove_download::RemoveDownload;
pub use resume_all::{ResumeAll, Resumed};
pub use retry_download::RetryDownload;
pub use set_on_done::SetOnDone;
pub use set_properties::SetProperties;
pub use set_speed_limit::{SetLimitRemember, SetSpeedLimit};
pub use stop_running::{StopRunning, Stopped};
