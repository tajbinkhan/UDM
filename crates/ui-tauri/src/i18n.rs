//! The window's text in the user's language.
//!
//! The tables are the egui window's own `crates/ui/src/lang/*.json`, built
//! in from there, so both windows share one copy of every translation
//! (AGENTS.md section 16). As there, the English text is the message ID:
//! the frontend's `t("Pause")` and the shell's [`tr`] look it up in the
//! chosen table and fall back to the English.

use std::collections::HashMap;
use std::sync::OnceLock;

/// One language the window comes in.
pub struct Language {
    /// A BCP 47 tag, as the Language setting stores it.
    pub code: &'static str,
    /// Its name in that language, for the Language list.
    pub name: &'static str,
    json: &'static str,
    /// What separates a number's whole part from its decimals.
    decimal: char,
    /// How a date is written, with `{y}`, `{m}` and `{d}`.
    date: &'static str,
}

macro_rules! table {
    ($file:literal) => {
        include_str!(concat!("../../ui/src/lang/", $file, ".json"))
    };
}

/// The languages, English first. The same list as the egui window's.
pub static LANGUAGES: &[Language] = &[
    Language { code: "en", name: "English", json: "{}", decimal: '.', date: "{y}-{m}-{d}" },
    Language { code: "id", name: "Bahasa Indonesia", json: table!("id"), decimal: ',', date: "{d}/{m}/{y}" },
    Language { code: "de", name: "Deutsch", json: table!("de"), decimal: ',', date: "{d}.{m}.{y}" },
    Language { code: "es", name: "Español", json: table!("es"), decimal: ',', date: "{d}/{m}/{y}" },
    Language { code: "fr", name: "Français", json: table!("fr"), decimal: ',', date: "{d}/{m}/{y}" },
    Language { code: "it", name: "Italiano", json: table!("it"), decimal: ',', date: "{d}/{m}/{y}" },
    Language { code: "nl", name: "Nederlands", json: table!("nl"), decimal: ',', date: "{d}-{m}-{y}" },
    Language { code: "pl", name: "Polski", json: table!("pl"), decimal: ',', date: "{d}.{m}.{y}" },
    Language { code: "pt-BR", name: "Português (Brasil)", json: table!("pt-BR"), decimal: ',', date: "{d}/{m}/{y}" },
    Language { code: "vi", name: "Tiếng Việt", json: table!("vi"), decimal: ',', date: "{d}/{m}/{y}" },
    Language { code: "tr", name: "Türkçe", json: table!("tr"), decimal: ',', date: "{d}.{m}.{y}" },
    Language { code: "ru", name: "Русский", json: table!("ru"), decimal: ',', date: "{d}.{m}.{y}" },
    Language { code: "uk", name: "Українська", json: table!("uk"), decimal: ',', date: "{d}.{m}.{y}" },
    Language { code: "ja", name: "日本語", json: table!("ja"), decimal: '.', date: "{y}/{m}/{d}" },
    Language { code: "ko", name: "한국어", json: table!("ko"), decimal: '.', date: "{y}. {m}. {d}." },
    Language { code: "zh-CN", name: "简体中文", json: table!("zh-CN"), decimal: '.', date: "{y}/{m}/{d}" },
    Language { code: "zh-TW", name: "繁體中文", json: table!("zh-TW"), decimal: '.', date: "{y}/{m}/{d}" },
];

/// A language's table: English text to its translation.
pub type Table = HashMap<String, String>;

fn tables() -> &'static [Table] {
    static TABLES: OnceLock<Vec<Table>> = OnceLock::new();
    TABLES.get_or_init(|| {
        LANGUAGES
            .iter()
            .map(|l| match serde_json::from_str::<Table>(l.json) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(language = l.code, error = %e, "translation table unreadable; English instead");
                    Table::new()
                }
            })
            .collect()
    })
}

impl Language {
    fn index(&self) -> usize {
        LANGUAGES.iter().position(|l| l.code == self.code).unwrap_or(0)
    }

    /// The table the frontend's `t()` reads.
    pub fn table(&self) -> &'static Table {
        &tables()[self.index()]
    }

    /// `english` in this language, or the English when the table lacks it.
    pub fn tr<'a>(&'static self, english: &'a str) -> &'a str {
        self.table().get(english).map_or(english, String::as_str)
    }

    /// `value` with `places` decimals, as the language writes it.
    pub fn number(&self, value: f64, places: usize) -> String {
        let text = format!("{value:.places$}");
        if self.decimal == '.' {
            text
        } else {
            text.replace('.', &self.decimal.to_string())
        }
    }

    /// A date and time as the language writes them.
    pub fn date_time(&self, (y, m, d): (i64, u32, u32), minute: u32) -> String {
        let date = self
            .date
            .replace("{y}", &y.to_string())
            .replace("{m}", &format!("{m:02}"))
            .replace("{d}", &format!("{d:02}"));
        format!("{date} {:02}:{:02}", minute / 60, minute % 60)
    }

    /// Whether the language is written right to left. None of the
    /// languages yet; the frontend sets `dir` from it.
    pub fn right_to_left(&self) -> bool {
        false
    }
}

/// The language the Language setting names, or the best match for it, or
/// English. An empty setting means the system's language, which the shell
/// hands in as `system`.
pub fn resolve(setting: &str, system: &[String]) -> &'static Language {
    let wanted: Vec<&str> =
        if setting.is_empty() { system.iter().map(String::as_str).collect() } else { vec![setting] };
    wanted.iter().find_map(|tag| matching(tag)).unwrap_or(&LANGUAGES[0])
}

fn matching(tag: &str) -> Option<&'static Language> {
    let tag = tag.replace('_', "-");
    let exact = LANGUAGES.iter().find(|l| l.code.eq_ignore_ascii_case(&tag));
    exact.or_else(|| {
        let base = tag.split('-').next().unwrap_or_default();
        match base.to_ascii_lowercase().as_str() {
            "zh" if tag.to_ascii_lowercase().contains("hant") || tag.ends_with("TW") || tag.ends_with("HK") => {
                LANGUAGES.iter().find(|l| l.code == "zh-TW")
            }
            "zh" => LANGUAGES.iter().find(|l| l.code == "zh-CN"),
            "pt" => LANGUAGES.iter().find(|l| l.code == "pt-BR"),
            b => LANGUAGES.iter().find(|l| l.code.eq_ignore_ascii_case(b)),
        }
    })
}

/// Fill `{name}` placeholders in a translated text.
pub fn fill(text: &str, values: &[(&str, &str)]) -> String {
    let mut out = text.to_string();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_reads() {
        for l in LANGUAGES {
            assert!(serde_json::from_str::<Table>(l.json).is_ok(), "{}", l.code);
        }
    }

    #[test]
    fn languages_resolve_by_tag_and_fall_back_to_english() {
        assert_eq!(resolve("de", &[]).code, "de");
        assert_eq!(resolve("", &["fr-FR".into()]).code, "fr");
        assert_eq!(resolve("", &["zh-Hant-TW".into()]).code, "zh-TW");
        assert_eq!(resolve("", &["pt-PT".into()]).code, "pt-BR");
        assert_eq!(resolve("xx", &[]).code, "en");
    }

    #[test]
    fn missing_text_stays_english() {
        let de = resolve("de", &[]);
        assert_eq!(de.tr("Browse..."), "Durchsuchen...");
        assert_eq!(de.tr("no such text"), "no such text");
        assert_eq!(de.number(1.5, 1), "1,5");
        assert_eq!(fill("{a} of {b}", &[("a", "1"), ("b", "2")]), "1 of 2");
    }
}
