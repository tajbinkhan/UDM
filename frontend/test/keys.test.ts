import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { shortcuts } from '../src/keys';

// AGENTS.md section 7.4: every key a window answers is documented.
describe('the keyboard shortcuts', () => {
  const doc = readFileSync(join(import.meta.dirname, '../../docs/keyboard-shortcuts.md'), 'utf8');
  it.each(shortcuts.map((s) => s.keys))('%s is in docs/keyboard-shortcuts.md', (keys) => {
    expect(doc).toContain('`' + keys + '`');
  });
});
