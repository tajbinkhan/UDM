//! The gallery dialog: the pictures gallery-dl finds on a gallery page (a
//! Wikimedia Commons category, a Flickr album, a DeviantArt gallery), each
//! with a box to tick, and the one folder they are saved in. It opens on the
//! page alone, from the browser's "Download gallery" or from Add URL, and
//! asks the daemon to read it; the list fills in when the answer comes.
//! Ticked pictures are added as one batch, downloaded by Pullbay's own
//! engine, which asks the batch questions (`answers`) like the link
//! picker's.

use std::sync::atomic::{AtomicU64, Ordering};

use eframe::egui;
use pullbay_domain::gallery::{gallery_folder, Gallery};
use pullbay_domain::id::QueueId;
use pullbay_domain::package::Packaging;
use pullbay_domain::queue::{Queue, MAIN_QUEUE};
use pullbay_domain::settings::{Settings, GENERAL};
use pullbay_domain::tools::Tool;

use super::{browse_button, take_picked, Action, Picked};
use crate::a11y;
use crate::i18n::{tr, trf};

/// Numbers the read requests, so an answer to an earlier one is told apart.
static READ_REQUESTS: AtomicU64 = AtomicU64::new(1);

/// Why a gallery gave no pictures, as the window shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Unread {
    pub why: String,
    /// gallery-dl is not installed, so the dialog offers to install it.
    pub missing: bool,
}

/// Where the dialog is with the page.
enum Reading {
    /// Waiting to send the request (the daemon is not connected yet).
    Due,
    /// Asked; waiting for request `0`'s answer.
    Asked(u64),
    Found(Gallery),
    Failed(Unread),
}

pub struct GalleryDialog {
    page: String,
    user_agent: Option<String>,
    reading: Reading,
    checked: Vec<bool>,
    /// Only pictures whose name holds these words.
    filter: String,
    folder: String,
    picked: Picked,
    queue: QueueId,
    start_queue: bool,
    error: Option<String>,
}

impl GalleryDialog {
    pub fn new(page: String, user_agent: Option<String>) -> Self {
        GalleryDialog {
            page: page.trim().to_string(),
            user_agent,
            reading: Reading::Due,
            checked: Vec::new(),
            filter: String::new(),
            folder: String::new(),
            picked: Picked::default(),
            queue: MAIN_QUEUE,
            start_queue: true,
            error: None,
        }
    }

    /// The request to send now, if one is due: its number, the page and
    /// the browser's user agent.
    pub fn take_read(&mut self) -> Option<(u64, String, Option<String>)> {
        if !matches!(self.reading, Reading::Due) {
            return None;
        }
        let request = READ_REQUESTS.fetch_add(1, Ordering::Relaxed);
        self.reading = Reading::Asked(request);
        Some((request, self.page.clone(), self.user_agent.clone()))
    }

    /// The daemon's answer to `request`: the gallery, saved by default in a
    /// folder named after it in `base`, or why there is none. An answer to
    /// an earlier request is dropped.
    pub fn heard(&mut self, request: u64, found: Result<Gallery, Unread>, base: &std::path::Path) {
        if !matches!(self.reading, Reading::Asked(r) if r == request) {
            return;
        }
        self.reading = match found {
            Ok(gallery) => {
                self.checked = vec![true; gallery.pictures.len()];
                self.folder = gallery_folder(base, &gallery.title).display().to_string();
                Reading::Found(gallery)
            }
            Err(unread) => Reading::Failed(unread),
        };
    }

    /// The pictures the filter shows, by their place in the gallery.
    fn visible(&self) -> Vec<usize> {
        match &self.reading {
            Reading::Found(g) => g.matching(&self.filter),
            _ => Vec::new(),
        }
    }

    /// The ticked pictures the filter shows.
    fn ticked(&self) -> Vec<usize> {
        self.visible().into_iter().filter(|&i| self.checked[i]).collect()
    }

