// What jsdom lacks and the windows use.
import { afterEach } from 'vitest';
import { clearMocks } from '@tauri-apps/api/mocks';
import { cleanup } from '@testing-library/svelte';

class Resize {
  observe() {}
  unobserve() {}
  disconnect() {}
}

globalThis.ResizeObserver ??= Resize as unknown as typeof ResizeObserver;
window.matchMedia ??= ((query: string) => ({
  matches: false,
  media: query,
  onchange: null,
  addEventListener() {},
  removeEventListener() {},
  addListener() {},
  removeListener() {},
  dispatchEvent: () => false,
})) as unknown as typeof window.matchMedia;

// jsdom has no modal dialogs.
HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true;
};

afterEach(() => {
  cleanup();
  clearMocks();
});
