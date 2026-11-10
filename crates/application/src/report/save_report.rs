use pullbay_domain::ports::{FilesError, ProblemReports};
use pullbay_domain::report::{Kind, Program, Report, SavedReport};

/// Write a report on something the window saw: its own frame that did not
/// finish, or a daemon that stopped answering. It holds this copy's
/// version, system and install and the end of the program's log; a freeze
/// seen from outside has no backtrace.
pub struct SaveReport<'a, P: ?Sized> {
    reports: &'a P,
}

impl<'a, P: ProblemReports + ?Sized> SaveReport<'a, P> {
    pub fn new(reports: &'a P) -> Self {
        SaveReport { reports }
    }

    /// Write the report on `program` (whose log it ends with) at `at_ms`
    /// (milliseconds since 1970), saying `what`.
    pub fn run(&self, program: Program, kind: Kind, what: &str, at_ms: u64) -> Result<SavedReport, FilesError> {
        let (log_name, log_tail) = self.reports.log_tail(program);
        let report = Report {
            program,
            kind,
            copy: self.reports.this_copy(),
            at_ms,
            thread: None,
            what: what.to_string(),
            location: None,
            backtrace: None,
            log_name,
            log_tail,
        };
        let saved = self.reports.save(&report)?;
        tracing::info!(program = program.name(), kind = kind.name(), path = %saved.path.display(), "saved a problem report");
        Ok(saved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Reports;

    #[test]
    fn a_freeze_report_holds_the_copy_and_the_log() {
        let reports = Reports::default();
        let saved = SaveReport::new(&reports).run(Program::Window, Kind::Freeze, "stuck", 5_000).unwrap();
        assert_eq!(saved.summary.kind, Kind::Freeze);
        assert_eq!(saved.summary.what, "stuck");
        let written = reports.written();
        assert_eq!(written.len(), 1);
        let r = &written[0];
        assert_eq!((r.program, r.at_ms, r.backtrace.as_deref()), (Program::Window, 5_000, None));
        assert_eq!(r.copy.version, "9.9.9");
        assert_eq!(r.log_name, "window.log");
        assert_eq!(r.log_tail, ["last window line"]);
    }

    #[test]
    fn a_report_that_cannot_be_written_says_why() {
        let reports = Reports::refusing();
        let e = SaveReport::new(&reports).run(Program::Daemon, Kind::Silent, "quiet", 1).unwrap_err();
        assert_eq!(e, FilesError("the disk is full".into()));
    }
}
