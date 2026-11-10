//! Help: links to the help pages in `docs/`, and the tip of the day. The Help
//! menu itself is in `menubar`.

use eframe::egui;

use crate::i18n::tr;

/// The help pages on GitHub, which are Pullbay's manual.
pub const MANUAL: &str = "https://github.com/tajbinkhan/pullbay/blob/main/docs/README.md";
/// The page on queues and their schedules.
pub const SCHEDULER: &str = "https://github.com/tajbinkhan/pullbay/blob/main/docs/queues-and-scheduler.md";
/// The page on which browser downloads Pullbay takes: the capture rules.
pub const CAPTURE: &str = "https://github.com/tajbinkhan/pullbay/blob/main/docs/browser-capture.md";
/// The page on streams, batches and the site grabber.
pub const GRABBER: &str = "https://github.com/tajbinkhan/pullbay/blob/main/docs/streams-batches-and-grabber.md";
/// The first steps: opening the window, sending downloads from the browser,
/// and the toolbar, menus and options from there.
pub const TUTORIALS: &str = "https://github.com/tajbinkhan/pullbay/blob/main/docs/using-pullbay.md";
/// Every key the window answers.
pub const KEYS: &str = "https://github.com/tajbinkhan/pullbay/blob/main/docs/keyboard-shortcuts.md";
/// Pullbay's home page.
pub const HOME: &str = "https://github.com/tajbinkhan/pullbay";
/// Where to report a problem or ask for help.
pub const SUPPORT: &str = "https://github.com/tajbinkhan/pullbay/issues";

/// The tips, one shown at a time.
pub const TIPS: [&str; 10] = [
    "Hold Alt while clicking a link to keep its download in the browser, or Ctrl to send any link to Pullbay. Options > Browser capture sets the keys.",
    "Right-click a link in the browser and choose Download later with Pullbay to add it to a queue without starting it.",
    "A queue can start on a schedule and put the computer to sleep or shut it down when it is done. Downloads > Scheduler sets one up.",
    "Tasks > Run site grabber finds the files of a type that a site links to, such as every PDF in its manuals.",
    "The video button over a playing video lists the qualities the page loaded. Pick one, and Pullbay downloads it as one file.",
    "Tasks > Add batch download downloads a numbered series, such as photo[001-120].jpg, or a list of addresses you paste.",
    "Ctrl+F finds a download in the list by its name or address.",
    "Options > Connection sets a proxy, and the logins for sites that ask for one.",
    "A category can remember the folder you last chose for it in the save-as dialog: tick its box under Options > Categories.",
    "Options > When done can scan each finished download for viruses, with Microsoft Defender or a scanner you choose.",
];

/// The tip window's state: the tip shown, and the startup box.
pub struct TipWindow {
    pub index: usize,
    pub on_startup: bool,
}

/// Draw the tip window. Returns `Some` with the tip to show next time and
/// the startup choice once it is closed.
pub fn tip_window(ctx: &egui::Context, t: &mut TipWindow) -> Option<(usize, bool)> {
    let mut open = true;
    let mut close = false;
    crate::dialog::Window::new(tr("Tip of the day"))
        .id(egui::Id::new("tip_of_the_day"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.label(egui::RichText::new(tr("Do you know...")).strong());
            ui.add_space(4.0);
            ui.label(tr(TIPS[t.index % TIPS.len()]));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut t.on_startup, tr("Show tips on startup"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("Close")).clicked() {
                        close = true;
                    }
                    if ui.button(tr("Next tip")).clicked() {
                        t.index = (t.index + 1) % TIPS.len();
                    }
                });
            });
        });
    (!open || close).then(|| ((t.index + 1) % TIPS.len(), t.on_startup))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_help_link_names_a_help_page() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for link in [MANUAL, SCHEDULER, GRABBER, TUTORIALS, CAPTURE, KEYS] {
            let page = link.strip_prefix("https://github.com/tajbinkhan/pullbay/blob/main/").unwrap();
            assert!(root.join(page).is_file(), "no help page {page}");
        }
    }

    /// How docs/keyboard-shortcuts.md writes an `egui::Key`.
    fn spelled(key: &str) -> String {
        match key {
            "ArrowUp" => "Up".into(),
            "ArrowDown" => "Down".into(),
            "ArrowLeft" => "Left".into(),
            "ArrowRight" => "Right".into(),
            "PageUp" => "Page Up".into(),
            "PageDown" => "Page Down".into(),
            other => other.into(),
        }
    }

    /// The keys docs/keyboard-shortcuts.md names: the last key of each
    /// `chord` in backquotes.
    fn documented() -> std::collections::BTreeSet<String> {
        let page = include_str!("../../../docs/keyboard-shortcuts.md");
        page.split('`').skip(1).step_by(2).filter_map(|chord| chord.rsplit('+').next()).map(str::to_string).collect()
    }

    #[test]
    fn the_shortcuts_page_lists_every_key_the_window_answers() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let documented = documented();
        let mut missing = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for (at, _) in source.match_indices("Key::") {
                // `egui::Key::F3`, not `ProgressKey::All`.
                if source[..at].ends_with(|c: char| c.is_alphanumeric()) {
                    continue;
                }
                let key: String = source[at + 5..].chars().take_while(|c| c.is_alphanumeric()).collect();
                if !key.is_empty() && !documented.contains(&spelled(&key)) {
                    missing.push(format!("{}: {key}", path.display()));
                }
            }
        }
        assert!(missing.is_empty(), "docs/keyboard-shortcuts.md lacks {missing:?}");
    }

    #[test]
    fn the_shortcuts_page_names_only_real_keys() {
        // Modifiers, and keys egui, a text box or the system answers, which
        // the window's own code does not name.
        const BUILT_IN: [&str; 10] = ["Ctrl", "Tab", "Alt", "Plus", "Minus", "0", "C", "X", "V", "Z"];
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let sources: String = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            .map(|p| std::fs::read_to_string(p).unwrap())
            .collect();
        for key in documented() {
            let named = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "PageUp", "PageDown"]
                .into_iter()
                .chain(std::iter::once(key.as_str()))
                .any(|k| spelled(k) == key && sources.contains(&format!("Key::{k}")));
            assert!(named || BUILT_IN.contains(&key.as_str()), "docs/keyboard-shortcuts.md names {key:?}");
        }
    }
}
