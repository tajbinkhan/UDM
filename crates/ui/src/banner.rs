//! Strips under the toolbar for what the window has to say: the daemon
//! connection when it is not up, the last error, notices, the download
//! quota, a newer release, a download waiting for a new link and a problem
//! to report. Each is
//! shown only while it applies, and its text wraps so its buttons stay in
//! view. Pullbay's window has no status bar; this is where its messages went.

use eframe::egui::{self, Color32};

use pullbay_domain::report::{Kind, Program};

use crate::i18n::{tr, trf};
use crate::state::ReportOffer;
use crate::Action;

/// The connection to the daemon, as the strip shows it.
pub enum Connection<'a> {
    Up,
    Connecting,
    /// Lost, and why.
    Lost(&'a str),
}

/// A newer release on offer.
pub struct Release<'a> {
    pub version: &'a str,
    pub url: &'a str,
    /// It has a setup Pullbay can install.
    pub installable: bool,
}

/// Everything the strips need for one frame.
pub struct Notes<'a> {
    pub connection: Connection<'a>,
    /// The download whose next browser download gives it a new link.
    pub refreshing: Option<String>,
    pub error: Option<&'a str>,
    /// The error is an older daemon not knowing a request: the error strip
    /// offers to restart it.
    pub stale_daemon: bool,
    pub notice: Option<&'a str>,
    pub quota: Option<&'a str>,
    /// The speed limit is lifted while nobody uses the computer.
    pub limit_lifted: bool,
    pub release: Option<Release<'a>>,
    /// The problem report on offer first.
    pub report: Option<&'a ReportOffer>,
}

impl Notes<'_> {
    /// Whether any strip is shown.
    pub fn any(&self) -> bool {
        !matches!(self.connection, Connection::Up)
            || self.refreshing.is_some()
            || self.error.is_some()
            || self.notice.is_some()
            || self.quota.is_some()
            || self.limit_lifted
            || self.release.is_some()
            || self.report.is_some()
    }
}

/// Strips the user closed.
#[derive(Default)]
pub struct Dismissed {
    pub error: bool,
    pub notice: bool,
    /// The newer release's strip, for that version.
    pub release: bool,
}

#[derive(Clone, Copy)]
enum Tone {
    Error,
    Warn,
    Info,
}

pub fn show(ui: &mut egui::Ui, notes: &Notes, connected: bool, actions: &mut Vec<Action>) -> Dismissed {
    let mut dismissed = Dismissed::default();
    ui.spacing_mut().item_spacing.y = 3.0;
    match notes.connection {
        Connection::Up => {}
        Connection::Connecting => strip(ui, Tone::Info, |ui| {
            ui.spinner();
            ui.label(tr("Connecting to the Pullbay daemon..."));
        }),
        Connection::Lost(why) => strip(ui, Tone::Error, |ui| {
            ui.label(trf!("Disconnected from the Pullbay daemon: {reason}", reason = why));
            if ui.button(tr("Retry")).clicked() {
                actions.push(Action::Reconnect);
            }
            if ui
                .button(tr("Restart daemon"))
                .on_hover_text(tr(
                    "Stop the daemon if it is running, then start it again. Downloads it was running resume.",
                ))
                .clicked()
            {
                actions.push(Action::RestartDaemon);
            }
        }),
    }
    if let Some(error) = notes.error {
        strip(ui, Tone::Warn, |ui| {
            ui.label(error);
            if notes.stale_daemon && ui.button(tr("Restart daemon")).clicked() {
                actions.push(Action::RestartDaemon);
            }
            if close(ui) {
                dismissed.error = true;
            }
        });
    }
    if let Some(offer) = notes.report {
        report(ui, offer, actions);
    }
    if let Some(quota) = notes.quota {
        strip(ui, Tone::Warn, |ui| {
            ui.label(quota);
        });
    }
    if notes.limit_lifted {
        strip(ui, Tone::Info, |ui| {
            ui.label(tr("The speed limit is lifted while you are away. It applies again when you use the computer."));
        });
    }
    if let Some(name) = &notes.refreshing {
        strip(ui, Tone::Info, |ui| {
            ui.label(trf!("Your next browser download gives {name} a new link", name = name));
            if ui.add_enabled(connected, egui::Button::new(tr("Stop waiting"))).clicked() {
                actions.push(Action::CancelRefresh);
            }
        });
    }
    if let Some(notice) = notes.notice {
        strip(ui, Tone::Info, |ui| {
            ui.label(notice);
            if close(ui) {
                dismissed.notice = true;
            }
        });
    }
    if let Some(r) = &notes.release {
        strip(ui, Tone::Info, |ui| {
            ui.hyperlink_to(trf!("Pullbay {version} is available", version = r.version), r.url);
            // About checks again and offers Update now.
            if r.installable && ui.add_enabled(connected, egui::Button::new(tr("Update now"))).clicked() {
                actions.push(Action::CheckUpdate);
            }
            if close(ui) {
                dismissed.release = true;
            }
        });
    }
    dismissed
}

