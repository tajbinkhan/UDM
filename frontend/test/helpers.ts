// Draw a window from the fixtures the shell's projections made
// (crates/ui/src/fixtures.rs), with the shell mocked.
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import fixtures from './fixtures/views.json';
import { receive } from '../src/stores/projection.svelte';
import type { Update } from '../src/ipc/types';

export type Call = { cmd: string; args: Record<string, unknown> };

/** Mock the shell for window `label`, feed it the fixture `kind`, and
 * return the commands the window sends. */
export function shell(label: string, kind: keyof typeof fixtures.views, look: 'light' | 'dark' = 'dark'): Call[] {
  const calls: Call[] = [];
  mockWindows(label);
  mockIPC((cmd, args) => {
    calls.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    return null;
  });
  receive({ kind: 'snapshot', seq: 0, look: fixtures.look[look], view: fixtures.views[kind] } as Update);
  return calls;
}

export const sent = (calls: Call[], cmd: string) => calls.filter((c) => c.cmd === cmd);

export { fixtures };
