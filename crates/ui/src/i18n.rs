//! The window's text in the user's language.
//!
//! Every string the window shows is written in the code in English, and that
//! English text is also its message ID: `tr("Save")` looks it up in the
//! chosen language's table and gives back the English when the table lacks
//! it. `trf!` does the same for text with `{name}` placeholders, filling them
//! in after the lookup so a translation can move them.
//!
//! The tables are `lang/<code>.json`, built into the program: one JSON object
//! from English text to its translation. To add a language, add its file and
//! a line to `LANGUAGES`; the tests check that every key is still used and
//! keeps its placeholders, and that every `tr` and `trf!` string has a
//! translation in each table, apart from those `lang/untranslated.json` lists.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use pullbay_application::window::ScriptFont;
use pullbay_domain::sync::RwLockExt;

pub struct Language {
    /// A BCP 47 tag: the ISO 639-1 code, as Windows names its display
    /// languages, with a region where the written language differs by it
    /// (`pt-BR`, `zh-CN`, `zh-TW`).
    pub code: &'static str,
    /// The language's name in that language, for the Options menu.
    pub name: &'static str,
    /// Its name in English, shown instead while the window lacks a font
    /// for the name's script.
    english: &'static str,
    json: &'static str,
    /// The system font the language's script needs beyond the window's
    /// text font, read on a worker when the language is chosen.
    pub font: Option<ScriptFont>,
    /// What separates a number's whole part from its decimals.
    decimal: char,
    /// How a date is written, with `{y}`, `{m}` and `{d}` (zero-padded).
    date: &'static str,
}

/// One language's line in `LANGUAGES`.
const fn language(
    (code, name, english): (&'static str, &'static str, &'static str),
    json: &'static str,
    font: Option<ScriptFont>,
    decimal: char,
    date: &'static str,
) -> Language {
    Language { code, name, english, json, font, decimal, date }
}

/// The languages the window comes in, English first and then by name.
/// English is the code's own text. The German, Spanish and French tables
/// came first; the others were made with a language model from the English
/// and wait for native speakers to review them (`docs/translating.md`).
pub static LANGUAGES: &[Language] = &[
    language(("en", "English", "English"), "{}", None, '.', "{y}-{m}-{d}"),
    language(("id", "Bahasa Indonesia", "Indonesian"), include_str!("lang/id.json"), None, ',', "{d}/{m}/{y}"),
    language(("de", "Deutsch", "German"), include_str!("lang/de.json"), None, ',', "{d}.{m}.{y}"),
    language(("es", "Español", "Spanish"), include_str!("lang/es.json"), None, ',', "{d}/{m}/{y}"),
    language(("fr", "Français", "French"), include_str!("lang/fr.json"), None, ',', "{d}/{m}/{y}"),
    language(("it", "Italiano", "Italian"), include_str!("lang/it.json"), None, ',', "{d}/{m}/{y}"),
    language(("nl", "Nederlands", "Dutch"), include_str!("lang/nl.json"), None, ',', "{d}-{m}-{y}"),
    language(("pl", "Polski", "Polish"), include_str!("lang/pl.json"), None, ',', "{d}.{m}.{y}"),
    language(
        ("pt-BR", "Português (Brasil)", "Portuguese (Brazil)"),
        include_str!("lang/pt-BR.json"),
        None,
        ',',
        "{d}/{m}/{y}",
    ),
    language(
        ("vi", "Tiếng Việt", "Vietnamese"),
        include_str!("lang/vi.json"),
        Some(ScriptFont::Vietnamese),
        ',',
        "{d}/{m}/{y}",
    ),
    language(("tr", "Türkçe", "Turkish"), include_str!("lang/tr.json"), None, ',', "{d}.{m}.{y}"),
    language(("ru", "Русский", "Russian"), include_str!("lang/ru.json"), None, ',', "{d}.{m}.{y}"),
    language(("uk", "Українська", "Ukrainian"), include_str!("lang/uk.json"), None, ',', "{d}.{m}.{y}"),
    language(
        ("ja", "日本語", "Japanese"),
        include_str!("lang/ja.json"),
        Some(ScriptFont::Japanese),
        '.',
        "{y}/{m}/{d}",
    ),
    language(("ko", "한국어", "Korean"), include_str!("lang/ko.json"), Some(ScriptFont::Korean), '.', "{y}. {m}. {d}."),
    language(
        ("zh-CN", "简体中文", "Chinese (Simplified)"),
        include_str!("lang/zh-CN.json"),
        Some(ScriptFont::SimplifiedChinese),
        '.',
        "{y}/{m}/{d}",
    ),
    language(
        ("zh-TW", "繁體中文", "Chinese (Traditional)"),
        include_str!("lang/zh-TW.json"),
        Some(ScriptFont::TraditionalChinese),
        '.',
        "{y}/{m}/{d}",
    ),
];

impl Language {
    /// The language's own name, or its English one while the window has no
    /// font for the own name's script (Chinese, Japanese, Korean and
    /// Vietnamese before their system font is read).
    pub fn shown_name(&self) -> &'static str {
        match self.font {
            Some(script) if !INSTALLED.read_or_recover().contains(&script) => self.english,
            _ => self.name,
        }
    }

    /// `value` with `places` decimals, as the language writes it.
    pub fn number(&self, value: f64, places: usize) -> String {
        let text = format!("{value:.places$}");
        if self.decimal == '.' {
            text
        } else {
            text.replacen('.', &self.decimal.to_string(), 1)
        }
    }

    /// A date and a time of day, as the language writes them.
    pub fn date_time(&self, (y, m, d): (i64, u32, u32), minute: u32) -> String {
        let date = self
            .date
            .replace("{y}", &format!("{y:04}"))
            .replace("{m}", &format!("{m:02}"))
            .replace("{d}", &format!("{d:02}"));
        format!("{date} {:02}:{:02}", minute / 60, minute % 60)
    }
}