/// The strip for a problem to report. Each button acts on the first offer.
fn report(ui: &mut egui::Ui, offer: &ReportOffer, actions: &mut Vec<Action>) {
    let save = |ui: &mut egui::Ui, actions: &mut Vec<Action>| {
        if ui
            .button(tr("Save a report"))
            .on_hover_text(tr("Write a report with the version, the system and the end of the log to Pullbay's data folder. Nothing is sent."))
            .clicked()
        {
            actions.push(Action::SaveReport);
        }
    };
    match offer {
        ReportOffer::Froze { secs, .. } => strip(ui, Tone::Warn, |ui| {
            ui.label(trf!("The window stopped responding for {seconds} seconds.", seconds = secs));
            save(ui, actions);
            if close(ui) {
                actions.push(Action::DismissReport);
            }
        }),
        ReportOffer::Silent { secs } => strip(ui, Tone::Warn, |ui| {
            ui.label(trf!("The Pullbay daemon has not answered for {seconds} seconds.", seconds = secs));
            save(ui, actions);
            if ui.button(tr("Restart daemon")).clicked() {
                actions.push(Action::RestartDaemon);
            }
            if close(ui) {
                actions.push(Action::DismissReport);
            }
        }),
        ReportOffer::Saving => strip(ui, Tone::Info, |ui| {
            ui.spinner();
            ui.label(tr("Saving the report..."));
        }),
        ReportOffer::Saved { report, earlier, more } => strip(ui, Tone::Info, |ui| {
            ui.label(match (earlier, report.summary.kind, report.summary.program) {
                (true, Kind::Crash, Program::Window) => {
                    tr("The Pullbay window closed unexpectedly and saved a report.")
                }
                (true, Kind::Crash, Program::Daemon) => {
                    tr("The Pullbay daemon closed unexpectedly and saved a report.")
                }
                (true, Kind::Crash, Program::Host) => {
                    tr("Pullbay's browser connection closed unexpectedly and saved a report.")
                }
                (true, Kind::Crash, Program::Cli) => {
                    tr("The Pullbay command-line tool closed unexpectedly and saved a report.")
                }
                (true, ..) => tr("Pullbay saved a report on a problem it saw earlier."),
                (false, ..) => tr("The report is saved."),
            });
            if *more > 0 {
                ui.label(trf!("{count} more are in the same folder.", count = more));
            }
            if ui
                .button(tr("Open an issue"))
                .on_hover_text(tr("Open a new issue on GitHub in your browser with the version and system filled in, and show the report file to attach to it. Nothing is sent until you submit the issue."))
                .clicked()
            {
                actions.push(Action::OpenIssue);
            }
            if ui.button(tr("Show the report")).clicked() {
                actions.push(Action::ShowReport);
            }
            if close(ui) {
                actions.push(Action::DismissReport);
            }
        }),
    }
}

/// One strip: a tinted band whose contents wrap.
fn strip(ui: &mut egui::Ui, tone: Tone, add: impl FnOnce(&mut egui::Ui)) {
    let visuals = ui.visuals();
    let accent = match tone {
        Tone::Error => visuals.error_fg_color,
        Tone::Warn => visuals.warn_fg_color,
        Tone::Info => visuals.hyperlink_color,
    };
    let fill = blend(visuals.panel_fill, accent, 0.14);
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0_f32, blend(visuals.panel_fill, accent, 0.45)))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(add);
        });
}

/// A small close box; true when clicked.
fn close(ui: &mut egui::Ui) -> bool {
    let close = ui.add(egui::Button::new("×").frame(false)).on_hover_text(tr("Close"));
    crate::a11y::label(&close, tr("Close"));
    close.clicked()
}

/// `a` moved `t` of the way to `b`.
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}
