// The window's text (AGENTS.md section 7.4): `t('Pause')` looks the English
// up in the table the shell sends with the snapshot and falls back to the
// English. `{name}` placeholders are filled after the lookup.
import { projection } from '../stores/projection.svelte';

export function t(english: string, values?: Record<string, string | number>): string {
  let text = projection.look?.text[english] ?? english;
  if (values) {
    for (const [name, value] of Object.entries(values)) text = text.replaceAll(`{${name}}`, String(value));
  }
  return text;
}

/** English kept in a variable and looked up later with `t()`, such as a
 * toolbar button's label. It returns the text unchanged; it is there so the
 * shell's tests (`crates/ui/src/i18n.rs`) find the string and require its
 * translations. Menu labels need no mark: the tests read the labels of
 * `item` and `sub` calls too. */
export function english(text: string): string {
  return text;
}

/** Whether the window is laid out right to left (Hebrew, Arabic, Persian),
 * for code that places things by the screen's coordinates. */
export function rightToLeft(): boolean {
  return document.documentElement.dir === 'rtl';
}

/** A menu label's text and its underlined letter: `&Tasks` is Tasks with T
 * underlined. */
export function mnemonic(english: string): { before: string; key: string; after: string } {
  const text = t(english);
  const at = text.indexOf('&');
  if (at < 0 || at === text.length - 1) return { before: text.replace('&', ''), key: '', after: '' };
  return { before: text.slice(0, at), key: text[at + 1], after: text.slice(at + 2).replace('&', '') };
}

/** A menu label without its `&`. */
export function plain(english: string): string {
  const m = mnemonic(english);
  return m.before + m.key + m.after;
}
