//! Acting on downloads: add them to wait in a queue, hold one for the
//! save-as dialog and take its answer, start or cancel one, point at an
//! older one instead of adding it again, remove one, retry one, download
//! one again, give it a new link, change its properties, speed limit or
//! what happens when it finishes, convert its file, compare its file with a
//! checksum, stop everything that runs, pause all and resume all, and keep
//! downloads together as packages and unpack their archives.

mod add_later;
mod add_many;
mod after_download;
mod cancel_download;
mod check_download;
mod confirm_answer;
mod convert_download;
mod hold_for_answer;
mod keep_packages;
mod lift_limit_while_away;
mod pause_all;
mod redownload;
mod refresh_link;
mod remove_download;
mod resume_all;
mod retry_download;
mod set_on_done;
mod set_properties;
mod set_speed_limit;
mod start_or_wait;
mod stop_running;
mod ungroup_package;
mod unpack_package;
mod use_existing;
mod verify_checksum;

pub use add_later::AddLater;
pub use add_many::{AddMany, AddedMany};
pub use after_download::AfterDownload;
pub use cancel_download::{CancelDownload, Canceled};
pub use check_download::{CheckDownload, CheckOutcome};
pub use confirm_answer::{ConfirmAnswer, ConfirmError, ConfirmWarning, Confirmed};
pub use convert_download::ConvertDownload;
pub use hold_for_answer::{Held, HoldForAnswer, Probed};
pub use keep_packages::KeepPackages;
pub use lift_limit_while_away::LiftLimitWhileAway;
pub use pause_all::PauseAll;
pub use redownload::Redownload;
pub use refresh_link::{RefreshLink, Refreshed};
pub use remove_download::RemoveDownload;
pub use resume_all::{ResumeAll, Resumed};
pub use retry_download::RetryDownload;
pub use set_on_done::SetOnDone;
pub use set_properties::SetProperties;
pub use set_speed_limit::{SetLimitRemember, SetSpeedLimit};
pub use start_or_wait::{StartOrWait, StartOrWaitError};
pub use stop_running::{StopRunning, Stopped};
pub use ungroup_package::UngroupPackage;
pub use unpack_package::{UnpackOutcome, UnpackPackage, UnpackPackageError};
pub use use_existing::{Existing, UseExisting};
pub use verify_checksum::{Verified, VerifyChecksum};
