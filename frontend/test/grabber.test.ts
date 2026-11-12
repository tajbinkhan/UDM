import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Grabber from '../src/windows/grabber/Grabber.svelte';
import { grab } from '../src/windows/grabber/grab.svelte';
import { sent, shell } from './helpers';

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true;
};

function fresh() {
  grab.draft = null;
  grab.checked = [];
  grab.results = -1;
}

describe('the site grabber', () => {
  it('walks the wizard and starts the crawl', async () => {
    fresh();
    const calls = shell('grabber', 'grabber');
    render(Grabber);
    await fireEvent.input(screen.getByLabelText('Start page/address'), { target: { value: 'https://example.com/' } });
    const next = screen.getByRole('button', { name: 'Next >' });
    await fireEvent.click(next);
    expect(screen.getByText(/Every file to a folder according to UDM category/)).toBeTruthy();
    await fireEvent.click(next);
    await fireEvent.click(next);
    await fireEvent.input(screen.getByLabelText('Download the following files (file types)'), { target: { value: 'zip, pdf' } });
    await fireEvent.click(next);
    const draft = sent(calls, 'grab_start').at(-1)?.args.draft as Record<string, unknown>;
    expect(draft).toMatchObject({ start: 'https://example.com/', types: 'zip, pdf', save: 'categories' });
  });

  it('adds the ticked results to a queue', async () => {
    fresh();
    const calls = shell('grabber', 'grabber-results');
    render(Grabber);
    expect(screen.getByText('manual.pdf')).toBeTruthy();
    await fireEvent.click(screen.getByRole('checkbox', { name: 'setup.exe' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Download checked' }));
    expect(sent(calls, 'grab_add').at(-1)?.args).toEqual({ indices: [0, 2], queue: 1, start: true });
    await fireEvent.click(screen.getByRole('treeitem', { name: /127\.0\.0\.1$/ }));
    expect(sent(calls, 'grab_select').at(-1)?.args).toEqual({ key: 'host/127.0.0.1' });
  });
});
