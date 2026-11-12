<!-- Step 2: where the files are saved. -->
<script lang="ts">
  import { grabAddOnClose, messageOf, pickPath } from '../../ipc';
  import type { GrabberView } from '../../ipc/types';
  import { t } from '../../i18n';
  import { grab } from './grab.svelte';

  let { view }: { view: GrabberView } = $props();
  const d = $derived(grab.draft!);
  let error = $state('');

  async function browse() {
    error = '';
    try {
      const p = await pickPath('folder', d.saveFolder);
      if (p) d.saveFolder = p;
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<div class="col">
  <fieldset class="group col">
    <legend>{t('Save To')}</legend>
    <label class="check"><input type="radio" value="categories" bind:group={d.save} />{t('Every file to a folder according to UDM category of the file')}</label>
    <label class="check"><input type="radio" value="category" bind:group={d.save} />{t('All files to the folder associated with the following UDM category:')}</label>
    <select class="select indent cat" bind:value={d.saveCategory} disabled={d.save !== 'category'} aria-label={t('Category')}>
      {#each view.categories as c (c)}<option value={c}>{t(c)}</option>{/each}
    </select>
    <label class="check"><input type="radio" value="folder" bind:group={d.save} />{t('All files to the following folder')}</label>
    <input class="text indent" bind:value={d.saveFolder} disabled={d.save !== 'folder'} aria-label={t('Folder')} spellcheck="false" />
    <div class="row indent">
      <label class="check grow"><input type="checkbox" bind:checked={d.subfolders} disabled={d.save !== 'folder'} />{t('Use original relative subfolders')}</label>
      <button class="btn" disabled={d.save !== 'folder'} onclick={() => void browse()}>{t('Browse...')}</button>
    </div>
  </fieldset>
  <label class="check"
    ><input type="checkbox" bind:checked={d.offline} disabled={d.save !== 'folder'} />{t('Convert the links in downloaded html files to the local files for offline browsing')}</label
  >
  <label class="check"><input type="checkbox" bind:checked={d.overwrite} />{t('Overwrite existing files')}</label>
  <label class="check"
    ><input type="checkbox" checked={view.addOnClose} onchange={(e) => void grabAddOnClose(e.currentTarget.checked)} />{t(
      'Add checked files to UDM main list and to download queue on closing the grabber',
    )}</label
  >
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</div>

<style>
  .col {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .indent {
    margin-left: 22px;
  }

  .cat {
    width: 220px;
  }

  input[type='radio'] {
    margin: 0 5px 0 0;
    accent-color: var(--focus);
  }

  p {
    margin: 0;
  }
</style>
