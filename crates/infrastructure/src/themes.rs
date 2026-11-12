//! `ThemeDir`, the `ThemeFiles` adapter: the `.theme` files in the themes
//! folder beside the settings file (`paths::themes_dir`).
//!
//! A theme file is UTF-8 text, one `key = value` setting per line. Blank
//! lines and lines starting with `#` or `;` are skipped, and so is a
//! `[section]` line, so an INI editor's habits do no harm. What the keys
//! mean is `udm_domain::theme::ThemeDraft`'s. The file's name, without
//! `.theme`, names the theme unless a `name =` line does. docs/themes.md
//! describes the format for users.

use std::fs;
use std::path::{Path, PathBuf};

use udm_application::window::{ThemeFile, ThemeFileError, ThemeFiles};
use udm_domain::ports::FilesError;
use udm_domain::theme::{ThemeDraft, ThemeName};

/// The extension of a theme file.
pub const EXTENSION: &str = "theme";

/// The largest theme file read; a theme takes a few hundred bytes.
const MAX_BYTES: u64 = 64 * 1024;

/// The most theme files read from the folder.
const MAX_FILES: usize = 200;

/// The themes folder, or none when the settings folder cannot be found.
pub struct ThemeDir(Option<PathBuf>);

impl ThemeDir {
    /// The themes folder of this copy's settings.
    pub fn new() -> ThemeDir {
        ThemeDir(crate::paths::themes_dir().ok())
    }

    /// The theme files in `dir`.
    pub fn at(dir: Option<PathBuf>) -> ThemeDir {
        ThemeDir(dir)
    }
}

impl Default for ThemeDir {
    fn default() -> Self {
        ThemeDir::new()
    }
}

impl ThemeFiles for ThemeDir {
    fn open_folder(&self) -> Result<PathBuf, FilesError> {
        let dir = self.0.clone().ok_or_else(|| FilesError("the settings folder cannot be found".into()))?;
        fs::create_dir_all(&dir).map_err(|e| FilesError(e.to_string()))?;
        Ok(dir)
    }

    fn read(&self) -> Vec<ThemeFile> {
        let Some(entries) = self.0.as_deref().and_then(|d| fs::read_dir(d).ok()) else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(EXTENSION)) && p.is_file())
            .collect();
        paths.sort();
        paths.truncate(MAX_FILES);
        paths.iter().map(|p| read_file(p)).collect()
    }
}

/// The theme in the file at `path`, or why it gives none.
fn read_file(path: &Path) -> ThemeFile {
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let text = match fs::metadata(path) {
        Ok(m) if m.len() > MAX_BYTES => None,
        Ok(_) => fs::read(path).ok().and_then(|b| String::from_utf8(b).ok()),
        Err(_) => None,
    };
    let theme = match text {
        Some(text) => parse(&stem, &text),
        None => {
            tracing::warn!(file = %path.display(), "theme file unreadable, too large or not UTF-8");
            Err(ThemeFileError::Unreadable)
        }
    };
    ThemeFile { file, theme }
}

/// The theme a file named `stem` holding `text` gives.
pub fn parse(stem: &str, text: &str) -> Result<udm_domain::theme::CustomTheme, ThemeFileError> {
    let mut draft = ThemeDraft::new(ThemeName::parse(stem).ok());
    for (i, line) in text.lines().enumerate() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(['#', ';']) || (line.starts_with('[') && line.ends_with(']')) {
            continue;
        }
        let line_no = i + 1;
        let (key, value) = line.split_once('=').ok_or(ThemeFileError::NotASetting { line: line_no })?;
        // A comment after the value, as in `link = #00ffff  # cyan`.
        // Not in a name, which may hold `#`.
        let value = value.trim_start();
        let value = match value.find(" #").or_else(|| value.find(" ;")) {
            Some(at) if !key.trim().eq_ignore_ascii_case("name") => &value[..at],
            _ => value,
        };
        draft.set(key, value).map_err(|error| ThemeFileError::Invalid { line: line_no, error })?;
    }
    draft.finish().ok_or(ThemeFileError::NoName)
}

#[cfg(test)]
mod tests {
    use super::*;
    use udm_domain::theme::{Color, InvalidTheme, Sizes, DARK, HIGH_CONTRAST};

    #[test]
    fn a_file_sets_what_it_changes_on_its_base() {
        let text = "\u{feff}# Ocean, after a sea at night\n\n[colors]\nbase = dark\nbackground = #0b1e2d\n\
                    link = #7fdbff   # light blue\nborder-width = 2\n; done\n";
        let theme = parse("Ocean", text).unwrap();
        assert_eq!(theme.name.as_str(), "Ocean");
        assert_eq!(theme.look.palette.background, Color::rgb(0x0b, 0x1e, 0x2d));
        assert_eq!(theme.look.palette.link, Color::rgb(0x7f, 0xdb, 0xff));
        assert_eq!(theme.look.palette.foreground, DARK.palette.foreground);
        assert_eq!(theme.look.sizes, Sizes { border_width: 2, ..Sizes::NORMAL });
    }

    #[test]
    fn a_name_line_names_the_theme() {
        let theme = parse("x", "name = Big # and bold\nbase = high_contrast\ntext_size = 16").unwrap();
        assert_eq!(theme.name.as_str(), "Big # and bold");
        assert_eq!(theme.look.sizes.text, 16);
        assert_eq!(theme.look.palette.background, HIGH_CONTRAST.palette.background);
    }

    #[test]
    fn a_broken_line_says_where() {
        assert_eq!(parse("a", "base = dark\nnot a setting"), Err(ThemeFileError::NotASetting { line: 2 }));
        assert_eq!(
            parse("a", "\n\nlink = blue"),
            Err(ThemeFileError::Invalid { line: 3, error: InvalidTheme::BadColor("blue".into()) })
        );
        assert_eq!(parse(" ", "base = dark"), Err(ThemeFileError::NoName));
    }

    #[test]
    fn the_folder_gives_its_theme_files() {
        let dir = std::env::temp_dir().join(format!("udm_themes_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let themes = ThemeDir::at(Some(dir.clone()));
        assert_eq!(themes.read(), [], "no folder, no themes");
        assert_eq!(themes.open_folder().unwrap(), dir);
        fs::write(dir.join("Ocean.theme"), "base = dark\n").unwrap();
        fs::write(dir.join("Broken.THEME"), "link = blue\n").unwrap();
        fs::write(dir.join("notes.txt"), "base = dark\n").unwrap();
        fs::write(dir.join("Big.theme"), vec![b'#'; MAX_BYTES as usize + 1]).unwrap();
        fs::create_dir(dir.join("folder.theme")).unwrap();
        let read = themes.read();
        let files: Vec<&str> = read.iter().map(|f| f.file.as_str()).collect();
        assert_eq!(files, ["Big.theme", "Broken.THEME", "Ocean.theme"]);
        assert_eq!(read[0].theme, Err(ThemeFileError::Unreadable));
        assert!(matches!(read[1].theme, Err(ThemeFileError::Invalid { line: 1, .. })));
        assert_eq!(read[2].theme.as_ref().map(|t| t.look), Ok(DARK));
        let _ = fs::remove_dir_all(&dir);
    }
}
