# Help translate UDM

UDM's window comes in 21 languages and its browser extension in the same
21. Most of them were made with a language model and have not been read by a
native speaker yet. If you speak one of them, a review is the most useful help
you can give; a new language is welcome too.

## Where the text is

- **The window**: `crates/ui/src/lang/<code>.json`. Each file is one JSON
  object from the English text, which is the string's ID in the code, to its
  translation. A string missing from a table shows in English.
- **The extension**: `extension/chromium/_locales/<code>/messages.json`, as Chrome
  expects. `extension/chromium/_locales/en/messages.json` has a `description` for each
  message that says where it shows.
- **The language list**: `LANGUAGES` in `crates/ui/src/i18n.rs` names each
  language and how it writes decimals and dates.

## The languages

| Language | Window | Extension | Made by |
| --- | --- | --- | --- |
| German (`de`) | `de.json` | `de` | first translations |
| Spanish (`es`) | `es.json` | `es` | first translations |
| French (`fr`) | `fr.json` | `fr` | first translations |
| Indonesian (`id`) | `id.json` | `id` | machine, needs review |
| Italian (`it`) | `it.json` | `it` | machine, needs review |
| Dutch (`nl`) | `nl.json` | `nl` | machine, needs review |
| Polish (`pl`) | `pl.json` | `pl` | machine, needs review |
| Portuguese, Brazil (`pt-BR`) | `pt-BR.json` | `pt_BR` | machine, needs review |
| Vietnamese (`vi`) | `vi.json` | `vi` | machine, needs review |
| Turkish (`tr`) | `tr.json` | `tr` | machine, needs review |
| Russian (`ru`) | `ru.json` | `ru` | machine, needs review |
| Ukrainian (`uk`) | `uk.json` | `uk` | machine, needs review |
| Hebrew (`he`) | `he.json` | `he` | machine, needs review |
| Arabic (`ar`) | `ar.json` | `ar` | machine, needs review |
| Persian (`fa`) | `fa.json` | `fa` | machine, needs review |
| Hindi (`hi`) | `hi.json` | `hi` | machine, needs review |
| Japanese (`ja`) | `ja.json` | `ja` | machine, needs review |
| Korean (`ko`) | `ko.json` | `ko` | machine, needs review |
| Chinese, Simplified (`zh-CN`) | `zh-CN.json` | `zh_CN` | machine, needs review |
| Chinese, Traditional (`zh-TW`) | `zh-TW.json` | `zh_TW` | machine, needs review |

"Machine" means every string in that table was translated by a language model
from the English, with the German and French tables beside it for meaning. The
checks below passed, but nobody has read the result in the running window.

## Reviewing a table

1. Pick the language in **Options > General > Language** (or **View >
   Language**) and use UDM for a while: the main window, Options, the
   save-as dialog, a download window, the Scheduler and the Site grabber.
2. Fix what reads wrong in the JSON file. Keep each `{placeholder}` (you may
   move it), a `&` before a menu letter, the `...` after a command that opens a
   dialog, and the `\n` line breaks. Some labels are pieces of one sentence
   around a number field ("Explore", "pages at once, and download", "files at
   once"); check them on screen.
3. When the whole table reads right, open a pull request that changes its row
   above from "machine, needs review" to "reviewed", and say which parts of the
   window you went through.

Use the terms your language's version of Windows uses (Options, Recycle Bin,
notification area) and keep one word for each idea: download, queue, category,
resume, scheduler, site grabber.

## Adding a language

1. Copy `crates/ui/src/lang/de.json` to `<code>.json` and translate every
   value. `<code>` is the ISO 639-1 code, with a region only where the written
   language differs by it (`pt-BR`, `zh-TW`).
2. Add a line to `LANGUAGES` in `crates/ui/src/i18n.rs`, ordered by the
   language's own name, with its decimal separator and date pattern.
3. Add `extension/chromium/_locales/<code>/messages.json` with every message in
   `extension/chromium/_locales/en/messages.json`, and the same placeholders.
4. Add a row to the table above.
5. Run `node installer/make-firefox.js`, which copies the new locale into
   `extension/firefox`.
6. Run `cargo test -p udm-ui i18n` and `node --test extension/test/*.test.js`.

The tests check that every key is still a string in the window, keeps its
placeholders, and that every language has every string but those
`crates/ui/src/lang/untranslated.json` lists; and that every extension locale
has the English messages with the same placeholders.

### Right-to-left languages

The window draws its text in the system's webview, which shapes every script
(Arabic, Hebrew, Devanagari and the rest) and lays it out right to left.
`Language::right_to_left` in `crates/ui/src/i18n.rs` names the languages
written that way (Hebrew, Arabic and Persian today, and Urdu when it comes),
and each window then sets `dir="rtl"`. The layout uses CSS's logical sides
(`margin-inline-start`, `inset-inline-end`, `text-align: start`), so panels,
columns, tree indents and dialogs mirror; use them, not `left` and `right`,
in new styles. The extension's popup takes its direction from the browser's
`@@bidi_dir` message.

Persian dates show in the Gregorian calendar, written year first.

---

[← Back to the help pages](README.md)
