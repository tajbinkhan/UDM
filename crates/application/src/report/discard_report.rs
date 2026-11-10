use pullbay_domain::ports::{FilesError, ProblemReports};
use pullbay_domain::report::SavedReport;

/// Delete a report the user chose not to keep, such as the one written
/// while the window was frozen once it answers again.
pub struct DiscardReport<'a, P: ?Sized> {
    reports: &'a P,
}

impl<'a, P: ProblemReports + ?Sized> DiscardReport<'a, P> {
    pub fn new(reports: &'a P) -> Self {
        DiscardReport { reports }
    }

    pub fn run(&self, report: &SavedReport) -> Result<(), FilesError> {
        self.reports.delete(report)?;
        // Offered now, so it is not offered again should it come back.
        self.reports.mark_offered(std::slice::from_ref(report));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Reports;
    use crate::report::{FindNewReports, SaveReport};
    use pullbay_domain::report::{Kind, Program};

    #[test]
    fn a_discarded_report_is_gone_and_not_offered() {
        let reports = Reports::default();
        let saved = SaveReport::new(&reports).run(Program::Window, Kind::Freeze, "stuck", 7).unwrap();
        DiscardReport::new(&reports).run(&saved).unwrap();
        assert!(reports.written().is_empty());
        assert!(FindNewReports::new(&reports).run().is_empty());
    }
}
