// A window drawn from the test fixtures in a plain browser, with Tauri's IPC
// mocked, for checking its looks while working on it (`pnpm dev`, then
// /dev/index.html?window=main&look=dark). Not part of the build.
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import fixtures from '../test/fixtures/views.json';

type Kind = keyof typeof fixtures.views;

const params = new URLSearchParams(location.search);
const kind = (params.get('window') ?? 'main') as Kind;
const look = params.get('look') === 'light' ? fixtures.look.light : fixtures.look.dark;
const labels: Record<Kind, string> = {
  main: 'main',
  'add-url': 'add-url',
  'save-as': 'save-as-17',
  download: 'download-12',
  done: 'done-17',
  remove: 'remove',
  about: 'about',
  options: 'options',
  scheduler: 'scheduler',
  grabber: 'grabber',
  'grabber-results': 'grabber',
  batch: 'batch',
  links: 'links',
  properties: 'properties-2',
  'drop-target': 'drop-target',
};

mockWindows(labels[kind]);
mockIPC((cmd, args) => {
  if (cmd === 'subscribe') {
    const channel = (args as { channel: { onmessage: (u: unknown) => void } }).channel;
    setTimeout(() => channel.onmessage({ kind: 'snapshot', seq: 0, look, view: fixtures.views[kind] }));
  }
  return null;
});

const entries: Record<Kind, () => Promise<unknown>> = {
  main: () => import('../src/windows/main/entry'),
  'add-url': () => import('../src/windows/add-url/entry'),
  'save-as': () => import('../src/windows/save-as/entry'),
  download: () => import('../src/windows/download/entry'),
  done: () => import('../src/windows/done/entry'),
  remove: () => import('../src/windows/remove/entry'),
  about: () => import('../src/windows/about/entry'),
  options: () => import('../src/windows/options/entry'),
  scheduler: () => import('../src/windows/scheduler/entry'),
  grabber: () => import('../src/windows/grabber/entry'),
  'grabber-results': () => import('../src/windows/grabber/entry'),
  batch: () => import('../src/windows/batch/entry'),
  links: () => import('../src/windows/links/entry'),
  properties: () => import('../src/windows/properties/entry'),
  'drop-target': () => import('../src/windows/drop-target/entry'),
};
void entries[kind]();
