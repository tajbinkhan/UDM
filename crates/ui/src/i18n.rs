//! The window's text in the user's language.
//!
//! The tables are `lang/<code>.json`, built into the program: one JSON
//! object from English text to its translation. The English text is the
//! message ID: the frontend's `t('Pause')` and the shell's
//! [`Language::tr`] look it up in the chosen table and fall back to the
//! English. To add a language, add its file and a line to `LANGUAGES`; the
//! tests check that every key is still used and keeps its placeholders, and
//! that every string the shell or the frontend looks up has a translation
//! in each table, apart from those `lang/untranslated.json` lists.

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
        include_str!(concat!("lang/", $file, ".json"))
    };
}

/// The languages, English first.
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

/// [`Language::tr`] for text with `{name}` placeholders, which are filled
/// in afterwards: `trf!(lang, "{n} files", n = count)`.
macro_rules! trf {
    ($lang:expr, $english:literal $(, $name:ident = $value:expr)* $(,)?) => {{
        #[allow(unused_mut)]
        let mut text = $lang.tr($english).to_string();
        $(text = text.replace(concat!("{", stringify!($name), "}"), &$value.to_string());)*
        text
    }};
}
pub(crate) use trf;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Every file under `dir` whose name ends with one of `ends`.
    fn files(dir: &Path, ends: &[&str], out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files(&path, ends, out);
            } else if ends.iter().any(|e| path.to_string_lossy().ends_with(e)) {
                out.push(path);
            }
        }
    }

    /// The shell's sources, without their tests (which look up text that
    /// is not the window's), and the frontend's, without the generated
    /// bindings.
    fn sources() -> (Vec<String>, Vec<String>) {
        let mut rust = Vec::new();
        files(&repo().join("crates/ui/src"), &[".rs"], &mut rust);
        let rust = rust
            .iter()
            .filter(|p| !["i18n.rs", "fixtures.rs", "testing.rs"].iter().any(|n| p.ends_with(n)))
            .map(|p| {
                let text = std::fs::read_to_string(p).unwrap();
                match text.find("#[cfg(test)]\nmod tests") {
                    Some(at) => text[..at].to_string(),
                    None => text,
                }
            })
            .collect();
        let mut web = Vec::new();
        files(&repo().join("frontend/src"), &[".svelte", ".ts"], &mut web);
        let web =
            web.iter().filter(|p| !p.ends_with("bindings.ts")).map(|p| std::fs::read_to_string(p).unwrap()).collect();
        (rust, web)
    }

    /// Sources whose labels the shell passes through `tr`.
    const LABELS: &[&str] = &[
        include_str!("../../domain/src/queue.rs"),
        include_str!("../../domain/src/settings.rs"),
        include_str!("../../infrastructure/src/settings.rs"),
        include_str!("../../domain/src/grab.rs"),
        include_str!("../../infrastructure/src/grab.rs"),
    ];

    /// The `{name}` placeholders in a string, sorted.
    fn placeholders(text: &str) -> Vec<&str> {
        let mut found: Vec<&str> = text
            .match_indices('{')
            .filter_map(|(i, _)| {
                let rest = &text[i + 1..];
                let end = rest.find('}')?;
                let name = &rest[..end];
                (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then_some(name)
            })
            .collect();
        found.sort_unstable();
        found
    }

    /// The text of the string literal that starts after `quote`, with its
    /// escapes undone, Rust's and TypeScript's alike.
    fn literal(rest: &str, quote: char) -> String {
        let mut text = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                c if c == quote => break,
                '\\' => match chars.next() {
                    Some('n') => text.push('\n'),
                    Some('t') => text.push('\t'),
                    Some('u') => {
                        let code: String = chars.by_ref().skip(1).take_while(|c| *c != '}').collect();
                        text.extend(u32::from_str_radix(&code, 16).ok().and_then(char::from_u32));
                    }
                    // A line continuation drops the break and the next line's indent.
                    Some('\r' | '\n') => {
                        while chars.peek().is_some_and(|c| c.is_whitespace()) {
                            chars.next();
                        }
                    }
                    Some(other) => text.push(other),
                    None => break,
                },
                _ => text.push(c),
            }
        }
        text
    }

    /// The English text of every lookup in `source`: `tr("…")` in the
    /// shell, `t('…')` or `t("…")` in the frontend.
    fn looked_up(source: &str, opener: &str, quotes: &[char]) -> Vec<String> {
        let mut found = Vec::new();
        for (at, _) in source.match_indices(opener) {
            // `attr("` or `str("` is not a lookup, nor is `$t(` or `x.t(`
            // in the frontend.
            let before = source[..at].chars().next_back();
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || (opener == "t(" && (c == '$' || c == '.'))) {
                continue;
            }
            // A formatter may put a long string on the line after the call.
            let rest = source[at + opener.len()..].trim_start();
            let Some(quote) = rest.chars().next().filter(|c| quotes.contains(c)) else {
                continue;
            };
            found.push(literal(&rest[1..], quote));
        }
        found
    }

    /// Every string the window looks up.
    fn used() -> Vec<String> {
        let (rust, web) = sources();
        let mut used: Vec<String> = rust.iter().flat_map(|s| looked_up(s, "tr(", &['"'])).collect();
        used.extend(web.iter().flat_map(|s| looked_up(s, "t(", &['\'', '"'])));
        used.sort();
        used.dedup();
        used
    }

    #[test]
    fn every_table_parses_and_keeps_placeholders() {
        for (i, language) in LANGUAGES.iter().enumerate() {
            let parsed: Table =
                serde_json::from_str(language.json).unwrap_or_else(|e| panic!("lang/{}.json: {e}", language.code));
            for (en, text) in &parsed {
                if !text.is_empty() {
                    assert_eq!(placeholders(en), placeholders(text), "{}: {en:?} -> {text:?}", language.code);
                }
            }
            assert_eq!(tables()[i].len(), parsed.len(), "{}", language.code);
        }
    }

    #[test]
    fn every_key_is_a_string_in_the_window() {
        let (rust, web) = sources();
        let mut problems = Vec::new();
        for language in &LANGUAGES[1..] {
            let parsed: Table = serde_json::from_str(language.json).unwrap();
            for en in parsed.keys() {
                let in_rust = format!("{en:?}");
                let in_web = format!("'{}'", en.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "\\n"));
                let found = rust.iter().map(String::as_str).chain(LABELS.iter().copied()).any(|s| s.contains(&in_rust))
                    || web
                        .iter()
                        .any(|s| s.contains(&in_web) || s.contains(&in_rust) || s.contains(&format!("`{en}`")));
                if !found {
                    problems
                        .push(format!("lang/{}.json has {in_rust}, which the window no longer uses", language.code));
                }
            }
        }
        problems.sort();
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    /// Strings that were already untranslated when this check came in
    /// (2026-11-06), or when the frontend's strings joined it. The list
    /// only shrinks: a new string needs its translations, and a translated
    /// one leaves the list.
    const UNTRANSLATED: &str = include_str!("lang/untranslated.json");

    #[test]
    fn every_string_in_the_window_is_translated() {
        let known: Vec<String> = serde_json::from_str(UNTRANSLATED).unwrap();
        let used = used();
        let tables: Vec<(&str, Table)> =
            LANGUAGES[1..].iter().map(|l| (l.code, serde_json::from_str(l.json).unwrap())).collect();
        let mut problems = Vec::new();
        for en in &used {
            for (code, table) in &tables {
                if !table.contains_key(en) && !known.contains(en) {
                    problems.push(format!("lang/{code}.json lacks {en:?}"));
                }
            }
        }
        for en in &known {
            if !used.contains(en) {
                problems.push(format!("lang/untranslated.json lists {en:?}, which the window no longer uses"));
            } else if tables.iter().all(|(_, table)| table.contains_key(en)) {
                problems.push(format!("lang/untranslated.json lists {en:?}, which every language now has"));
            }
        }
        problems.sort();
        problems.dedup();
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn every_language_has_one_line_and_a_valid_code() {
        for (i, language) in LANGUAGES.iter().enumerate() {
            assert!(
                LANGUAGES[..i].iter().all(|l| !l.code.eq_ignore_ascii_case(language.code)),
                "{} is listed twice",
                language.code
            );
            // What Options can save (`Settings::validate`).
            assert!(language.code.len() <= 16 && language.code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
            assert!(std::ptr::eq(resolve(language.code, &[]), language), "{} resolves elsewhere", language.code);
        }
    }

    #[test]
    fn languages_resolve_by_tag_and_fall_back_to_english() {
        assert_eq!(resolve("de", &[]).code, "de");
        assert_eq!(resolve("fr-CA", &[]).code, "fr");
        assert_eq!(resolve("", &["fr-FR".into()]).code, "fr");
        assert_eq!(resolve("pt_PT", &[]).code, "pt-BR");
        assert_eq!(resolve("zh-SG", &[]).code, "zh-CN");
        assert_eq!(resolve("zh-HK", &[]).code, "zh-TW");
        assert_eq!(resolve("", &["zh-Hant-TW".into()]).code, "zh-TW");
        assert_eq!(resolve("ja-JP", &[]).code, "ja");
        assert_eq!(resolve("xx", &[]).code, "en");
    }

    #[test]
    fn missing_text_stays_english() {
        let de = resolve("de", &[]);
        assert_eq!(de.tr("Browse..."), "Durchsuchen...");
        assert_eq!(de.tr("no such text"), "no such text");
        assert_eq!(fill("{a} of {b}", &[("a", "1"), ("b", "2")]), "1 of 2");
    }

    #[test]
    fn numbers_and_dates_follow_the_language() {
        let get = |code: &str| resolve(code, &[]);
        assert_eq!(get("en").number(2.5, 1), "2.5");
        assert_eq!(get("de").number(2.5, 1), "2,5");
        assert_eq!(get("ja").number(1234.567, 2), "1234.57");
        let when = ((2026, 3, 7), 14 * 60 + 5);
        assert_eq!(get("en").date_time(when.0, when.1), "2026-03-07 14:05");
        assert_eq!(get("de").date_time(when.0, when.1), "07.03.2026 14:05");
        assert_eq!(get("ko").date_time(when.0, when.1), "2026. 03. 07. 14:05");
        assert_eq!(get("zh-TW").date_time(when.0, when.1), "2026/03/07 14:05");
    }
}
