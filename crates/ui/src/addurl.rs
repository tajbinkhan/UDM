//! Tasks > Add new download: the address to download, a checksum to compare
//! the file with when it finishes, and, if the site wants one, a login. OK
//! adds it; the save-as dialog follows as for any download. Download gallery
//! opens the gallery dialog on the address instead, for a page of pictures.

use eframe::egui;
use udm_domain::checksum::Checksum;
use udm_domain::download::{Download, DownloadLogin};

use crate::i18n::tr;

pub struct AddDialog {
    url: String,
    /// A checksum the site publishes, optional.
    checksum: String,
    use_login: bool,
    user: String,
    password: String,
    /// Put the cursor in the address on the first frame.
    focus: bool,
}

impl AddDialog {
    /// Start with the clipboard's link, if it holds one.
    pub fn new(clipboard: Option<String>) -> Self {
        let url = clipboard.map(|t| t.trim().to_string()).filter(|t| crate::is_download_url(t)).unwrap_or_default();
        AddDialog {
            url,
            checksum: String::new(),
            use_login: false,
            user: String::new(),
            password: String::new(),
            focus: true,
        }
    }
}

/// What the dialog asked for this frame.
pub enum Outcome {
    Open,
    Add(Box<Download>),
    /// Open the gallery dialog on this page.
    Gallery(String),
    Close,
}

pub fn show(ctx: &egui::Context, d: &mut AddDialog, connected: bool) -> Outcome {
    let mut outcome = Outcome::Open;
    let mut open = true;
    crate::dialog::Window::new(tr("Enter new address to download"))
        .id(egui::Id::new("add_url"))
        .dlu_width(373.0)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let url = d.url.trim().to_string();
            let checksum = Some(d.checksum.trim()).filter(|c| !c.is_empty()).map(Checksum::parse);
            let checksum_ok = !matches!(checksum, Some(Err(_)));
            let can_add = connected && crate::is_download_url(&url) && checksum_ok;
            let mut ok = false;
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(tr("Address"));
                        let input = ui.add(egui::TextEdit::singleline(&mut d.url).desired_width(420.0));
                        if std::mem::take(&mut d.focus) {
                            input.request_focus();
                        }
                        ok |= input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("Checksum"));
                        ui.add(
                            egui::TextEdit::singleline(&mut d.checksum)
                                .hint_text(tr("optional, checked when the download finishes"))
                                .desired_width(420.0),
                        );
                    });
                    if !checksum_ok {
                        ui.colored_label(ui.visuals().error_fg_color, tr("This is not a checksum UDM knows."));
                    }
                    ui.group(|ui| {
                        ui.checkbox(&mut d.use_login, tr("Use authorization"));
                        ui.add_enabled_ui(d.use_login, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(tr("Login"));
                                ui.add(egui::TextEdit::singleline(&mut d.user).desired_width(130.0));
                                ui.label(tr("Password"));
                                ui.add(egui::TextEdit::singleline(&mut d.password).password(true).desired_width(130.0));
                            });
                        });
                    });
                });
                ui.vertical(|ui| {
                    let size = egui::vec2(80.0, 0.0);
                    ok |= ui.add_enabled(can_add, egui::Button::new(tr("OK")).min_size(size)).clicked();
                    if ui.add(egui::Button::new(tr("Cancel")).min_size(size)).clicked() {
                        outcome = Outcome::Close;
                    }
                    let page = udm_domain::gallery::gallery_page(&url).ok();
                    let gallery = ui
                        .add_enabled(
                            connected && page.is_some(),
                            egui::Button::new(tr("Download gallery")).min_size(size),
                        )
                        .on_hover_text(tr("List the pictures on this page with gallery-dl, to pick which to download"));
                    if let Some(page) = page.filter(|_| gallery.clicked()) {
                        outcome = Outcome::Gallery(page.as_str().to_string());
                    }
                });
            });
            if ok && can_add {
                let login = (d.use_login && !d.user.trim().is_empty())
                    .then(|| DownloadLogin { username: d.user.trim().to_string(), password: Some(d.password.clone()) });
                let checksum = checksum.and_then(Result::ok);
                outcome = Outcome::Add(Box::new(Download { url: url.into(), login, checksum, ..Download::default() }));
            }
        });
    if !open {
        outcome = Outcome::Close;
    }
    outcome
}