    /// The ticked pictures as downloads waiting in the dialog's queue, or
    /// why the folder will not do.
    fn downloads(&self) -> Result<Vec<pullbay_domain::download::Download>, String> {
        let Reading::Found(gallery) = &self.reading else {
            return Ok(Vec::new());
        };
        let folder = self.folder.trim();
        if folder.is_empty() {
            return Err(tr("Choose the folder to save the files to.").to_string());
        }
        let folder = std::path::Path::new(folder);
        if !folder.is_absolute() {
            return Err(trf!("The folder must be a full path: {folder}", folder = folder.display()));
        }
        let mut items = gallery.downloads(&self.ticked(), folder, &self.page);
        for item in &mut items {
            item.user_agent = self.user_agent.clone();
            item.queue = Some(self.queue);
        }
        Ok(items)
    }
}

/// The folder a gallery goes in by default, before its own subfolder: the
/// General category's.
pub fn base_folder(settings: &Settings) -> std::path::PathBuf {
    settings.category_folder(GENERAL, &crate::ports::ports().downloads_dir())
}

/// Draw the dialog. `installing` is whether gallery-dl is being installed.
/// Returns false once it is closed.
#[allow(clippy::too_many_arguments)]
pub fn dialog(
    ctx: &egui::Context,
    d: &mut GalleryDialog,
    queues: &[Queue],
    settings: &Settings,
    connected: bool,
    installing: bool,
    actions: &mut Vec<Action>,
) -> bool {
    take_picked(&d.picked, &mut d.folder);
    if connected {
        if let Some((request, page, user_agent)) = d.take_read() {
            actions.push(Action::ReadGallery { request, page, user_agent });
        }
    }
    let mut open = true;
    let mut close = false;
    crate::dialog::Window::new(tr("Download gallery"))
        .id(egui::Id::new("gallery_picker"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .min_width(600.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            if let Reading::Found(g) = &d.reading {
                if !g.title.is_empty() {
                    ui.strong(&g.title);
                }
            }
            ui.add(egui::Label::new(egui::RichText::new(&d.page).weak()).truncate());
            match &d.reading {
                Reading::Due | Reading::Asked(_) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr("Reading the gallery with gallery-dl..."));
                    });
                }
                Reading::Failed(unread) => {
                    ui.colored_label(ui.visuals().error_fg_color, &unread.why);
                    let missing = unread.missing;
                    ui.horizontal(|ui| {
                        if missing {
                            let install = egui::Button::new(trf!("Install {tool}", tool = Tool::GalleryDl.name()));
                            if ui.add_enabled(connected && !installing, install).clicked() {
                                actions.push(Action::InstallTool(Tool::GalleryDl));
                            }
                            if installing {
                                ui.spinner();
                            }
                        }
                        if ui.add_enabled(connected && !installing, egui::Button::new(tr("Retry"))).clicked() {
                            d.reading = Reading::Due;
                        }
                    });
                }
                Reading::Found(_) => list(ui, d),
            }
            if matches!(d.reading, Reading::Found(_)) {
                ui.add_space(6.0);
                egui::Grid::new("gallery_target").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label(tr("Save to"));
                    ui.horizontal(|ui| {
                        let field = ui.add(egui::TextEdit::singleline(&mut d.folder).desired_width(380.0));
                        a11y::label(&field, tr("Save to"));
                        browse_button(ui, tr("Save to"), &d.folder, &d.picked);
                    });
                    ui.end_row();
                    if settings.list_queue_panel {
                        ui.label(tr("Queue"));
                        ui.horizontal(|ui| {
                            super::tools::queue_combo(ui, "gallery_queue", &mut d.queue, queues);
                            ui.checkbox(&mut d.start_queue, tr("Start the queue now"));
                        });
                        ui.end_row();
                    }
                });
            }
            if let Some(e) = &d.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            let ticked = d.ticked();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let add = egui::Button::new(trf!("Add {count}", count = ticked.len()));
                if ui.add_enabled(connected && !ticked.is_empty(), add).clicked() {
                    match d.downloads() {
                        Ok(items) => {
                            actions.push(Action::AddMany {
                                items,
                                queue: d.queue,
                                start_queue: d.start_queue,
                                packaging: Packaging::None,
                            });
                            close = true;
                        }
                        Err(why) => d.error = Some(why),
                    }
                }
                if ui.button(tr("Cancel")).clicked() {
                    close = true;
                }
            });
        });
    open && !close
}

