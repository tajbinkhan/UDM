import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Scheduler from '../src/windows/scheduler/Scheduler.svelte';
import { sent, shell } from './helpers';

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true;
};

describe('the Scheduler', () => {
  it('applies a queue schedule', async () => {
    const calls = shell('scheduler', 'scheduler');
    render(Scheduler);
    const apply = screen.getByRole('button', { name: 'Apply' }) as HTMLButtonElement;
    expect(apply.disabled).toBe(true);
    await fireEvent.click(screen.getByLabelText('Start download at', { selector: 'input[type=checkbox]' }));
    await fireEvent.click(screen.getByLabelText('Turn off computer when done', { selector: 'input[type=checkbox]' }));
    expect(apply.disabled).toBe(false);
    await fireEvent.click(apply);
    const draft = sent(calls, 'save_queue').at(-1)?.args.draft as Record<string, unknown>;
    expect(draft).toMatchObject({ id: 1, startOn: true, start: '23:00', then: 'shutDown' });
  });

  it('starts, stops and names queues', async () => {
    const calls = shell('scheduler', 'scheduler');
    render(Scheduler);
    await fireEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(sent(calls, 'control_queue').at(-1)?.args).toEqual({ id: 1, start: false });
    await fireEvent.click(screen.getByRole('button', { name: 'New queue' }));
    expect((screen.getByLabelText(/Enter a name/) as HTMLInputElement).value).toBe('Queue # 3');
    await fireEvent.submit(screen.getByRole('dialog').querySelector('form') as HTMLFormElement);
    expect(sent(calls, 'create_queue').at(-1)?.args).toEqual({ name: 'Queue # 3' });
  });

  it('applies the download limits', async () => {
    const calls = shell('scheduler', 'scheduler');
    render(Scheduler);
    await fireEvent.click(screen.getByRole('option', { name: /Download limits/ }));
    await fireEvent.click(screen.getByLabelText('Download limits'));
    await fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
    expect(sent(calls, 'save_quota').at(-1)?.args).toEqual({ quota: { enabled: true, mb: 1024, hours: 24, warn: true } });
  });
});
