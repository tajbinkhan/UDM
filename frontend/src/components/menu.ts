// The entries of a menu: items, submenus and separators. Labels are the
// English message IDs, with `&` before the underlined letter.

export type MenuEntry =
  | { kind: 'item'; label: string; shortcut?: string; disabled?: boolean; checked?: boolean; run?: () => void }
  | { kind: 'sub'; label: string; disabled?: boolean; items: MenuEntry[] }
  | { kind: 'sep' };

export const item = (
  label: string,
  run: (() => void) | undefined,
  opts: { disabled?: boolean; shortcut?: string; checked?: boolean } = {},
): MenuEntry => ({ kind: 'item', label, run, ...opts, disabled: opts.disabled || !run });

export const sub = (label: string, items: MenuEntry[], disabled = false): MenuEntry => ({
  kind: 'sub',
  label,
  items,
  disabled: disabled || items.length === 0,
});

export const sep: MenuEntry = { kind: 'sep' };