/// The pictures, with the filter and the boxes to tick them.
fn list(ui: &mut egui::Ui, d: &mut GalleryDialog) {
    ui.horizontal(|ui| {
        ui.label(tr("Find"));
        let field =
            ui.add(egui::TextEdit::singleline(&mut d.filter).hint_text(tr("Words in the name")).desired_width(200.0));
        a11y::label(&field, tr("Find"));
    });
    let visible = d.visible();
    let Reading::Found(gallery) = &d.reading else {
        return;
    };
    ui.horizontal(|ui| {
        ui.label(trf!("{shown} of {total} pictures shown", shown = visible.len(), total = gallery.pictures.len()));
        if ui.small_button(tr("Check all")).clicked() {
            visible.iter().for_each(|&i| d.checked[i] = true);
        }
        if ui.small_button(tr("Uncheck all")).clicked() {
            visible.iter().for_each(|&i| d.checked[i] = false);
        }
    });
    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
        egui::Grid::new("gallery_grid").striped(true).num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
            ui.label("");
            ui.strong("#");
            ui.strong(tr("Name"));
            ui.strong(tr("Size"));
            ui.end_row();
            for &i in &visible {
                let p = &gallery.pictures[i];
                let name = p.file_name(i + 1);
                let tick = ui.checkbox(&mut d.checked[i], "");
                a11y::label(&tick, &name);
                ui.weak((i + 1).to_string());
                ui.add_sized([400.0, 18.0], egui::Label::new(name.as_str()).truncate()).on_hover_text(p.url.as_str());
                ui.weak(p.dimensions().unwrap_or_default());
                ui.end_row();
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use pullbay_domain::gallery::Picture;
    use std::path::Path;

    fn gallery(names: &[&str]) -> Gallery {
        let pictures = names
            .iter()
            .enumerate()
            .map(|(i, n)| Picture {
                url: format!("https://i/{i}.jpg"),
                name: n.to_string(),
                extension: "jpg".into(),
                ..Picture::default()
            })
            .collect();
        Gallery { title: "Cats".into(), pictures }
    }

    fn base() -> std::path::PathBuf {
        std::env::temp_dir().join("pullbay-gallery-test")
    }

    #[test]
    fn it_reads_the_page_once_and_takes_only_its_own_answer() {
        let mut d = GalleryDialog::new(" https://g/cats ".into(), Some("UA".into()));
        let (request, page, ua) = d.take_read().unwrap();
        assert_eq!((page.as_str(), ua.as_deref()), ("https://g/cats", Some("UA")));
        assert!(d.take_read().is_none(), "asked once");
        d.heard(request + 100, Ok(gallery(&["a"])), &base());
        assert!(matches!(d.reading, Reading::Asked(_)), "another request's answer is dropped");
        d.heard(request, Ok(gallery(&["a", "b"])), &base());
        assert_eq!(d.ticked(), [0, 1], "every picture starts ticked");
        assert_eq!(Path::new(&d.folder), base().join("Cats"));
    }

    #[test]
    fn a_failure_can_be_read_again() {
        let mut d = GalleryDialog::new("https://g/cats".into(), None);
        let (request, ..) = d.take_read().unwrap();
        d.heard(request, Err(Unread { why: "no".into(), missing: true }), &base());
        assert!(matches!(&d.reading, Reading::Failed(u) if u.missing));
        assert!(d.take_read().is_none());
        d.reading = Reading::Due;
        assert!(d.take_read().is_some());
    }

    #[test]
    fn the_ticked_pictures_go_to_the_folder_in_the_queue() {
        let mut d = GalleryDialog::new("https://g/cats".into(), Some("UA".into()));
        let (request, ..) = d.take_read().unwrap();
        d.heard(request, Ok(gallery(&["Red cat", "Blue cat", "Red dog"])), &base());
        d.filter = "red".into();
        d.checked[2] = false;
        let items = d.downloads().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].filename.as_deref(), Some("Red cat.jpg"));
        assert_eq!(items[0].user_agent.as_deref(), Some("UA"));
        assert_eq!(items[0].queue, Some(MAIN_QUEUE));
        assert_eq!(items[0].referrer.as_deref(), Some("https://g/cats"));
        d.folder = "relative".into();
        assert!(d.downloads().is_err());
        d.folder = " ".into();
        assert!(d.downloads().is_err());
    }
}
