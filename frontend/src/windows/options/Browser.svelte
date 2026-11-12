<!-- Options > Browser: which browser downloads are big enough to take, and
     the buttons UDM shows on pages. -->
<script lang="ts">
  import { addBrowser, messageOf, pickPath, removeBrowser, restartBrowser, setBrowser } from '../../ipc';
  import type { OptionsView, SelectionChoice } from '../../ipc/types';
  import { t } from '../../i18n';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  let { view }: { view: OptionsView } = $props();
  let error = $state('');

  async function add() {
    error = '';
    try {
      const program = await pickPath('program', '');
      if (program) await addBrowser(program);
    } catch (err) {
      error = messageOf(err);
    }
  }

  // The pages are drawn only while the draft exists.
  const d = $derived(editing.draft!);
  const e = $derived(d.extras);
  const places: [SelectionChoice, string][] = [
    ['off', 'Nowhere'],
    ['everywhere', 'On every site'],
    ['sites', 'On these sites only'],
  ];
</script>

<PageHead icon="browser" title={t('Browser capture')} />
<div class="opt-col">
  <fieldset class="group opt-col">
    <legend>{t('Browsers')}</legend>
    <p class="opt-note">{t('Capture downloads from these browsers. Each needs UDM\'s extension; restart a browser after a change.')}</p>
    {#if !view.browsers}
      <p class="opt-note" role="status">…</p>
    {:else}
      {#each view.browsers as b (b.index)}
        <div class="row">
          <label class="check grow" title={b.detail}
            ><input type="checkbox" checked={b.registered} onchange={(ev) => void setBrowser(b.index, ev.currentTarget.checked)} />{b.name}</label
          >
          {#if b.canRestart}
            <button class="btn small" title={t('Close this browser and start it again, so it sees a change')} onclick={() => void restartBrowser(b.index)}
              >{t('Restart')}</button
            >
          {/if}
          {#if b.added}<button class="btn small" onclick={() => void removeBrowser(b.index)}>{t('Remove')}</button>{/if}
        </div>
      {:else}
        <p class="opt-note">{t('No browser was found.')}</p>
      {/each}
    {/if}
    {#if view.chromeNote}<p class="opt-note">{view.chromeNote}</p>{/if}
    {#if view.browserNote}<p class:error-text={view.browserNote.error} class:opt-note={!view.browserNote.error} role="status">{view.browserNote.text}</p>{/if}
    {#if error}<p class="error-text" role="alert">{error}</p>{/if}
    <button class="btn opt-right" title={t('Choose the program of a browser that is not in the list')} onclick={() => void add()}>{t('Add browser...')}</button>
  </fieldset>
  <div class="row">
    <label for="min-size">{t('Minimum size')}</label>
    <input id="min-size" class="text num" type="number" min="0" bind:value={e.captureMinSizeKb} />
    <span>{t('KB (0 takes every size)')}</span>
  </div>
  <fieldset class="group opt-col">
    <legend>{t('Selected links')}</legend>
    <label for="selection">{t('Show a Download with UDM button by selected text with links')}</label>
    <select id="selection" class="select places" bind:value={e.selectionButton}>
      {#each places as [value, label] (value)}<option {value}>{t(label)}</option>{/each}
    </select>
    {#if e.selectionButton === 'sites'}
      <textarea class="opt-area" bind:value={e.selectionSites} placeholder={t('One site per line, such as *.example.com')} aria-label={t('On these sites only')}
      ></textarea>
    {/if}
  </fieldset>
  <fieldset class="group opt-col" class:disabled={!d.videoButton}>
    <legend>{t('Videos')}</legend>
    <label for="no-video">{t('No video button on')}</label>
    <textarea
      id="no-video"
      class="opt-area"
      bind:value={e.videoExcludedSites}
      disabled={!d.videoButton}
      placeholder={t('One site per line, such as *.example.com')}
    ></textarea>
    <div class="row">
      <label for="video-min">{t('Minimum size')}</label>
      <input id="video-min" class="text num" type="number" min="0" bind:value={e.videoMinSizeKb} disabled={!d.videoButton} />
      <span>{t('KB: smaller video and audio files are left out of the list')}</span>
    </div>
    <label for="auto-types">{t('Download these types from web players at once, without the video button:')}</label>
    <input id="auto-types" class="text" bind:value={e.videoAutoTypes} disabled={!d.videoAutoCapture} spellcheck="false" />
    <p class="opt-note">{t('Whole files a page\'s player loads, each address once per tab; not HLS or DASH streams')}</p>
  </fieldset>
</div>

<style>
  .num {
    width: 80px;
  }

  p {
    margin: 0;
  }

  .row > label {
    white-space: nowrap;
  }

  .places {
    width: 190px;
  }
</style>
