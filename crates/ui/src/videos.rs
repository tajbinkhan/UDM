//! The playlist dialog: the videos of a playlist or channel page the
//! browser's video button found (yt-dlp listed them), each with a box to
//! tick, one quality for all of them, and the queue they wait in. Ticked
//! videos are added as one batch, which asks the batch questions
//! (`answers`) like the link picker's.

use eframe::egui;
use pullbay_domain::id::QueueId;
use pullbay_domain::media::{Playlist, PlaylistQuality, Target};
use pullbay_domain::package::Packaging;
use pullbay_domain::queue::{Queue, MAIN_QUEUE};
use pullbay_domain::settings::Settings;

use super::Action;
use crate::i18n::{tr, trf};

pub struct VideosDialog {
    page: String,
    user_agent: Option<String>,
    playlist: Playlist,
    checked: Vec<bool>,
    /// Only videos whose title holds this.
    filter: String,
    quality: PlaylistQuality,
    queue: QueueId,
    start_queue: bool,
}

impl VideosDialog {
    pub fn new(page: String, playlist: Playlist, user_agent: Option<String>) -> Self {
        let checked = vec![true; playlist.entries.len()];
        VideosDialog {
            page,
            user_agent,
            playlist,
            checked,
            filter: String::new(),
            quality: PlaylistQuality::Best,
            queue: MAIN_QUEUE,
            start_queue: true,
        }
    }

    /// The videos the filter shows, by their place in the playlist.
    fn visible(&self) -> Vec<usize> {
        let words = self.filter.trim().to_lowercase();
        (0..self.playlist.entries.len())
            .filter(|&i| words.is_empty() || self.playlist.entries[i].title.to_lowercase().contains(&words))
            .collect()
    }

    fn ticked(&self) -> Vec<usize> {
        let visible = self.visible();
        visible.into_iter().filter(|&i| self.checked[i]).collect()
    }
}

/// How a quality reads in the menu.
fn quality_label(q: PlaylistQuality) -> String {
    match q {
        PlaylistQuality::Best => tr("Best quality").to_string(),
        PlaylistQuality::UpTo(h) => trf!("Up to {height}p", height = h),
        PlaylistQuality::Sound(t) => trf!("Sound only, {format}", format = sound_name(t)),
    }
}

fn sound_name(t: Target) -> &'static str {
    match t {
        Target::Mp3 => "MP3",
        Target::M4a => "M4A",
        Target::Opus => "Opus",
        Target::Flac => "FLAC",
        Target::Mp4 => "MP4",
    }
}

/// `secs` as `m:ss` or `h:mm:ss`.
fn length(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Draw the dialog. Returns false once it is closed.
pub fn dialog(
    ctx: &egui::Context,
    d: &mut VideosDialog,
    queues: &[Queue],
    settings: &Settings,
    connected: bool,
    actions: &mut Vec<Action>,
) -> bool {
    let mut open = true;
    let mut close = false;
    crate::dialog::Window::new(tr("Download videos from the playlist"))
        .id(egui::Id::new("videos_picker"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .min_width(600.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            if !d.playlist.title.trim().is_empty() {
                ui.strong(d.playlist.title.trim());
            }
            ui.add(egui::Label::new(egui::RichText::new(&d.page).weak()).truncate());
            ui.horizontal(|ui| {
                ui.label(tr("Find"));
                ui.add(
                    egui::TextEdit::singleline(&mut d.filter).hint_text(tr("Words in the title")).desired_width(200.0),
                );
            });
            let visible = d.visible();
            ui.horizontal(|ui| {
                ui.label(trf!(
                    "{shown} of {total} videos shown",
                    shown = visible.len(),
                    total = d.playlist.entries.len()
                ));
                if ui.small_button(tr("Check all")).clicked() {
                    visible.iter().for_each(|&i| d.checked[i] = true);
                }
                if ui.small_button(tr("Uncheck all")).clicked() {
                    visible.iter().for_each(|&i| d.checked[i] = false);
                }
            });
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                egui::Grid::new("videos_grid").striped(true).num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
                    ui.label("");
                    ui.strong("#");
                    ui.strong(tr("Title"));
                    ui.strong(tr("Length"));
                    ui.end_row();
                    for &i in &visible {
                        let e = &d.playlist.entries[i];
                        ui.checkbox(&mut d.checked[i], "");
                        ui.weak((i + 1).to_string());
                        let title = if e.title.trim().is_empty() { e.url.as_str() } else { e.title.as_str() };
                        ui.add_sized([400.0, 18.0], egui::Label::new(title).truncate()).on_hover_text(e.url.as_str());
                        ui.weak(e.duration.map(length).unwrap_or_default());
                        ui.end_row();
                    }
                });
            });
            ui.add_space(6.0);
            egui::Grid::new("videos_target").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label(tr("Quality"));
                egui::ComboBox::from_id_salt("videos_quality").selected_text(quality_label(d.quality)).show_ui(
                    ui,
                    |ui| {
                        for q in PlaylistQuality::CHOICES {
                            ui.selectable_value(&mut d.quality, q, quality_label(q));
                        }
                    },
                );
                ui.end_row();
                if settings.list_queue_panel {
                    ui.label(tr("Queue"));
                    ui.horizontal(|ui| {
                        super::tools::queue_combo(ui, "videos_queue", &mut d.queue, queues);
                        ui.checkbox(&mut d.start_queue, tr("Start the queue now"));
                    });
                    ui.end_row();
                }
            });
            if matches!(d.quality, PlaylistQuality::Sound(_)) {
                ui.weak(tr("Saving only the sound needs ffmpeg."));
            }
            let ticked = d.ticked();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let add = egui::Button::new(trf!("Add {count}", count = ticked.len()));
                if ui.add_enabled(connected && !ticked.is_empty(), add).clicked() {
                    let mut items = d.playlist.downloads(&ticked, d.quality, &d.page);
                    for item in &mut items {
                        item.user_agent = d.user_agent.clone();
                        item.queue = Some(d.queue);
                    }
                    actions.push(Action::AddMany {
                        items,
                        queue: d.queue,
                        start_queue: d.start_queue,
                        packaging: Packaging::None,
                    });
                    close = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    close = true;
                }
            });
        });
    open && !close
}

#[cfg(test)]
mod tests {
    use super::*;
    use pullbay_domain::media::PlaylistEntry;

    fn dialog_of(titles: &[&str]) -> VideosDialog {
        let entries = titles
            .iter()
            .enumerate()
            .map(|(i, t)| PlaylistEntry { url: format!("https://v/{i}"), title: t.to_string(), duration: None })
            .collect();
        VideosDialog::new("https://v/list".into(), Playlist { title: "L".into(), entries }, None)
    }

    #[test]
    fn every_video_starts_ticked_and_the_filter_narrows_them() {
        let mut d = dialog_of(&["Intro", "Part two", "Outro"]);
        assert_eq!(d.ticked(), [0, 1, 2]);
        d.filter = "TRO".into();
        assert_eq!(d.ticked(), [0, 2]);
        d.checked[0] = false;
        assert_eq!(d.ticked(), [2]);
    }

    #[test]
    fn lengths_read_as_clocks() {
        assert_eq!(length(61.4), "1:01");
        assert_eq!(length(3725.0), "1:02:05");
    }
}
