//! Problem reports in the window: offering the ones written since the last
//! look, writing one when the window freezes or the daemon stops
//! answering, and deleting one the user does not want. The panic hook that
//! writes crash reports is `pullbay_infrastructure::crash`; the rules are
//! `pullbay_domain::report`.

mod discard_report;
mod find_new_reports;
mod save_report;

pub use discard_report::DiscardReport;
pub use find_new_reports::FindNewReports;
pub use save_report::SaveReport;
