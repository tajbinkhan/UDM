// Start a window: send its errors to the log, draw its root and subscribe
// it to its view.
import { mount, type Component } from 'svelte';
import { connect, reportErrors } from '../ipc';

export function boot(Root: Component): void {
  reportErrors();
  const target = document.getElementById('app');
  if (target) mount(Root, { target });
  void connect();
}
