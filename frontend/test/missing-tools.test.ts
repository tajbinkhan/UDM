import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Main from '../src/windows/main/Main.svelte';
import { receive } from '../src/stores/projection.svelte';
import type { MissingTools, Update } from '../src/ipc/types';
import { fixtures, sent, shell } from './helpers';

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true;
};

const missing = (ask: boolean): MissingTools => ({
  text: 'Some downloads need tools UDM did not find: yt-dlp, ffmpeg.',
  strip: true,
  ask,
  busy: false,
  busyText: 'Installing...',
  tools: [
    {
      tool: 'ytDlp',
      name: 'yt-dlp',
      purpose: 'Downloads videos from YouTube and many other video sites.',
      installable: true,
      hint: '',
      progress: null,
    },
    {
      tool: 'ffmpeg',
      name: 'ffmpeg',
      purpose: 'Joins streamed videos into one .mp4.',
      installable: false,
      hint: 'UDM cannot install it here. Install it with: sudo apt install ffmpeg',
      progress: null,
    },
  ],
});

function show(tools: MissingTools) {
  const calls = shell('main', 'main');
  const main = structuredClone(fixtures.views.main);
  (main.view.chrome as { missingTools: MissingTools | null }).missingTools = tools;
  receive({ kind: 'snapshot', seq: 0, look: fixtures.look.dark, view: main } as Update);
  render(Main);
  return calls;
}

describe('the missing tools', () => {
  it('the strip installs the ones UDM can install, and closes', async () => {
    const calls = show(missing(false));
    expect(screen.getByText('Some downloads need tools UDM did not find: yt-dlp, ffmpeg.')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Install them' }));
    expect(sent(calls, 'answer_tools_offer').at(-1)?.args).toEqual({ tools: ['ytDlp'] });
    await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(sent(calls, 'dismiss_tools')).toHaveLength(1);
  });

  it('the first start asks, and Not now installs nothing', async () => {
    const calls = show(missing(true));
    const dialog = await screen.findByRole('dialog');
    expect(dialog.textContent).toContain('sudo apt install ffmpeg');
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(sent(calls, 'answer_tools_offer').at(-1)?.args).toEqual({ tools: [] });
  });

  it('an install shows how far its download is', async () => {
    const busy = missing(true);
    busy.busy = true;
    busy.busyText = 'Installing... 30%';
    busy.tools[0].progress = { text: '12.0 MB of 40.0 MB (30%)', percent: 30 };
    show(busy);
    const dialog = await screen.findByRole('dialog');
    expect(dialog.textContent).toContain('12.0 MB of 40.0 MB (30%)');
    expect(screen.getAllByText('Installing... 30%').length).toBeGreaterThan(0);
    const bar = dialog.querySelector('[role="progressbar"]');
    expect(bar?.getAttribute('aria-valuenow')).toBe('30');
    // Without a size, only the words.
    busy.tools[0].progress = { text: '12.0 MB downloaded', percent: null };
    const main = structuredClone(fixtures.views.main);
    (main.view.chrome as { missingTools: MissingTools | null }).missingTools = busy;
    receive({ kind: 'snapshot', seq: 0, look: fixtures.look.dark, view: main } as Update);
    await screen.findByText('12.0 MB downloaded');
    expect(dialog.querySelector('[role="progressbar"]')).toBeNull();
  });

  it('the sheet installs the ticked tools', async () => {
    const calls = show(missing(true));
    const dialog = await screen.findByRole('dialog');
    await fireEvent.submit(dialog.querySelector('form') as HTMLFormElement);
    expect(sent(calls, 'answer_tools_offer').at(-1)?.args).toEqual({ tools: ['ytDlp'] });
  });
});
