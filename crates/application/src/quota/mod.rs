//! The download quota: pausing everything when it is reached, warning
//! shortly before, and resuming once the window allows.

mod check_quota;
mod quota_status;

pub use check_quota::{CheckQuota, QuotaEvent};
pub use quota_status::QuotaStatus;
