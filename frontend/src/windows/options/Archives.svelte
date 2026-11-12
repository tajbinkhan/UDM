<!-- Options > Archives: unpacking a package once all its files are in, and
     where 7-Zip is. -->
<script lang="ts">
  import { messageOf, pickPath } from '../../ipc';
  import type { PickKind } from '../../ipc/types';
  import { t } from '../../i18n';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  // The pages are drawn only while the draft exists.
  const e = $derived(editing.draft!.extras);
  let error = $state('');

  async function browse(kind: PickKind, start: string, done: (path: string) => void) {
    error = '';
    try {
      const path = await pickPath(kind, start);
      if (path) done(path);
    } catch (err) {
      error = messageOf(err);
    }
  }
</script>

<PageHead icon="archive" title={t('Archives')} />
<div class="opt-col">
  <label class="check"><input type="checkbox" bind:checked={e.unpackAuto} />{t('Unpack a package as soon as all its files are in')}</label>
  <fieldset class="group opt-col">
    <legend>{t('Unpack to')}</legend>
    <label class="check"
      ><input type="radio" name="unpack-to" value="subfolder" bind:group={e.unpackTo} />{t(
        'A folder named after the package, beside the archives',
      )}</label
    >
    <label class="check"><input type="radio" name="unpack-to" value="beside" bind:group={e.unpackTo} />{t('The folder the archives are in')}</label>
    <label class="check"
      ><input type="radio" name="unpack-to" value="folder" bind:group={e.unpackTo} />{t('A folder named after the package, in')}</label
    >
    <div class="row opt-indent">
      <input class="text grow" bind:value={e.unpackFolder} disabled={e.unpackTo !== 'folder'} aria-label={t('The folder to unpack into')} spellcheck="false" />
      <button
        class="btn small"
        disabled={e.unpackTo !== 'folder'}
        aria-label={t('The folder to unpack into')}
        onclick={() => void browse('folder', e.unpackFolder, (p) => (e.unpackFolder = p))}>...</button
      >
    </div>
  </fieldset>
  <label class="check"><input type="checkbox" bind:checked={e.unpackRecycle} />{t('Then send the archives to the Recycle Bin')}</label>
  <p class="opt-note">{t('Zip, tar, gz and xz archives are unpacked by UDM itself; 7z and RAR archives need 7-Zip.')}</p>
  <div class="row">
    <label for="seven-zip">7-Zip</label>
    <input
      id="seven-zip"
      class="text grow"
      bind:value={e.sevenZipPath}
      placeholder={t('Empty looks for it on PATH and where 7-Zip installs')}
      spellcheck="false"
    />
    <button class="btn small" aria-label={t('Where is 7-Zip?')} onclick={() => void browse('program', e.sevenZipPath, (p) => (e.sevenZipPath = p))}
      >...</button
    >
  </div>
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</div>
