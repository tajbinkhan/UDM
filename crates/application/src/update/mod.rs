//! Newer UDM releases: looking for one, by hand or once a week, and
//! downloading it ahead, and installing it.

mod check_for_update;
mod claim_weekly_check;
mod download_update;
mod install_update;

pub use check_for_update::CheckForUpdate;
pub use claim_weekly_check::ClaimWeeklyCheck;
pub use download_update::{DownloadUpdate, ReadyUpdate};
pub use install_update::InstallUpdate;
