// The windows the daemon asks for: a login, the countdown, a new link, a
// package's password, and the browser's links, videos and gallery.
import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Login from '../src/windows/login/Login.svelte';
import Countdown from '../src/windows/countdown/Countdown.svelte';
import Refresh from '../src/windows/refresh/Refresh.svelte';
import Password from '../src/windows/password/Password.svelte';
import PickLinks from '../src/windows/pick-links/PickLinks.svelte';
import PickVideos from '../src/windows/pick-videos/PickVideos.svelte';
import Gallery from '../src/windows/gallery/Gallery.svelte';
import { sent, shell } from './helpers';

describe('what the daemon asks', () => {
  it('the login prompt sends the user name, password and whether to keep it', async () => {
    const calls = shell('login', 'login');
    render(Login);
    expect(screen.getByText('127.0.0.1 asks for a user name and password.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('User name'), { target: { value: 'me' } });
    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'pw' } });
    await fireEvent.click(screen.getByLabelText('Save this password'));
    await fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(sent(calls, 'answer_login').at(-1)?.args).toEqual({ user: 'me', password: 'pw', save: true });
  });

  it('the countdown does it now or cancels', async () => {
    const calls = shell('countdown', 'countdown');
    render(Countdown);
    expect(screen.getByText('This computer will shut down in 60 seconds.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Shut down now' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(sent(calls, 'answer_countdown').map((c) => c.args)).toEqual([{ now: true }, { now: false }]);
  });

  it('New link sends the typed address or waits for the browser', async () => {
    const calls = shell('refresh', 'refresh');
    render(Refresh);
    await fireEvent.click(screen.getByRole('button', { name: 'Use my next download in the browser' }));
    await fireEvent.input(screen.getByLabelText('Paste the new address'), { target: { value: 'http://127.0.0.1/new.mp3' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Use this link' }));
    expect(sent(calls, 'refresh_link').map((c) => c.args)).toEqual([
      { id: 3, url: null, restart: false },
      { id: 3, url: 'http://127.0.0.1/new.mp3', restart: false },
    ]);
  });

  it('the password unpacks the package', async () => {
    const calls = shell('password', 'password');
    render(Password);
    await fireEvent.input(screen.getByLabelText('Archive password'), { target: { value: 'secret' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Unpack' }));
    expect(sent(calls, 'unpack_package').at(-1)?.args).toEqual({ id: 1, password: 'secret' });
  });

  it('the link picker adds the ticked links as a package', async () => {
    const calls = shell('pick-links', 'pick-links');
    render(PickLinks);
    await fireEvent.change(screen.getByLabelText('Types'), { target: { value: 'exe' } });
    await fireEvent.click(screen.getByLabelText('manual.pdf'));
    await fireEvent.click(screen.getByLabelText('Keep them together as a package'));
    await fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(sent(calls, 'add_picks').at(-1)?.args).toEqual({
      input: { indices: [1, 0], queue: 1, start: true, package: 'manual', quality: 0, folder: '' },
    });
  });

  it('the video picker sends the quality', async () => {
    const calls = shell('pick-videos', 'pick-videos');
    render(PickVideos);
    await fireEvent.click(screen.getByLabelText('Lesson 2: getting started'));
    await fireEvent.change(screen.getByLabelText('Quality'), { target: { value: '2' } });
    await fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(sent(calls, 'add_picks').at(-1)?.args).toEqual({
      input: { indices: [0, 2, 3], queue: 1, start: true, package: null, quality: 2, folder: '' },
    });
  });

  it('the gallery sends its folder', async () => {
    const calls = shell('gallery', 'gallery');
    render(Gallery);
    await fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(sent(calls, 'add_picks').at(-1)?.args).toEqual({
      input: { indices: [0, 1, 2], queue: 1, start: true, package: null, quality: 0, folder: '/home/u/Downloads/Album' },
    });
  });
});
