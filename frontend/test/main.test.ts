import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Main from '../src/windows/main/Main.svelte';
import { fixtures, sent, shell } from './helpers';

describe('the main window', () => {
  it('draws the rows, the tree and the toolbar the snapshot holds', () => {
    shell('main', 'main');
    render(Main);
    const rows = screen.getAllByRole('row').filter((r) => r.getAttribute('aria-rowindex'));
    expect(rows.length).toBe(fixtures.views.main.view.rows.length);
    expect(screen.getByText('interview-notes.pdf')).toBeTruthy();
    expect(screen.getByRole('treeitem', { name: /All Downloads/ }).getAttribute('aria-selected')).toBe('true');
    expect((screen.getByRole('button', { name: /Stop All/ }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole('button', { name: /Resume$/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('sends what the user clicks', async () => {
    const calls = shell('main', 'main');
    render(Main);
    await fireEvent.click(screen.getByRole('button', { name: /Stop All/ }));
    expect(sent(calls, 'run_list_action').at(-1)?.args).toEqual({ action: { action: 'stopAll' } });
    await fireEvent.click(screen.getByRole('button', { name: /Add URL/ }));
    expect(sent(calls, 'open_window').at(-1)?.args).toEqual({ window: 'addUrl' });
    await fireEvent.mouseDown(screen.getByText('rufus-4.15.exe'), { button: 0 });
    expect(sent(calls, 'select_rows').at(-1)?.args).toEqual({ ids: [15] });
    await fireEvent.click(screen.getByRole('button', { name: 'Size' }));
    expect(sent(calls, 'sort_list').at(-1)?.args).toEqual({ column: 'size' });
    await fireEvent.click(screen.getByRole('treeitem', { name: /Finished/ }));
    expect(sent(calls, 'select_node').at(-1)?.args).toEqual({ key: 'finished' });
  });

  it('opens the list menu on a right click', async () => {
    shell('main', 'main');
    render(Main);
    await fireEvent.contextMenu(screen.getByRole('grid'), { clientX: 200, clientY: 120 });
    expect(screen.getByRole('menuitem', { name: 'Redownload' }).getAttribute('aria-disabled')).toBe('false');
    expect(screen.getByRole('menuitem', { name: 'Open with...' }).getAttribute('aria-disabled')).toBe('true');
  });

  it('opens the menu bar menus', async () => {
    const calls = shell('main', 'main');
    render(Main);
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Help' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'About UDM' }));
    expect(sent(calls, 'open_window').at(-1)?.args).toEqual({ window: 'about' });
  });
});
