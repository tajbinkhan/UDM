//! The contract of what the window does with a download's file: the zip
//! preview, converting a video, checking a checksum, and the offer to keep
//! a download the user canceled twice in the browser.

use serde::{Deserialize, Serialize};
use specta::Type;

/// One file inside a zip.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ZipRow {
    pub name: String,
    pub size: String,
}

/// The files inside a zip download, read from the end of the archive.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ZipView {
    /// "Inside" and the download's name.
    pub title: String,
    /// How many files and their size unpacked.
    pub summary: String,
    /// That only the first are shown.
    pub note: String,
    pub rows: Vec<ZipRow>,
    pub error: Option<String>,
    pub reading: bool,
}

/// After a captured download's save-as dialog is canceled twice.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionView {
    pub url: String,
    /// The site, for "Don't capture" it; empty when the address has none.
    pub host: String,
    pub connected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExceptionChoice {
    /// Don't capture the site.
    Site,
    /// Don't capture this address.
    Address,
    Keep,
}

/// What Convert makes from a finished video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ConvertTarget {
    Mp4,
    Mp3,
    M4a,
    Opus,
    Flac,
}

/// What Verify checksum found.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    /// The algorithm the checksum is, such as SHA-256.
    pub algorithm: String,
    pub matches: bool,
    /// The checksum given and the file's, in lowercase hex.
    pub expected: String,
    pub actual: String,
}
