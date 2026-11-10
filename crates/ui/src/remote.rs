//! Options' Remote control page: turning the phone page on, its port and
//! addresses, the paired devices, and pairing a phone with a QR code.
//!
//! The page only edits its draft and asks the daemon (`Command`s the
//! dialog sends for it): how remote control stands, every couple of
//! seconds while the page is shown, a pairing code, or a device's removal,
//! which takes effect at once. The settings themselves apply on Save, as
//! the other pages' do.

use std::time::{Duration, Instant};

use eframe::egui;

use pullbay_application::daemon::Command;
use pullbay_domain::pairing::{RemoteState, PAIRING_LIFETIME};
use pullbay_domain::settings::{Settings, REMOTE_PORT_RANGE};

use crate::errors;
use crate::i18n::{tr, trf};

/// How often the page asks how remote control stands while it is shown,
/// so a phone that just paired shows up.
const ASK_EVERY: Duration = Duration::from_secs(2);
/// The QR code's side, in points.
const QR_SIZE: f32 = 220.0;

/// A QR code's modules, row by row; true is dark.
pub struct Matrix {
    width: usize,
    dark: Vec<bool>,
}

impl Matrix {
    /// The QR code of `text`, at error correction level M.
    pub fn encode(text: &str) -> Option<Matrix> {
        let code = qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::M).ok()?;
        let dark = code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect();
        Some(Matrix { width: code.width(), dark })
    }
}

/// Draw `matrix` about `size` points wide: black on white whatever the
/// theme, with the quiet zone readers need around it.
fn draw_qr(ui: &mut egui::Ui, matrix: &Matrix, size: f32, label: &str) -> egui::Response {
    const QUIET: usize = 4;
    let modules = matrix.width + 2 * QUIET;
    let cell = (size / modules as f32).floor().max(1.0);
    let side = cell * modules as f32;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::WHITE);
    for (i, _) in matrix.dark.iter().enumerate().filter(|(_, dark)| **dark) {
        let (x, y) = ((i % matrix.width + QUIET) as f32, (i / matrix.width + QUIET) as f32);
        let cell_rect = egui::Rect::from_min_size(rect.min + egui::vec2(x * cell, y * cell), egui::vec2(cell, cell));
        painter.rect_filled(cell_rect, 0.0, egui::Color32::BLACK);
    }
    let label = label.to_string();
    response.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &label));
    response
}

/// The Remote control page being edited.
pub struct RemoteDraft {
    pub enabled: bool,
    pub port: u16,
    /// Listen on every interface, or on `addresses` only.
    pub all: bool,
    /// The addresses to listen on, separated by commas or spaces.
    pub addresses: String,
    /// What the daemon last said.
    state: Option<RemoteState>,
    /// When the page last asked.
    asked: Option<Instant>,
    /// Whether the pairing code is being shown (or was asked for).
    pairing: bool,
    /// The QR code of the link shown, and the link.
    qr: Option<(String, Matrix)>,
    /// Remote control as saved, to tell whether Save is still needed.
    saved: (bool, u16, Vec<String>),
}

impl RemoteDraft {
    pub fn new(s: &Settings) -> Self {
        RemoteDraft {
            enabled: s.remote_enabled,
            port: s.remote_port,
            all: s.remote_addresses.is_empty(),
            addresses: s.remote_addresses.join(", "),
            state: None,
            asked: None,
            pairing: false,
            qr: None,
            saved: (s.remote_enabled, s.remote_port, s.remote_addresses.clone()),
        }
    }

    /// The addresses to listen on: none means every interface.
    pub fn address_list(&self) -> Vec<String> {
        if self.all {
            return Vec::new();
        }
        self.addresses.split([',', ' ', ';']).map(str::trim).filter(|a| !a.is_empty()).map(str::to_string).collect()
    }

    /// Whether the fields differ from the settings saved.
    fn unsaved(&self) -> bool {
        (self.enabled, self.port, self.address_list()) != self.saved
    }

