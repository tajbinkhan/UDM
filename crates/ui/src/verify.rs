//! A finished download's Verify checksum dialog: paste the checksum the
//! site published, and the dialog says whether the file matches. The file
//! is hashed on a worker, so the frame never waits on the disk.

use std::path::PathBuf;

use eframe::egui;
use udm_application::downloads::{Verified, VerifyChecksum};
use udm_domain::checksum::Checksum;
use udm_domain::download::DownloadEntry;
use udm_domain::id::DownloadId;
use udm_domain::ports::HashError;

use crate::i18n::{tr, trf};
use crate::pending::Pending;

pub struct VerifyDialog {
    pub id: DownloadId,
    path: PathBuf,
    name: String,
    /// The checksum as typed or pasted.
    text: String,
    hashing: Pending<Result<Verified, HashError>>,
    /// The last check: the checksum it compared with and what it found.
    result: Option<(Checksum, Result<Verified, HashError>)>,
    /// The checksum the running check compares with.
    checking: Option<Checksum>,
}

impl VerifyDialog {
    /// The dialog for a finished download, filled with the checksum it was
    /// added with. `None` when it has no file.
    pub fn new(e: &DownloadEntry) -> Option<Self> {
        let path = PathBuf::from(e.path.as_ref()?);
        Some(VerifyDialog {
            id: e.id,
            path,
            name: e.display_name(),
            text: e.checksum.as_ref().map(Checksum::to_string).unwrap_or_default(),
            hashing: Pending::default(),
            result: None,
            checking: None,
        })
    }
}

/// Whether the dialog stays open.
pub fn show(ctx: &egui::Context, d: &mut VerifyDialog) -> bool {
    if let Some(found) = d.hashing.take() {
        if let Some(sum) = d.checking.take() {
            d.result = Some((sum, found));
        }
    }
    let mut open = true;
    let mut close = false;
    crate::dialog::Window::new(tr("Verify checksum"))
        .id(egui::Id::new(("verify_checksum", d.id.get())))
        .dlu_width(320.0)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(&d.name);
            ui.label(tr("Paste the MD5, SHA-1, SHA-256, SHA-512 or CRC32 checksum the site gives for this file."));
            let input = ui.add(egui::TextEdit::singleline(&mut d.text).desired_width(f32::INFINITY));
            if input.changed() {
                d.result = None;
            }
            let parsed = (!d.text.trim().is_empty()).then(|| Checksum::parse(&d.text));
            let busy = d.hashing.busy();
            match &parsed {
                Some(Ok(sum)) => {
                    ui.label(trf!("Algorithm: {name}", name = sum.algorithm().to_string()));
                }
                Some(Err(_)) => {
                    ui.colored_label(ui.visuals().error_fg_color, tr("This is not a checksum UDM knows."));
                }
                None => {}
            }
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr("Reading the file..."));
                });
            } else if let Some((sum, found)) = &d.result {
                match found {
                    Ok(v) if v.matches => {
                        ui.colored_label(
                            egui::Color32::from_rgb(0x2e, 0x8b, 0x57),
                            tr("The file matches the checksum."),
                        );
                    }
                    Ok(v) => {
                        ui.colored_label(ui.visuals().error_fg_color, tr("The file does not match the checksum."));
                        ui.label(trf!("Expected: {digest}", digest = sum.hex()));
                        ui.label(trf!("File: {digest}", digest = v.actual.clone()));
                    }
                    Err(e) => {
                        ui.colored_label(ui.visuals().error_fg_color, tr("The file could not be read."));
                        ui.label(e.to_string());
                    }
                }
            }
            ui.horizontal(|ui| {
                let sum = parsed.and_then(Result::ok);
                if ui.add_enabled(sum.is_some() && !busy, egui::Button::new(tr("Verify"))).clicked() {
                    if let Some(sum) = sum {
                        let (path, check) = (d.path.clone(), sum.clone());
                        d.checking = Some(sum);
                        d.result = None;
                        d.hashing.start(ui.ctx(), move || {
                            VerifyChecksum::new(&*crate::ports::ports().hashes).run(&path, &check)
                        });
                    }
                }
                close |= ui.button(tr("Close")).clicked();
            });
        });
    open && !close
}
