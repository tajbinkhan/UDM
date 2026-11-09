//! File-name rules: what a saved file may be called, and the numbered
//! names (`report (1).pdf`) used when one is taken.
//!
//! `FileName` is a bare file name, not a path: not empty, not `.` or `..`,
//! and without `/`, `\` or NUL. The cleaning rules below make one from
//! any text (a server's suggestion, what the user typed), so a `FileName`
//! is always safe to join to a folder. Names the user or a client hands in
//! stay text until they are cleaned, so a request is never refused for its
//! name; it serializes as the plain string.

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Why text is not a file name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FileNameError {
    #[error("the file name is empty")]
    Empty,
    /// `.` or `..`, which name folders.
    #[error("{0} is not a file name")]
    Dots(String),
    /// A `/`, `\` or NUL, which would make it a path or cut it short.
    #[error("{0} is a path, not a file name")]
    Separator(String),
}

/// A file's name, without its folder.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FileName(String);

impl FileName {
    /// `name` as a file name, as given: refused when empty, `.` or `..`,
    /// or holding `/`, `\` or NUL. Characters only Windows forbids are
    /// kept; [`sanitize_filename`] replaces those.
    pub fn parse(name: &str) -> Result<FileName, FileNameError> {
        FileName::try_from(name.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    /// The name with ` (n)` before its extension, for when it is taken:
    /// itself for 0 (see [`numbered_name`]).
    pub fn numbered(&self, n: u32) -> FileName {
        // A name without separators, numbered, has none either, and is
        // neither empty nor dots: the number is in it.
        FileName(numbered_name(&self.0, n))
    }
}

impl TryFrom<String> for FileName {
    type Error = FileNameError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        if name.is_empty() {
            Err(FileNameError::Empty)
        } else if name == "." || name == ".." {
            Err(FileNameError::Dots(name))
        } else if name.contains(['/', '\\', '\0']) {
            Err(FileNameError::Separator(name))
        } else {
            Ok(FileName(name))
        }
    }
}

impl From<FileName> for String {
    fn from(n: FileName) -> Self {
        n.0
    }
}

impl AsRef<str> for FileName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<Path> for FileName {
    fn as_ref(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl fmt::Display for FileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<str> for FileName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for FileName {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Make `name` safe to use as a file name on every system UIDM runs on:
/// characters Windows forbids become `_`, and device names get a `_` prefix.
/// `None` when nothing is left of it.
pub fn sanitize_filename(name: &str) -> Option<FileName> {
    let clean = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();
    // No separator or NUL is left, nor `.` or `..`; only an empty name fails.
    FileName::parse(&not_a_device(clean)).ok()
}

/// A file name from `url`'s last path segment, without its query, or
/// `download` when it has none.
pub fn name_from_url(url: &str) -> FileName {
    let no_query = url.split(['?', '#']).next().unwrap_or(url);
    let tail = no_query.rsplit('/').next().unwrap_or("");
    sanitize_filename(tail).unwrap_or_else(|| FileName("download".to_string()))
}

/// The name a download of `url` gets before its server is asked: the one
/// it asked for, made safe, else one from its address.
pub fn proposed_name(url: &str, filename: Option<&str>) -> FileName {
    filename.and_then(sanitize_filename).unwrap_or_else(|| name_from_url(url))
}

/// `name`, with `_` in front if Windows would take it for a device such as
/// `CON`, `NUL` or `COM1`, with or without an extension: writing to one of
/// those reaches the device, not a file.
pub fn not_a_device(name: String) -> String {
    const DEVICES: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    let stem = name.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix).is_some_and(|n| {
            matches!(n, "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "\u{b9}" | "\u{b2}" | "\u{b3}")
        })
    };
    if DEVICES.contains(&stem.as_str()) || numbered("COM") || numbered("LPT") {
        format!("_{name}")
    } else {
        name
    }
}

/// `report.pdf` for 0, then `report (1).pdf`, `report (2).pdf`, ...
/// `filename` may be any path's last part; [`FileName::numbered`] numbers
/// a `FileName`.
pub fn numbered_name(filename: &str, n: u32) -> String {
    if n == 0 {
        return filename.to_string();
    }
    let path = Path::new(filename);
    let stem = path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
    match path.extension() {
        Some(ext) => format!("{stem} ({n}).{}", ext.to_string_lossy()),
        None => format!("{stem} ({n})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(name: &str) -> String {
        sanitize_filename(name).map(FileName::into_string).unwrap_or_default()
    }

    #[test]
    fn a_name_comes_from_the_address_when_none_is_asked_for() {
        assert_eq!(name_from_url("https://x.com/a/b/movie.mp4"), "movie.mp4");
        assert_eq!(name_from_url("https://x.com/file.zip?token=1"), "file.zip");
        assert_eq!(name_from_url("https://x.com/"), "download");
        assert_eq!(proposed_name("https://x.com/f.bin", Some("a:b.txt")), "a_b.txt");
        assert_eq!(proposed_name("https://x.com/f.bin", Some("  ")), "f.bin");
    }

    #[test]
    fn device_names_are_not_kept() {
        assert_eq!(clean("CON"), "_CON");
        assert_eq!(clean("nul.txt"), "_nul.txt");
        assert_eq!(clean("Com1.mp4"), "_Com1.mp4");
        assert_eq!(clean("LPT9"), "_LPT9");
        assert_eq!(clean("console.log"), "console.log");
        assert_eq!(clean("COM10.txt"), "COM10.txt");
    }

    #[test]
    fn sanitize_strips_bad_chars() {
        assert_eq!(clean("a:b/c*?.txt"), "a_b_c__.txt");
    }

    #[test]
    fn numbered_names() {
        assert_eq!(numbered_name("report.pdf", 0), "report.pdf");
        assert_eq!(numbered_name("report.pdf", 2), "report (2).pdf");
        assert_eq!(numbered_name("archive.tar.gz", 1), "archive.tar (1).gz");
        assert_eq!(numbered_name("README", 1), "README (1)");
        assert_eq!(numbered_name(".bashrc", 1), ".bashrc (1)");
    }

    #[test]
    fn a_file_name_is_not_empty_dots_or_a_path() {
        for ok in ["a.zip", ".bashrc", "a:b?.txt", "CON", " x ", "..a", "é.mp4"] {
            assert_eq!(FileName::parse(ok).unwrap().as_str(), ok, "kept as given");
        }
        assert_eq!(FileName::parse(""), Err(FileNameError::Empty));
        assert_eq!(FileName::parse(".."), Err(FileNameError::Dots("..".into())));
        assert_eq!(FileName::parse("."), Err(FileNameError::Dots(".".into())));
        for bad in ["a/b", "a\\b", "/", "a\0b"] {
            assert_eq!(FileName::parse(bad), Err(FileNameError::Separator(bad.into())), "{bad:?}");
        }
    }

    #[test]
    fn cleaning_always_gives_a_file_name_or_nothing() {
        for text in ["", "  ", "...", ".", "..", "/", "\\", "a/../b", "\0", "CON", "x\ny", "<>:?*"] {
            if let Some(name) = sanitize_filename(text) {
                assert_eq!(FileName::parse(name.as_str()).as_ref(), Ok(&name), "{text:?}");
            }
        }
        assert_eq!(sanitize_filename(".."), None);
        assert_eq!(sanitize_filename(" . "), None);
        assert_eq!(clean("a/../b"), "a_.._b");
        assert_eq!(proposed_name("https://x/", Some("..")).as_str(), "download");
    }

    #[test]
    fn numbering_a_file_name_keeps_it_one() {
        let n = FileName::parse("report.pdf").unwrap();
        assert_eq!(n.numbered(0), n);
        assert_eq!(n.numbered(3).as_str(), "report (3).pdf");
        assert_eq!(FileName::parse(".bashrc").unwrap().numbered(1).as_str(), ".bashrc (1)");
    }

    #[test]
    fn file_names_are_plain_strings_in_json() {
        let n = FileName::parse("a b.zip").unwrap();
        assert_eq!(serde_json::to_string(&n).unwrap(), r#""a b.zip""#);
        assert_eq!(serde_json::from_str::<FileName>(r#""a b.zip""#).unwrap(), n);
        assert!(serde_json::from_str::<FileName>(r#""../x""#).is_err());
    }
}
