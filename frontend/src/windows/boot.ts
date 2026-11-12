// Start a window: send its errors to the log, draw its root and subscribe
// it to its view.
import { mount, type Component } from 'svelte';
import { connect, heartbeat, reportErrors } from '../ipc';

export function boot(Root: Component): void {
  reportErrors();
  const target = document.getElementById('app');
  if (target) mount(Root, { target });
  void connect();
  beat();
}

/** Tell the shell's watchdog this window answers while it is visible
 * (AGENTS.md section 7.3); a hidden one is not watched. */
function beat(): void {
  const send = () => void heartbeat(document.visibilityState === 'visible').catch(() => undefined);
  send();
  setInterval(() => {
    if (document.visibilityState === 'visible') send();
  }, 3000);
  document.addEventListener('visibilitychange', send);
}
