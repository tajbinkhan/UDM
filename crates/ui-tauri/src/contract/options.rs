//! The Options window's contract: the settings it edits as plain fields,
//! and what it chooses from. The window edits a copy and sends it back
//! whole with `save_options`; the shell parses and checks it.
//!
//! No password crosses to the webview: the password fields come empty, and
//! an empty one sent back keeps the stored password.

use serde::{Deserialize, Serialize};
use specta::Type;

/// A value and what a list shows for it.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub value: String,
    pub label: String,
}

/// The Options window.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OptionsView {
    pub draft: OptionsDraft,
    pub themes: Vec<Choice>,
    pub languages: Vec<Choice>,
    /// The items UDM can add to the browsers' context menus.
    pub menu_items: Vec<Choice>,
    /// What the Default buttons put back.
    pub default_capture_types: String,
    pub default_excluded_sites: String,
    pub connections: Vec<u32>,
    pub server_connections: Vec<u32>,
}

/// Keys held while clicking a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeysDraft {
    pub on: bool,
    pub alt: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub insert: bool,
    pub delete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoTypeDraft {
    pub ext: String,
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDraft {
    /// The name it had when the window opened; `None` for a new one.
    pub original: Option<String>,
    pub name: String,
    /// File extensions, separated by spaces.
    pub extensions: String,
    pub sites_on: bool,
    pub sites: String,
    pub folder: String,
    pub remember_folder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerLimitDraft {
    pub site: String,
    pub max: u32,
}

/// One proxy: on for its protocol, its server, port and login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProxyDraft {
    pub on: bool,
    pub host: String,
    pub port: String,
    pub user: String,
    /// Empty keeps the stored password.
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LoginDraft {
    pub site: String,
    pub user: String,
    /// Empty keeps the stored password.
    pub password: String,
    /// Had a password when the window opened.
    pub stored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SoundDraft {
    pub on: bool,
    pub file: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProxyChoice {
    None,
    System,
    Script,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DuplicateChoice {
    Ask,
    NewName,
    Overwrite,
    Existing,
}

/// The settings the Options window edits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OptionsDraft {
    // General.
    pub start_with_system: bool,
    pub tray_icon: bool,
    pub close_to_tray: bool,
    pub minimize_to_tray: bool,
    pub keep_awake: bool,
    pub check_updates: bool,
    pub clipboard_monitor: bool,
    pub clipboard_start: bool,
    pub capture_private: bool,
    /// `system`, `light`, `dark`, `highContrast`, or `custom:` and a name.
    pub theme: String,
    pub language: String,
    pub skip_key: KeysDraft,
    pub force_key: KeysDraft,
    pub keys_left_click: bool,
    pub force_skips_pages: bool,
    /// The context menu items shown in Chrome-like browsers and in Firefox.
    pub menu_chromium: Vec<String>,
    pub menu_firefox: Vec<String>,
    pub video_button: bool,
    pub video_mini: bool,
    pub video_types: Vec<VideoTypeDraft>,
    pub video_auto_capture: bool,
    pub video_protected_note: bool,
    // File types.
    pub capture_types: String,
    pub capture_excluded_sites: String,
    pub capture_excluded_urls: Vec<String>,
    pub offer_exceptions: bool,
    // Save to.
    pub download_dir: String,
    pub general_remember_folder: bool,
    pub categories: Vec<CategoryDraft>,
    pub server_file_date: bool,
    pub temp_dir: String,
    // Downloads.
    pub ask_where_to_save: bool,
    pub add_only: bool,
    pub done_dialog: bool,
    pub start_while_asking: bool,
    pub later_panel: bool,
    pub list_queue_panel: bool,
    pub resume_ignores_date: bool,
    pub if_duplicate: DuplicateChoice,
    pub user_agent: String,
    pub progress_on_start: bool,
    pub progress_speed_tab: bool,
    pub progress_done_tab: bool,
    pub progress_hide_buttons: bool,
    pub progress_start_details: bool,
    pub scan_after_download: bool,
    pub scanner_custom: bool,
    pub scanner_command: String,
    // Connection.
    pub connections: u32,
    pub server_limits: Vec<ServerLimitDraft>,
    pub quota_enabled: bool,
    pub quota_mb: u32,
    pub quota_hours: u32,
    pub quota_warn: bool,
    // Proxy / Socks.
    pub proxy_mode: ProxyChoice,
    pub browser_proxy: bool,
    pub pac_url: String,
    /// HTTP, HTTPS, FTP and SOCKS, in that order.
    pub proxies: Vec<ProxyDraft>,
    pub socks4: bool,
    pub passive_ftp: bool,
    // Sites Logins.
    pub logins: Vec<LoginDraft>,
    // Sounds: download complete, failed, queue started, queue done.
    pub sounds: Vec<SoundDraft>,
}

/// What a Browse button picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PickKind {
    Folder,
    Sound,
    Program,
}
