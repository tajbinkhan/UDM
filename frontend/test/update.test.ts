import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Main from '../src/windows/main/Main.svelte';
import { receive } from '../src/stores/projection.svelte';
import type { Update } from '../src/ipc/types';
import { fixtures, sent, shell } from './helpers';

describe('a newer release', () => {
  it('downloaded ahead, Update now installs it', async () => {
    const calls = shell('main', 'main');
    const main = structuredClone(fixtures.views.main);
    Object.assign(main.view.chrome, { newer: '9.0.0', newerInstallable: true, newerReady: true });
    receive({ kind: 'snapshot', seq: 0, look: fixtures.look.dark, view: main } as Update);
    render(Main);
    expect(screen.getByText('UDM 9.0.0 is downloaded and ready to install.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Update now' }));
    expect(sent(calls, 'install_update')).toHaveLength(1);
    expect(sent(calls, 'check_update')).toHaveLength(0);
  });
});
