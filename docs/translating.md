# Help translate UDM

UDM's window comes in 17 languages and its browser extension in the same
17. Most of them were made with a language model and have not been read by a
native speaker yet. If you speak one of them, a review is the most useful help
you can give; a new language is welcome too.

## Where the text is

- **The window**: `crates/ui/src/lang/<code>.json`. Each file is one JSON
  object from the English text, which is the string's ID in the code, to its
  translation. A string missing from a table shows in English.
- **The extension**: `extension/_locales/<code>/messages.json`, as Chrome
  expects. `extension/_locales/en/messages.json` has a `description` for each
  message that says where it shows.
- **The language list**: `LANGUAGES` in `crates/ui/src/i18n.rs` names each
  language, its system font (for Chinese, Japanese, Korean and Vietnamese),
  and how it writes decimals and dates.

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
3. Add `extension/_locales/<code>/messages.json` with every message in
   `extension/_locales/en/messages.json`, and the same placeholders.
4. Add a row to the table above.
5. Run `cargo test -p udm-ui i18n` and `node --test extension/test/*.test.js`.

The tests check that every key is still a string in the window, keeps its
placeholders, and that every language has every string; that every extension
locale has the English messages with the same placeholders; and that every
character of a language without a system font of its own can be drawn with the
window's built-in fonts.

### Languages the window cannot show yet

egui, which draws the window, lays text out one character after another, left
to right. It does not shape text or reorder right-to-left text. So Arabic,
Persian, Urdu and Hebrew, and scripts that need shaping such as Hindi, Bengali,
Tamil, Telugu and Thai, would show disconnected, reversed or wrongly placed
letters. Until that changes ([Known gaps](known-gaps.md)), a table for them
cannot be added to the window. The extension could take one, since the browser
draws its text.

---

[← Back to the help pages](README.md)
