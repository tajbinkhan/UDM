// Every keyboard shortcut a window answers (AGENTS.md section 7.4). Each
// must be in docs/keyboard-shortcuts.md too; test/keys.test.ts compares them.

export type Shortcut = { keys: string; window: 'main' | 'dialog' | 'any'; does: string };

export const shortcuts: Shortcut[] = [
  { keys: 'Tab', window: 'any', does: 'Move to the next control' },
  { keys: 'Escape', window: 'any', does: 'Close a menu or the dialog' },
  { keys: 'F10', window: 'main', does: 'Go to the menu bar' },
  { keys: 'Up', window: 'main', does: 'Select the download above' },
  { keys: 'Down', window: 'main', does: 'Select the download below' },
  { keys: 'Page Up', window: 'main', does: 'Select a screenful up' },
  { keys: 'Page Down', window: 'main', does: 'Select a screenful down' },
  { keys: 'Home', window: 'main', does: 'Select the first download' },
  { keys: 'End', window: 'main', does: 'Select the last download' },
  { keys: 'Space', window: 'main', does: 'Select the download with the focus' },
  { keys: 'Ctrl+A', window: 'main', does: 'Select every download shown' },
  { keys: 'Enter', window: 'main', does: 'Open it, or show its download window' },
  { keys: 'Delete', window: 'main', does: 'Remove the selected downloads' },
  { keys: 'Shift+F10', window: 'main', does: "Open the download's menu" },
  { keys: 'Ctrl+F', window: 'main', does: 'Find a download' },
  { keys: 'F3', window: 'main', does: 'Find the next match' },
  { keys: 'Right', window: 'main', does: 'Open a branch of the categories' },
  { keys: 'Left', window: 'main', does: 'Close a branch of the categories' },
];
