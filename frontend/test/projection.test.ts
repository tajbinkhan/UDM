import { describe, expect, it } from 'vitest';
import { projection, receive } from '../src/stores/projection.svelte';
import { fixtures } from './helpers';
import type { MainView, Update } from '../src/ipc/types';

describe('the projection store', () => {
  it('applies the main window patches row by row', () => {
    receive({ kind: 'snapshot', seq: 0, look: fixtures.look.dark, view: fixtures.views.main } as Update);
    const before = (projection.view as { view: MainView }).view;
    const changed = { ...before.rows[0], status: '50%' };
    const order = before.rows.map((r) => r.id).reverse();
    receive({ kind: 'patch', seq: 1, look: null, patch: { window: 'main', patch: { rows: [changed], order, chrome: null } } });
    const after = (projection.view as { view: MainView }).view;
    expect(after.rows.at(-1)?.status).toBe('50%');
    expect(after.rows[0].id).toBe(order[0]);
    expect(after.chrome).toEqual(before.chrome);
  });
});
