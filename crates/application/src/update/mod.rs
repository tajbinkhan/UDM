//! Newer UIDM releases: looking for one, by hand or once a week, and
//! installing it.

mod check_for_update;
mod claim_weekly_check;
mod install_update;

pub use check_for_update::CheckForUpdate;
pub use claim_weekly_check::ClaimWeeklyCheck;
pub use install_update::InstallUpdate;
