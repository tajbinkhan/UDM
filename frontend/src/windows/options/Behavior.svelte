<!-- Options > Behavior: what UDM does when a download ends, when its file
     is taken, at startup and when removing downloads, and its size. -->
<script lang="ts">
  import type { FileExistsChoice, NotifyChoice, OptionsView, StartTabChoice } from '../../ipc/types';
  import { t } from '../../i18n';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  let { view }: { view: OptionsView } = $props();
  // The pages are drawn only while the draft exists.
  const d = $derived(editing.draft!);
  const e = $derived(d.extras);

  const notify: [NotifyChoice, string][] = [
    ['toast', 'Show a Windows notification'],
    ['dialog', 'Show a dialog in UDM'],
    ['off', 'Show nothing'],
  ];
  const exists: [FileExistsChoice, string][] = [
    ['rename', 'Save with a new name'],
    ['overwrite', 'Overwrite it'],
    ['ask', 'Ask me'],
  ];
  const tabs: [StartTabChoice, string][] = [
    ['status', 'Download status'],
    ['speedLimit', 'Speed limit'],
    ['whenDone', 'When done'],
  ];
</script>

<PageHead icon="general" title={t('Behavior')} />
<div class="opt-col">
  <fieldset class="group opt-col">
    <legend>{t('When a download ends')}</legend>
    {#each notify as [value, label] (value)}
      <label class="check"
        ><input
          type="radio"
          name="notify"
          checked={e.notify === value}
          onchange={() => {
            e.notify = value;
            d.doneDialog = value === 'dialog';
          }}
        />{t(label)}</label
      >
    {/each}
  </fieldset>
  <div class="row">
    <label for="exists">{t('If the file exists')}</label>
    <select id="exists" class="select wide" bind:value={e.fileExists}>
      {#each exists as [value, label] (value)}<option {value}>{t(label)}</option>{/each}
    </select>
  </div>
  <div class="row">
    <label for="auto-start">{t('Start these types without asking')}</label>
    <input id="auto-start" class="text grow" bind:value={e.autoStartTypes} placeholder={t('none, or exe, msi')} spellcheck="false" />
  </div>
  <label class="check"><input type="checkbox" bind:checked={e.resumeAtStartup} />{t('Resume unfinished downloads at startup')}</label>
  <label
    class="check"
    title={view.creationTime
      ? t('Windows and macOS keep a creation time that can be set; Linux does not.')
      : t('This system keeps no creation time that can be set; Windows and macOS do.')}
    ><input type="checkbox" bind:checked={e.serverFileDateCreated} disabled={!d.serverFileDate || !view.creationTime} />{t(
      "Use the server's date for the file's creation time too",
    )}</label
  >
  <div class="row">
    <label for="start-tab">{t('Download window')}: {t('Open it on')}</label>
    <select id="start-tab" class="select wide" bind:value={e.progressStartTab}>
      {#each tabs as [value, label] (value)}<option {value}>{t(label)}</option>{/each}
    </select>
  </div>
  <fieldset class="group opt-col">
    <legend>{t('Removing downloads')}</legend>
    <label class="check"><input type="checkbox" bind:checked={e.confirmRemove} />{t('Ask first')}</label>
    <label class="check"><input type="checkbox" bind:checked={e.removeDeletesFiles} />{t('Also send their files to the Recycle Bin')}</label>
  </fieldset>
  <div class="row">
    <label for="size">{t('Interface size')}</label>
    <input id="size" class="text num" type="number" min="80" max="200" step="10" bind:value={e.textSize} />
    <span>%</span>
    <span class="opt-note">{t('Scales everything in the window')}</span>
  </div>
</div>

<style>
  .wide {
    width: 190px;
  }

  .num {
    width: 70px;
  }
</style>
