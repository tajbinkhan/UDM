//! The site grabber's contract: the wizard's fields, the crawl's progress
//! and results, and the projects, templates and filters it works with.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{FileIcon, Id, QueueItem, Then, TreeNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SaveChoice {
    /// Each file to its own category's folder.
    Categories,
    /// Every file to one category's folder.
    Category,
    /// Every file to one folder.
    Folder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SizeUnit {
    Kb,
    Mb,
}

/// The wizard's fields, as typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrabDraft {
    // Step 1.
    pub start: String,
    pub login_on: bool,
    pub login_user: String,
    /// A new password; empty keeps the stored one.
    pub login_password: String,
    pub login_page_on: bool,
    pub login_page: String,
    pub logout_on: bool,
    // Step 2.
    pub save: SaveChoice,
    pub save_category: String,
    pub save_folder: String,
    pub subfolders: bool,
    /// Convert the links of saved pages for offline browsing.
    pub offline: bool,
    pub overwrite: bool,
    // Step 3.
    pub whole_site: bool,
    pub depth: u32,
    pub other_depth: u32,
    pub ignore_popups: bool,
    pub no_parent: bool,
    pub subdomains: bool,
    pub render_scripts: bool,
    pub explore_only_on: bool,
    pub explore_only: String,
    /// Also the logout pages Step 1 leaves out.
    pub explore_skip_on: bool,
    pub explore_skip: String,
    // Step 4.
    pub types: String,
    pub skip_types: String,
    pub files_this_site: bool,
    pub hide_duplicates: bool,
    pub download_found: bool,
    pub min_on: bool,
    pub min_size: u32,
    pub min_unit: SizeUnit,
    pub max_on: bool,
    pub max_size: u32,
    pub max_unit: SizeUnit,
    pub files_only_on: bool,
    pub files_only: String,
    pub files_skip_on: bool,
    pub files_skip: String,
    // Options > Settings.
    pub explore_at_once: u32,
    pub download_at_once: u32,
    pub link_text: bool,
    pub max_pages: u32,
    pub max_mb: u32,
    pub delay_secs: u32,
}

/// A file-type filter: its name, its mask, and the types it puts in the
/// list of file types.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FilterChoice {
    pub name: String,
    pub mask: String,
    pub types: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FilterInput {
    pub name: String,
    pub mask: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GrabPhase {
    /// The wizard.
    Setup,
    /// Exploring the site.
    Running,
    /// The results.
    Done,
}

/// The crawl's counts, as the statistics show them.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrabStats {
    pub status: String,
    pub pages: String,
    pub explored: String,
    pub matched: String,
    pub added: String,
    /// Why it stopped early, if it did.
    pub stopped: Option<String>,
}

/// A file the crawl found.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrabRow {
    /// Its place in the results, for ticking it.
    pub index: u32,
    pub name: String,
    pub icon: FileIcon,
    pub kind: String,
    pub size: String,
    pub link_text: String,
    pub url: String,
    pub page: String,
}

/// The grabber window.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrabberView {
    /// What a new project starts from.
    pub base: GrabDraft,
    /// The projects folder is there, so projects and templates can be kept.
    pub library: bool,
    pub templates: Vec<String>,
    pub filters: Vec<FilterChoice>,
    pub recent: Vec<String>,
    pub categories: Vec<String>,
    pub queues: Vec<QueueItem>,
    pub phase: GrabPhase,
    pub stats: GrabStats,
    /// Grows when new results replace the old, so the window ticks them
    /// afresh from `checked`.
    pub results: u32,
    pub checked: Vec<bool>,
    pub tree: Vec<TreeNode>,
    pub rows: Vec<GrabRow>,
    /// The project's name, empty for a new one.
    pub project: String,
    /// Results the project has not saved.
    pub unsaved: bool,
    pub add_on_close: bool,
    /// The project's schedule.
    pub schedule: GrabScheduleDraft,
    /// The power plan lets nothing wake the computer.
    pub wake_warning: Option<super::WakeWarning>,
    pub connected: bool,
}

/// An opened project: its fields and name.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrabOpened {
    pub name: String,
    pub draft: GrabDraft,
}

/// What a scheduled run of a project does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GrabRun {
    /// Explore the site and keep what it finds.
    Explore,
    /// Explore it and download the files earlier runs did not find.
    ExploreAndDownload,
    /// Download the files ticked in the project, without exploring.
    DownloadChecked,
}

/// A project's schedule, as Options > Schedule edits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrabScheduleDraft {
    pub on: bool,
    /// `HH:MM`.
    pub start: String,
    pub once: bool,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// 0 is Monday, 6 Sunday.
    pub days: Vec<u8>,
    pub stop_on: bool,
    pub stop: String,
    pub repeat_on: bool,
    pub repeat_hours: u32,
    pub repeat_minutes: u32,
    pub wake: bool,
    pub run: GrabRun,
    /// Download again what earlier runs downloaded, as a synchronization
    /// queue does.
    pub sync: bool,
    pub queue: Id,
    pub then: Then,
    pub force_close: bool,
    pub open_on: bool,
    pub open_file: String,
}
