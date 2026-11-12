import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Options from '../src/windows/options/Options.svelte';
import { editing } from '../src/windows/options/draft.svelte';
import { sent, shell } from './helpers';

// jsdom has no modal dialogs.
HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true;
};

describe('the Options window', () => {
  it('saves the draft with what the user changed', async () => {
    editing.draft = null;
    const calls = shell('options', 'options');
    render(Options);
    await fireEvent.change(screen.getByLabelText('Theme'), { target: { value: 'dark' } });
    await fireEvent.click(screen.getByRole('tab', { name: 'Connection' }));
    await fireEvent.click(screen.getByLabelText('Download limits'));
    await fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    const draft = sent(calls, 'save_options').at(-1)?.args.draft as Record<string, unknown>;
    expect(draft.theme).toBe('dark');
    expect(draft.quotaEnabled).toBe(true);
  });

  it('adds a site login from its sheet', async () => {
    editing.draft = null;
    const calls = shell('options', 'options');
    render(Options);
    await fireEvent.click(screen.getByRole('tab', { name: 'Sites Logins' }));
    await fireEvent.click(screen.getByRole('button', { name: 'New' }));
    await fireEvent.input(screen.getByLabelText('Server/path'), { target: { value: 'files.example.com' } });
    await fireEvent.input(screen.getByLabelText('User'), { target: { value: 'me' } });
    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'pw' } });
    const sheet = screen.getByRole('dialog', { name: 'Site login' });
    await fireEvent.submit(sheet.querySelector('form') as HTMLFormElement);
    expect(screen.getByText('files.example.com')).toBeTruthy();
    await fireEvent.click(screen.getAllByRole('button', { name: 'OK' }).at(-1) as HTMLElement);
    const draft = sent(calls, 'save_options').at(-1)?.args.draft as { logins: unknown[] };
    expect(draft.logins).toContainEqual({ site: 'files.example.com', user: 'me', password: 'pw', stored: false });
  });
});
