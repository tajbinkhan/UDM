use pullbay_domain::ports::ProblemReports;
use pullbay_domain::report::SavedReport;

/// The reports written since the window last looked, to offer once each:
/// they are marked offered as they are found.
pub struct FindNewReports<'a, P: ?Sized> {
    reports: &'a P,
}

impl<'a, P: ProblemReports + ?Sized> FindNewReports<'a, P> {
    pub fn new(reports: &'a P) -> Self {
        FindNewReports { reports }
    }

    /// The new reports, the newest first.
    pub fn run(&self) -> Vec<SavedReport> {
        let mut found = self.reports.unoffered();
        if found.is_empty() {
            return found;
        }
        self.reports.mark_offered(&found);
        found.sort_by_key(|r| std::cmp::Reverse(r.summary.at_ms));
        tracing::info!(reports = found.len(), "found problem reports to offer");
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Reports;
    use pullbay_domain::report::{Kind, Program};

    #[test]
    fn each_report_is_offered_once_newest_first() {
        let reports = Reports::default();
        reports.add(Program::Daemon, Kind::Crash, 10);
        reports.add(Program::Window, Kind::Freeze, 30);
        reports.add(Program::Host, Kind::Crash, 20);
        let found = FindNewReports::new(&reports).run();
        let at: Vec<u64> = found.iter().map(|r| r.summary.at_ms).collect();
        assert_eq!(at, [30, 20, 10]);
        assert!(FindNewReports::new(&reports).run().is_empty(), "offered already");
        reports.add(Program::Daemon, Kind::Crash, 40);
        let again = FindNewReports::new(&reports).run();
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].summary.program, Program::Daemon);
    }
}
