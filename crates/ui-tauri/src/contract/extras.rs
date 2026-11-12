//! The contract of the windows the daemon asks for: the login prompt, the
//! countdown before sleeping or shutting down, a new link for a download,
//! a package's password, and the browser's links, videos and gallery to
//! pick from.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{FileIcon, Id, QueueItem};

/// The login prompt: a download's site wants a user name and password.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LoginView {
    pub host: String,
    pub realm: String,
    /// The last login was refused.
    pub refused: bool,
    /// Every waiting download is on the site that asked, so the login can
    /// be theirs alone; otherwise it is saved as a site login.
    pub own_site: bool,
    pub user: String,
    /// How many downloads wait for this login.
    pub count: u32,
}

/// The countdown before UDM exits, or the computer sleeps, hibernates or
/// shuts down.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CountdownView {
    /// Download finished, or Queue finished.
    pub title: String,
    /// What finished.
    pub done: String,
    /// What happens, and in how many seconds.
    pub message: String,
    /// The button that does it now.
    pub now: String,
    pub seconds: u32,
}

/// A new address for a download whose link expired.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RefreshView {
    pub id: Id,
    pub name: String,
    pub address: String,
    /// The daemon waits for the next download the browser starts.
    pub waiting: bool,
    /// The new link is for another file: the link and why.
    pub mismatch: Option<RefreshMismatchView>,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RefreshMismatchView {
    pub url: String,
    pub reason: String,
}

/// A package's archives want a password.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PasswordView {
    pub id: Id,
    pub name: String,
    /// The last password given did not open them.
    pub wrong: bool,
}

/// One link of a page, to pick.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PickRow {
    pub index: u32,
    pub name: String,
    pub icon: FileIcon,
    pub kind: String,
    pub size: String,
    pub url: String,
    pub text: String,
}

/// The browser's "Download all links": a page's links, probed for their
/// sizes as the window shows them.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PickLinksView {
    pub page: String,
    pub rows: Vec<PickRow>,
    /// The file types among the links, for ticking all of one type.
    pub types: Vec<String>,
    pub suggested_package: String,
    pub queues: Vec<QueueItem>,
    pub connected: bool,
}

/// One video of a playlist.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoRow {
    pub index: u32,
    pub title: String,
    pub duration: String,
    pub url: String,
}

/// A playlist's or channel's videos, to pick, with the quality to get.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PickVideosView {
    pub title: String,
    pub rows: Vec<VideoRow>,
    /// The qualities offered, best first.
    pub qualities: Vec<String>,
    pub queues: Vec<QueueItem>,
    pub connected: bool,
}

/// One picture of a gallery.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PictureRow {
    pub index: u32,
    pub name: String,
    pub size: String,
    pub url: String,
}

/// A gallery page's pictures, read with gallery-dl.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GalleryView {
    pub page: String,
    /// Reading the page.
    pub reading: bool,
    pub title: String,
    pub rows: Vec<PictureRow>,
    /// Why there are no pictures.
    pub error: Option<String>,
    /// The folder the pictures go in unless another is chosen.
    pub folder: String,
    pub queues: Vec<QueueItem>,
    pub connected: bool,
}

/// What the pickers' Add sends: the ticked rows, where, and whether to
/// start the queue.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PickInput {
    pub indices: Vec<u32>,
    pub queue: Id,
    pub start: bool,
    /// Keep the links together as a package of this name.
    pub package: Option<String>,
    /// The videos' quality, by its place in `qualities`.
    pub quality: u32,
    /// The gallery's folder.
    pub folder: String,
}