    /// Take the daemon's last answer.
    pub fn take_state(&mut self, state: Option<&RemoteState>) {
        self.state = state.cloned();
        let link = self.state.as_ref().and_then(|s| s.pairing.as_ref()).map(|p| p.link.clone());
        match link {
            Some(link) if self.qr.as_ref().is_none_or(|(shown, _)| *shown != link) => {
                self.qr = Matrix::encode(&link).map(|m| (link, m));
            }
            Some(_) => {}
            None => self.qr = None,
        }
    }

    /// What to ask the daemon now, while the page is shown.
    pub fn poll(&mut self) -> Option<Command> {
        if self.asked.is_some_and(|at| at.elapsed() < ASK_EVERY) {
            return None;
        }
        self.asked = Some(Instant::now());
        Some(Command::RemoteStatus)
    }

    /// What to tell the daemon when the dialog closes: withdraw a code
    /// still shown.
    pub fn closing(&self) -> Option<Command> {
        self.pairing.then_some(Command::CancelPairing)
    }
}

/// Draw the page, adding what to ask the daemon to `asks`.
pub fn tab(ui: &mut egui::Ui, draft: &mut RemoteDraft, now: u64, asks: &mut Vec<Command>) {
    ui.checkbox(&mut draft.enabled, tr("Let phones on this network control Pullbay"));
    ui.label(
        egui::RichText::new(tr(
            "A paired phone's browser can see the download list and add, pause, resume and remove downloads. Nothing listens on the network while this is off.",
        ))
        .weak(),
    );
    ui.add_space(6.0);
    ui.add_enabled_ui(draft.enabled, |ui| {
        egui::Grid::new("remote").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label(tr("Port"));
            ui.add(egui::DragValue::new(&mut draft.port).range(REMOTE_PORT_RANGE.0..=REMOTE_PORT_RANGE.1));
            ui.end_row();
            ui.label(tr("Listen on"));
            ui.vertical(|ui| {
                ui.radio_value(&mut draft.all, true, tr("Every network interface"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut draft.all, false, tr("Only these addresses:"));
                    ui.add_enabled(
                        !draft.all,
                        egui::TextEdit::singleline(&mut draft.addresses).hint_text("192.168.1.20").desired_width(200.0),
                    );
                });
            });
            ui.end_row();
        });
    });
    ui.label(
        egui::RichText::new(tr(
            "Your firewall may ask whether to let Pullbay's daemon accept connections: allow it on private networks only.",
        ))
        .weak(),
    );
    ui.add_space(6.0);
    if draft.unsaved() {
        ui.colored_label(ui.visuals().warn_fg_color, tr("Save to apply these changes."));
    }
    let state = draft.state.clone().unwrap_or_default();
    let listening = state.listener.page.clone();
    match (&listening, &state.listener.fault) {
        (Some(page), _) => {
            ui.label(trf!("The page is at {page}", page = page));
        }
        (None, Some(fault)) => {
            ui.colored_label(ui.visuals().error_fg_color, errors::remote_fault(fault));
        }
        (None, None) if draft.saved.0 && draft.state.is_some() => {
            ui.label(tr("Starting…"));
        }
        (None, None) => {
            ui.label(tr("Remote control is off."));
        }
    }
    if let Some(fingerprint) = &state.listener.fingerprint {
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Certificate fingerprint (SHA-256):"));
            ui.add(egui::Label::new(egui::RichText::new(fingerprint).monospace().small()).wrap());
        });
    }
    ui.add_space(6.0);
    pairing(ui, draft, &state, now, asks);
    ui.add_space(8.0);
    ui.separator();
    ui.strong(tr("Paired devices"));
    if state.devices.is_empty() {
        ui.label(egui::RichText::new(tr("None yet.")).weak());
    }
    egui::Grid::new("remote_devices").num_columns(4).spacing([12.0, 6.0]).striped(true).show(ui, |ui| {
        for device in &state.devices {
            let name = if device.name.is_empty() { tr("Unnamed device") } else { device.name.as_str() };
            ui.label(name);
            ui.label(trf!("Paired {date}", date = crate::columns::local_date(device.paired_at)));
            match device.last_seen {
                Some(seen) => ui.label(trf!("Last seen {date}", date = crate::columns::local_date(seen))),
                None => ui.label(""),
            };
            if ui.button(tr("Remove")).on_hover_text(tr("It stops working at once")).clicked() {
                asks.push(Command::RemoveDevice { id: device.id });
            }
            ui.end_row();
        }
    });
}