type Table = HashMap<&'static str, &'static str>;

/// The current language's place in `LANGUAGES`, and its table (`None`
/// while it is English).
static CURRENT: RwLock<(usize, Option<&'static Table>)> = RwLock::new((0, None));

/// Each language's table, parsed on first use. The strings live as long as
/// the program, so `tr` can hand out `&'static str` like the literals it
/// replaces, and switching back and forth parses nothing twice.
fn table(index: usize) -> &'static Table {
    static TABLES: OnceLock<Vec<OnceLock<Table>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| LANGUAGES.iter().map(|_| OnceLock::new()).collect());
    tables[index].get_or_init(|| {
        let parsed: HashMap<String, String> = serde_json::from_str(LANGUAGES[index].json).unwrap_or_default();
        parsed
            .into_iter()
            .filter(|(_, text)| !text.is_empty())
            .map(|(en, text)| (&*Box::leak(en.into_boxed_str()), &*Box::leak(text.into_boxed_str())))
            .collect()
    })
}

/// The language Options names, or for an empty setting the first Windows
/// display language Pullbay has, else English.
pub fn resolve(setting: &str) -> &'static Language {
    let wanted = if setting.is_empty() { crate::win::display_languages() } else { vec![setting.to_string()] };
    wanted.iter().find_map(|tag| matching(tag)).unwrap_or(&LANGUAGES[0])
}

/// The language a BCP 47 tag (`de-AT`, `zh-Hant-HK`, `pt_PT`) asks for:
/// the same tag, then for Chinese the script the region writes, then the
/// first language with the tag's own code.
fn matching(tag: &str) -> Option<&'static Language> {
    let tag = tag.replace('_', "-");
    let by_code = |code: &str| LANGUAGES.iter().find(|l| l.code.eq_ignore_ascii_case(code));
    if let Some(exact) = by_code(&tag) {
        return Some(exact);
    }
    let mut parts = tag.split('-');
    let primary = parts.next().unwrap_or("");
    if primary.eq_ignore_ascii_case("zh") {
        let traditional = parts.any(|p| ["hant", "tw", "hk", "mo"].iter().any(|t| p.eq_ignore_ascii_case(t)));
        return by_code(if traditional { "zh-TW" } else { "zh-CN" });
    }
    LANGUAGES.iter().find(|l| l.code.split('-').next().is_some_and(|c| c.eq_ignore_ascii_case(primary)))
}

/// The scripts whose system font the window has installed.
static INSTALLED: RwLock<Vec<ScriptFont>> = RwLock::new(Vec::new());

