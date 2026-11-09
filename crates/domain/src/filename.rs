//! File-name rules: what a saved file may be called, and the numbered
//! names (`report (1).pdf`) used when one is taken.

use std::path::Path;

/// Make `name` safe to use as a file name on every system Pullbay runs on:
/// characters Windows forbids become `_`, and device names get a `_` prefix.
pub fn sanitize_filename(name: &str) -> String {
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
    not_a_device(clean)
}

/// A file name from `url`'s last path segment, without its query, or
/// `download` when it has none.
pub fn name_from_url(url: &str) -> String {
    let no_query = url.split(['?', '#']).next().unwrap_or(url);
    let tail = no_query.rsplit('/').next().unwrap_or("");
    let name = sanitize_filename(tail);
    if name.is_empty() {
        "download".to_string()
    } else {
        name
    }
}

/// The name a download of `url` gets before its server is asked: the one
/// it asked for, made safe, else one from its address.
pub fn proposed_name(url: &str, filename: Option<&str>) -> String {
    filename.map(sanitize_filename).filter(|s| !s.is_empty()).unwrap_or_else(|| name_from_url(url))
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
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("nul.txt"), "_nul.txt");
        assert_eq!(sanitize_filename("Com1.mp4"), "_Com1.mp4");
        assert_eq!(sanitize_filename("LPT9"), "_LPT9");
        assert_eq!(sanitize_filename("console.log"), "console.log");
        assert_eq!(sanitize_filename("COM10.txt"), "COM10.txt");
    }

    #[test]
    fn sanitize_strips_bad_chars() {
        assert_eq!(sanitize_filename("a:b/c*?.txt"), "a_b_c__.txt");
    }

    #[test]
    fn numbered_names() {
        assert_eq!(numbered_name("report.pdf", 0), "report.pdf");
        assert_eq!(numbered_name("report.pdf", 2), "report (2).pdf");
        assert_eq!(numbered_name("archive.tar.gz", 1), "archive.tar (1).gz");
        assert_eq!(numbered_name("README", 1), "README (1)");
        assert_eq!(numbered_name(".bashrc", 1), ".bashrc (1)");
    }
}
