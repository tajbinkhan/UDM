import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Main from '../src/windows/main/Main.svelte';
import Grabber from '../src/windows/grabber/Grabber.svelte';
import { grab } from '../src/windows/grabber/grab.svelte';
import { sent, shell } from './helpers';

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true;
};

async function menu(top: string, ...path: string[]) {
  await fireEvent.click(screen.getByRole('menuitem', { name: top }));
  for (const name of path) await fireEvent.click(screen.getByRole('menuitem', { name }));
}

describe('the last pieces', () => {
  it('View > Toolbar switches to small buttons', async () => {
    const calls = shell('main', 'main');
    render(Main);
    await menu('View', 'Toolbar');
    await fireEvent.click(screen.getByRole('menuitemcheckbox', { name: 'Small Buttons' }));
    const args = sent(calls, 'set_toolbar').at(-1)?.args as { small: boolean; buttons: string[] };
    expect(args.small).toBe(true);
    expect(args.buttons[0]).toBe('add');
  });

  it('View > Font chooses a fixed-width font', async () => {
    const calls = shell('main', 'main');
    render(Main);
    await menu('View', 'Font', 'Choose Font...');
    await fireEvent.change(screen.getByLabelText('Font:'), { target: { value: 'mono' } });
    await fireEvent.submit(screen.getByRole('dialog').querySelector('form') as HTMLFormElement);
    expect(sent(calls, 'set_list_font').at(-1)?.args).toEqual({ font: { mono: true, size: 12 } });
  });

  it('the tip of the day moves on and remembers the startup box', async () => {
    const calls = shell('main', 'main');
    render(Main);
    await menu('Help', 'Tip of the Day...');
    await fireEvent.click(screen.getByRole('button', { name: 'Next Tip' }));
    await fireEvent.click(screen.getAllByRole('button', { name: 'Close' }).at(-1) as HTMLElement);
    expect(sent(calls, 'close_tip').at(-1)?.args).toMatchObject({ next: 2 });
  });

  it("the grabber asks to save a project before scheduling it", async () => {
    grab.draft = null;
    grab.schedule = null;
    grab.results = -1;
    shell('grabber', 'grabber-results');
    render(Grabber);
    await menu('Options', 'Schedule...');
    expect(screen.getByText(/save the project at first/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Yes' }));
    expect(screen.getByLabelText('Save this project with the following name')).toBeTruthy();
  });
});
