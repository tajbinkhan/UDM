use udm_domain::theme::{CustomTheme, SystemContrast};

use crate::window::{SystemServices, ThemeFileError, ThemeFiles};

/// Read the theme files and the system's contrast colors, as the window
/// does every few seconds on a worker so a new or changed file shows and
/// the system's contrast setting is followed.
pub struct ReadThemes<'a> {
    files: &'a dyn ThemeFiles,
    system: &'a dyn SystemServices,
}

/// What [`ReadThemes`] found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemesRead {
    /// The files' themes by name, each name once.
    pub themes: Vec<CustomTheme>,
    /// The files that give no theme, by file name, and why.
    pub broken: Vec<BrokenTheme>,
    /// The system's contrast colors, while its high-contrast setting is on.
    pub contrast: Option<SystemContrast>,
}

/// A theme file that gives no theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenTheme {
    pub file: String,
    pub why: ThemeFileError,
}

impl<'a> ReadThemes<'a> {
    pub fn new(files: &'a dyn ThemeFiles, system: &'a dyn SystemServices) -> Self {
        ReadThemes { files, system }
    }

    /// The themes sorted by name, case aside. Of two files with one name
    /// the first by file name gives the theme and the other is broken, so
    /// the setting that names it always means the same file.
    pub fn run(&self) -> ThemesRead {
        let mut files = self.files.read();
        files.sort_by(|a, b| a.file.to_lowercase().cmp(&b.file.to_lowercase()).then_with(|| a.file.cmp(&b.file)));
        let mut read = ThemesRead { contrast: self.system.contrast(), ..ThemesRead::default() };
        for file in files {
            match file.theme {
                Ok(theme) if read.themes.iter().any(|t| t.name.same(&theme.name)) => read
                    .broken
                    .push(BrokenTheme { file: file.file, why: ThemeFileError::NameTaken(theme.name.to_string()) }),
                Ok(theme) => read.themes.push(theme),
                Err(why) => read.broken.push(BrokenTheme { file: file.file, why }),
            }
        }
        read.themes.sort_by_key(|t| t.name.as_str().to_lowercase());
        read
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Contrast, Shelf};
    use crate::window::ThemeFile;
    use udm_domain::theme::{ThemeName, DARK, HIGH_CONTRAST, LIGHT};

    fn theme(name: &str) -> CustomTheme {
        CustomTheme { name: ThemeName::parse(name).unwrap(), look: DARK }
    }

    fn file(file: &str, theme: Result<CustomTheme, ThemeFileError>) -> ThemeFile {
        ThemeFile { file: file.into(), theme }
    }

    #[test]
    fn themes_come_sorted_by_name_with_the_broken_files_apart() {
        let shelf = Shelf(vec![
            file("b.theme", Ok(theme("zebra"))),
            file("a.theme", Ok(theme("Apple"))),
            file("c.theme", Err(ThemeFileError::NotASetting { line: 3 })),
        ]);
        let read = ReadThemes::new(&shelf, &Contrast(None)).run();
        let names: Vec<&str> = read.themes.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Apple", "zebra"]);
        assert_eq!(read.broken, [BrokenTheme { file: "c.theme".into(), why: ThemeFileError::NotASetting { line: 3 } }]);
        assert_eq!(read.contrast, None);
    }

    #[test]
    fn the_first_file_by_name_keeps_a_theme_name() {
        let mut second = theme("OCEAN");
        second.look = LIGHT;
        let shelf = Shelf(vec![file("ocean_2.theme", Ok(second)), file("Ocean.theme", Ok(theme("Ocean")))]);
        let read = ReadThemes::new(&shelf, &Contrast(None)).run();
        assert_eq!(read.themes, [theme("Ocean")]);
        assert_eq!(
            read.broken,
            [BrokenTheme { file: "ocean_2.theme".into(), why: ThemeFileError::NameTaken("OCEAN".into()) }]
        );
    }

    #[test]
    fn the_system_contrast_colors_come_along() {
        let p = HIGH_CONTRAST.palette;
        let colors = SystemContrast {
            window: p.background,
            text: p.foreground,
            highlight: p.selected,
            highlight_text: p.selected_text,
            link: p.link,
            gray_text: p.foreground,
            button: p.background,
            button_text: p.foreground,
        };
        let read = ReadThemes::new(&Shelf(Vec::new()), &Contrast(Some(colors))).run();
        assert_eq!(read, ThemesRead { contrast: Some(colors), ..ThemesRead::default() });
    }
}