/// The Pair a phone button, or the QR code and code being shown.
fn pairing(ui: &mut egui::Ui, draft: &mut RemoteDraft, state: &RemoteState, now: u64, asks: &mut Vec<Command>) {
    let can_pair = state.listener.page.is_some();
    if !draft.pairing {
        let button = ui.add_enabled(can_pair, egui::Button::new(tr("Pair a phone…")));
        if button.on_disabled_hover_text(tr("Turn remote control on and save first")).clicked() {
            draft.pairing = true;
            asks.push(Command::StartPairing);
        }
        return;
    }
    match (&state.pairing, &draft.qr) {
        (Some(offer), Some((_, matrix))) => {
            ui.horizontal(|ui| {
                draw_qr(ui, matrix, QR_SIZE, tr("QR code for pairing a phone"));
                ui.vertical(|ui| {
                    ui.label(tr("Scan the QR code with the phone's camera and open the link."));
                    if let Some(page) = &state.listener.page {
                        ui.label(trf!("Or open {page} on the phone and type this code:", page = page));
                    }
                    ui.label(egui::RichText::new(offer.code.grouped()).monospace().size(18.0));
                    let left = offer.expires_at.saturating_sub(now).min(PAIRING_LIFETIME);
                    ui.label(trf!(
                        "The code works once, for {minutes}:{seconds} more.",
                        minutes = left / 60,
                        seconds = format!("{:02}", left % 60)
                    ));
                    ui.label(
                        egui::RichText::new(tr(
                            "The phone warns that the page's certificate is not trusted, as it is Pullbay's own. Before going on, check that the certificate's SHA-256 fingerprint is the one above.",
                        ))
                        .weak(),
                    );
                    if ui.button(tr("Done")).clicked() {
                        draft.pairing = false;
                        asks.push(Command::CancelPairing);
                    }
                });
            });
        }
        _ if draft.state.as_ref().is_some_and(|s| s.pairing.is_none()) && draft.asked.is_some() => {
            ui.horizontal(|ui| {
                ui.label(tr("The code was used or ran out."));
                if ui.add_enabled(can_pair, egui::Button::new(tr("Show a new code"))).clicked() {
                    asks.push(Command::StartPairing);
                }
                if ui.button(tr("Done")).clicked() {
                    draft.pairing = false;
                }
            });
        }
        _ => {
            ui.spinner();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_makes_a_square_qr_code() {
        let m = Matrix::encode("https://192.168.1.20:18443/#pair=0123456789ABCDEFGHJKMNPQRS").unwrap();
        assert!(m.width >= 21);
        assert_eq!(m.dark.len(), m.width * m.width);
        assert!(m.dark.iter().any(|d| *d) && m.dark.iter().any(|d| !*d));
    }

    #[test]
    fn the_addresses_are_listed_unless_every_interface_is_chosen() {
        let mut draft = RemoteDraft::new(&Settings::default());
        assert!(draft.all && !draft.enabled);
        draft.addresses = "192.168.1.20, 10.0.0.2 ;".into();
        assert!(draft.address_list().is_empty());
        draft.all = false;
        assert_eq!(draft.address_list(), ["192.168.1.20", "10.0.0.2"]);
        assert!(draft.unsaved());
    }

    #[test]
    fn the_page_asks_now_and_then_and_withdraws_its_code_on_closing() {
        let mut draft = RemoteDraft::new(&Settings::default());
        assert!(matches!(draft.poll(), Some(Command::RemoteStatus)));
        assert!(draft.poll().is_none(), "not again at once");
        assert!(draft.closing().is_none());
        draft.pairing = true;
        assert!(matches!(draft.closing(), Some(Command::CancelPairing)));
    }
}
