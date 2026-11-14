import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Main from '../src/windows/main/Main.svelte';
import { receive } from '../src/stores/projection.svelte';
import type { Update } from '../src/ipc/types';
import { fixtures, sent, shell } from './helpers';

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true;
};

describe('the window language', () => {
  it('View > Language lists every language and switches to one', async () => {
    const calls = shell('main', 'main');
    render(Main);
    await fireEvent.click(screen.getByRole('menuitem', { name: 'View' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Language' }));
    expect(screen.getByRole('menuitemcheckbox', { name: 'System default' }).getAttribute('aria-checked')).toBe('true');
    expect(screen.getByRole('menuitemcheckbox', { name: 'العربية' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('menuitemcheckbox', { name: 'Deutsch' }));
    expect(sent(calls, 'set_language').at(-1)?.args).toEqual({ code: 'de' });
  });

  it('the first start asks for a language, starting on the one in use', async () => {
    const calls = shell('main', 'main');
    const main = structuredClone(fixtures.views.main);
    main.view.chrome.askLanguage = true;
    receive({ kind: 'snapshot', seq: 0, look: fixtures.look.dark, view: main } as Update);
    render(Main);
    const list = (await screen.findByLabelText(/Which language should UDM use/)) as HTMLSelectElement;
    expect(list.value).toBe('en');
    expect([...list.options].some((o) => o.value === '')).toBe(false);
    await fireEvent.change(list, { target: { value: 'he' } });
    await fireEvent.submit(screen.getByRole('dialog').querySelector('form') as HTMLFormElement);
    expect(sent(calls, 'set_language').at(-1)?.args).toEqual({ code: 'he' });
  });
});
