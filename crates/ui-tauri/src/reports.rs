//! Problem reports the window offers: one a crash left for the next start,
//! one the watchdog wrote while a window or the state thread stood still,
//! or one to write when the daemon stops answering. The offers wait in
//! line; the main window's strip shows the first. Plain functions, tested
//! without Tauri.

use std::collections::VecDeque;

use udm_domain::report::{Kind, Program, SavedReport};

use crate::contract::{ReportStrip, StripTone};
use crate::i18n::{fill, Language};

/// One report offer.
#[derive(Debug, Clone, PartialEq)]
pub enum Offer {
    /// A window or the state thread stopped for `secs`; the report the
    /// watchdog wrote meanwhile, if it could.
    Froze { secs: u64, draft: Option<SavedReport> },
    /// The daemon has not answered for `secs`, its connection open.
    Silent { secs: u64 },
    /// The report asked for is being written.
    Saving,
    /// A report is saved, to open an issue with. `earlier`: the window
    /// found it rather than wrote it; `more`: other new reports found
    /// with it.
    Saved { report: SavedReport, earlier: bool, more: usize },
}

/// The offers, the first shown.
#[derive(Debug, Default)]
pub struct Offers(pub VecDeque<Offer>);

impl Offers {
    pub fn first(&self) -> Option<&Offer> {
        self.0.front()
    }

    /// Reports a crash or an earlier stall left, newest first.
    pub fn found(&mut self, mut found: Vec<SavedReport>) {
        if found.is_empty() {
            return;
        }
        let report = found.remove(0);
        self.0.push_back(Offer::Saved { report, earlier: true, more: found.len() });
    }

    /// A window or the state thread stood still for `lasted_ms`.
    pub fn froze(&mut self, lasted_ms: u64, draft: Option<SavedReport>) {
        self.0.push_back(Offer::Froze { secs: lasted_ms.div_ceil(1000), draft });
    }

    /// The daemon stopped answering; once, until it answers again.
    pub fn silent(&mut self, secs: u64) {
        if !self.0.iter().any(|o| matches!(o, Offer::Silent { .. })) {
            self.0.push_back(Offer::Silent { secs });
        }
    }

    pub fn answering(&mut self) {
        self.0.retain(|o| !matches!(o, Offer::Silent { .. }));
    }

    /// The first offer's report is being written.
    pub fn saving(&mut self) {
        if let Some(first) = self.0.front_mut() {
            *first = Offer::Saving;
        }
    }

    /// The first offer's report is written, or the watchdog's kept.
    pub fn saved(&mut self, report: SavedReport) {
        let offer = Offer::Saved { report, earlier: false, more: 0 };
        match self.0.front_mut() {
            Some(first) => *first = offer,
            None => self.0.push_back(offer),
        }
    }

    /// Writing it failed: the offer goes.
    pub fn dismiss(&mut self) {
        self.0.pop_front();
    }
}

/// The main window's strip for the first offer.
pub fn strip(lang: &'static Language, offers: &Offers) -> Option<ReportStrip> {
    let blank = |text: String, tone: StripTone| ReportStrip {
        text,
        tone,
        more: String::new(),
        save: false,
        restart: false,
        issue: false,
        busy: false,
    };
    Some(match offers.first()? {
        Offer::Froze { secs, .. } => ReportStrip {
            save: true,
            ..blank(
                fill(
                    lang.tr("The window stopped responding for {seconds} seconds."),
                    &[("seconds", &secs.to_string())],
                ),
                StripTone::Warn,
            )
        },
        Offer::Silent { secs } => ReportStrip {
            save: true,
            restart: true,
            ..blank(
                fill(
                    lang.tr("The UDM daemon has not answered for {seconds} seconds."),
                    &[("seconds", &secs.to_string())],
                ),
                StripTone::Warn,
            )
        },
        Offer::Saving => {
            ReportStrip { busy: true, ..blank(lang.tr("Saving the report...").to_string(), StripTone::Info) }
        }
        Offer::Saved { report, earlier, more } => {
            let text = match (earlier, report.summary.kind, report.summary.program) {
                (true, Kind::Crash, Program::Window) => "The UDM window closed unexpectedly and saved a report.",
                (true, Kind::Crash, Program::Daemon) => "The UDM daemon closed unexpectedly and saved a report.",
                (true, Kind::Crash, Program::Host) => {
                    "UDM's browser connection closed unexpectedly and saved a report."
                }
                (true, Kind::Crash, Program::Cli) => {
                    "The UDM command-line tool closed unexpectedly and saved a report."
                }
                (true, ..) => "UDM saved a report on a problem it saw earlier.",
                (false, ..) => "The report is saved.",
            };
            ReportStrip {
                more: if *more > 0 {
                    fill(lang.tr("{count} more are in the same folder."), &[("count", &more.to_string())])
                } else {
                    String::new()
                },
                issue: true,
                ..blank(lang.tr(text).to_string(), StripTone::Info)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use udm_domain::report::Summary;

    fn report(kind: Kind) -> SavedReport {
        SavedReport {
            path: "/r/udm-report.txt".into(),
            summary: Summary {
                program: Program::Daemon,
                kind,
                at_ms: 5,
                version: Some("0.4.0".into()),
                what: String::new(),
            },
        }
    }

    fn lang() -> &'static Language {
        crate::i18n::resolve("en", &[])
    }

    #[test]
    fn a_crash_found_at_start_is_offered_with_the_rest_counted() {
        let mut o = Offers::default();
        o.found(vec![report(Kind::Crash), report(Kind::Freeze)]);
        let s = strip(lang(), &o).unwrap();
        assert_eq!(s.text, "The UDM daemon closed unexpectedly and saved a report.");
        assert_eq!(s.more, "1 more are in the same folder.");
        assert!(s.issue && !s.save);
    }

    #[test]
    fn a_silent_daemon_is_offered_once_until_it_answers() {
        let mut o = Offers::default();
        o.silent(30);
        o.silent(35);
        assert_eq!(o.0.len(), 1);
        assert!(strip(lang(), &o).unwrap().restart);
        o.answering();
        assert!(o.first().is_none());
    }

    #[test]
    fn saving_turns_the_offer_into_a_saved_report() {
        let mut o = Offers::default();
        o.froze(12_400, None);
        assert_eq!(strip(lang(), &o).unwrap().text, "The window stopped responding for 13 seconds.");
        o.saving();
        assert!(strip(lang(), &o).unwrap().busy);
        o.saved(report(Kind::Freeze));
        assert_eq!(strip(lang(), &o).unwrap().text, "The report is saved.");
    }
}