/// Note that the window can now draw `script`.
pub fn font_installed(script: ScriptFont) {
    let mut installed = INSTALLED.write_or_recover();
    if !installed.contains(&script) {
        installed.push(script);
    }
}

/// Switch the window's text to the language a setting names (see `resolve`).
pub fn set_language(setting: &str) {
    let language = resolve(setting);
    let index = LANGUAGES.iter().position(|l| std::ptr::eq(l, language)).unwrap_or(0);
    *CURRENT.write_or_recover() = (index, (index != 0).then(|| table(index)));
}

/// The language the window's text is in.
pub fn current() -> &'static Language {
    &LANGUAGES[CURRENT.read_or_recover().0]
}

/// `value` with `places` decimals in the current language: "2,5" in German.
pub fn number(value: f64, places: usize) -> String {
    current().number(value, places)
}

/// `english` in the current language.
pub fn tr(english: &'static str) -> &'static str {
    match CURRENT.read_or_recover().1 {
        Some(table) => table.get(english).copied().unwrap_or(english),
        None => english,
    }
}

/// `tr` for text with `{name}` placeholders, which are filled in afterwards:
/// `trf!("{n} files", n = count)`.
macro_rules! trf {
    ($english:literal $(, $name:ident = $value:expr)* $(,)?) => {{
        #[allow(unused_mut)]
        let mut text = $crate::i18n::tr($english).to_string();
        $(text = text.replace(concat!("{", stringify!($name), "}"), &$value.to_string());)*
        text
    }};
}
pub(crate) use trf;

#[cfg(test)]
mod tests {
    use super::*;

    /// The sources the window's strings come from.
    const SOURCES: &[&str] = &[
        include_str!("lib.rs"),
        include_str!("addurl.rs"),
        include_str!("answers.rs"),
        include_str!("banner.rs"),
        include_str!("categories.rs"),
        include_str!("columns.rs"),
        include_str!("connections.rs"),
        include_str!("droptarget.rs"),
        include_str!("errors.rs"),
        include_str!("find.rs"),
        include_str!("help.rs"),
        include_str!("links.rs"),
        include_str!("packages.rs"),
        include_str!("videos.rs"),
        include_str!("menubar.rs"),
        include_str!("login.rs"),
        include_str!("progress.rs"),
        include_str!("properties.rs"),
        include_str!("queues.rs"),
        include_str!("remote.rs"),
        include_str!("state.rs"),
        include_str!("table.rs"),
        include_str!("toolbar.rs"),
        include_str!("tools.rs"),
        include_str!("tray.rs"),
        include_str!("verify.rs"),
        // Labels the window passes through `tr`.
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

    /// `text` as it appears in the source, inside a string literal.
    fn escaped(text: &str) -> String {
        format!("{text:?}")
    }

    #[test]
    fn every_table_parses_and_keeps_placeholders() {
        for (i, language) in LANGUAGES.iter().enumerate() {
            let parsed: HashMap<String, String> =
                serde_json::from_str(language.json).unwrap_or_else(|e| panic!("lang/{}.json: {e}", language.code));
            for (en, text) in &parsed {
                if !text.is_empty() {
                    assert_eq!(placeholders(en), placeholders(text), "{}: {en:?} -> {text:?}", language.code);
                }
            }
            assert_eq!(table(i).len(), parsed.values().filter(|t| !t.is_empty()).count());
        }
    }

    #[test]
    fn every_key_is_a_string_in_the_window() {
        for language in &LANGUAGES[1..] {
            let parsed: HashMap<String, String> = serde_json::from_str(language.json).unwrap();
            for en in parsed.keys() {
                let literal = escaped(en);
                assert!(
                    SOURCES.iter().any(|s| s.contains(&literal)),
                    "lang/{}.json has {literal}, which the window no longer uses",
                    language.code
                );
            }
        }
    }

    /// The English text of every `tr("…")` and `trf!("…")` in `source`,
    /// with Rust's string escapes undone.
    fn looked_up(source: &str) -> Vec<String> {
        let mut found = Vec::new();
        for opener in ["tr(", "trf!("] {
            for (at, _) in source.match_indices(opener) {
                // `attr("` or `str("` is not a lookup.
                let before = source[..at].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                // rustfmt may put a long string on the line after `tr(`.
                let rest = source[at + opener.len()..].trim_start();
                let Some(literal) = rest.strip_prefix('"') else {
                    continue;
                };
                let mut text = String::new();
                let mut chars = literal.chars().peekable();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
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
                found.push(text);
            }
        }
        found
    }

    #[test]
    fn every_source_with_strings_is_scanned() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("i18n.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            if !looked_up(&source).is_empty() {
                assert!(
                    SOURCES.contains(&source.as_str()),
                    "{} looks up strings but is not in SOURCES",
                    path.display()
                );
            }
        }
    }

    /// Strings that were already untranslated when this check came in
    /// (2026-11-06). The list only shrinks: a new string needs its
    /// translations, and a translated one leaves the list.
    const UNTRANSLATED: &str = include_str!("lang/untranslated.json");

    #[test]
    fn every_string_in_the_window_is_translated() {
        let known: Vec<String> = serde_json::from_str(UNTRANSLATED).unwrap();
        let used: Vec<String> = SOURCES.iter().flat_map(|s| looked_up(s)).collect();
        let tables: Vec<(&str, HashMap<String, String>)> =
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
    fn resolve_picks_the_language() {
        assert_eq!(resolve("de").code, "de");
        assert_eq!(resolve("fr-CA").code, "fr");
        assert_eq!(resolve("xx").code, "en");
        assert_eq!(resolve("pt-BR").code, "pt-BR");
        assert_eq!(resolve("pt_PT").code, "pt-BR");
        assert_eq!(resolve("zh-CN").code, "zh-CN");
        assert_eq!(resolve("zh-SG").code, "zh-CN");
        assert_eq!(resolve("zh-Hans").code, "zh-CN");
        assert_eq!(resolve("zh-TW").code, "zh-TW");
        assert_eq!(resolve("zh-HK").code, "zh-TW");
        assert_eq!(resolve("zh-Hant-MO").code, "zh-TW");
        assert_eq!(resolve("ja-JP").code, "ja");
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
            assert!(std::ptr::eq(resolve(language.code), language), "{} resolves elsewhere", language.code);
        }
    }

    /// egui draws text without falling back to the system's fonts, so a
    /// language without a script font of its own must draw entirely with
    /// egui's built-in ones (where Windows' Segoe UI lacks a character).
    #[test]
    fn languages_without_a_script_font_draw_with_eguis_fonts() {
        use eframe::egui;
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let font = egui::FontId::proportional(13.0);
        for language in LANGUAGES.iter().filter(|l| l.font.is_none()) {
            let parsed: HashMap<String, String> = serde_json::from_str(language.json).unwrap();
            let mut missing: Vec<char> = parsed
                .values()
                .flat_map(|t| t.chars())
                .chain(language.name.chars())
                .filter(|c| !c.is_control() && !ctx.fonts(|f| f.has_glyph(&font, *c)))
                .collect();
            missing.sort_unstable();
            missing.dedup();
            assert!(missing.is_empty(), "lang/{}.json: egui's fonts lack {missing:?}", language.code);
        }
    }

    #[test]
    fn numbers_and_dates_follow_the_language() {
        let get = |code: &str| LANGUAGES.iter().find(|l| l.code == code).unwrap();
        assert_eq!(get("en").number(2.5, 1), "2.5");
        assert_eq!(get("de").number(2.5, 1), "2,5");
        assert_eq!(get("ja").number(1234.567, 2), "1234.57");
        let when = ((2026, 3, 7), 14 * 60 + 5);
        assert_eq!(get("en").date_time(when.0, when.1), "2026-03-07 14:05");
        assert_eq!(get("de").date_time(when.0, when.1), "07.03.2026 14:05");
        assert_eq!(get("fr").date_time(when.0, when.1), "07/03/2026 14:05");
        assert_eq!(get("ko").date_time(when.0, when.1), "2026. 03. 07. 14:05");
        assert_eq!(get("zh-TW").date_time(when.0, when.1), "2026/03/07 14:05");
    }

    #[test]
    fn trf_fills_placeholders() {
        assert_eq!(trf!("{n} of {total}", n = 2, total = "5"), "2 of 5");
    }
}
