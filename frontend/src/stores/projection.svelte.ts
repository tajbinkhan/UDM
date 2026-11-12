// The window's projection store (AGENTS.md section 7.4): the look and the
// view the shell sends. Only the subscription writes it, through `receive`;
// components read it.
import type { Look, MainPatch, MainView, Update, View } from '../ipc/types';

class Projection {
  look = $state<Look | null>(null);
  view = $state<View | null>(null);
  /** The window's subject is gone; the shell is closing the window. */
  gone = $state(false);
}

const store = new Projection();

/** What the window draws. Read only. */
export const projection: Readonly<Projection> = store;

function patchMain(view: MainView, patch: MainPatch): MainView {
  // A lookup made and dropped within this call, never drawn from.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const rows = new Map(view.rows.map((r) => [r.id, r]));
  for (const r of patch.rows) rows.set(r.id, r);
  const order = patch.order ?? view.rows.map((r) => r.id);
  return {
    rows: order.flatMap((id) => {
      const r = rows.get(id);
      return r ? [r] : [];
    }),
    chrome: patch.chrome ?? view.chrome,
  };
}

/** Take one update from the subscription. */
export function receive(update: Update): void {
  switch (update.kind) {
    case 'snapshot':
      store.look = update.look;
      store.view = update.view;
      break;
    case 'patch': {
      if (update.look) store.look = update.look;
      const { patch } = update;
      if (patch.window === 'whole') {
        store.view = patch.patch;
      } else if (store.view?.window === 'main') {
        store.view = { window: 'main', view: patchMain(store.view.view, patch.patch) };
      }
      break;
    }
    case 'gone':
      store.gone = true;
      break;
  }
}
