<!-- Step 4: which files to keep. -->
<script lang="ts">
  import { grabSaveFilters, messageOf } from '../../ipc';
  import type { GrabberView } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import { grab } from './grab.svelte';

  let { view, advanced }: { view: GrabberView; advanced: boolean } = $props();
  const d = $derived(grab.draft!);
  let adding = $state<{ name: string; mask: string; skip: boolean } | null>(null);
  let error = $state('');

  async function addFilter() {
    if (!adding) return;
    error = '';
    try {
      await grabSaveFilters([...view.filters.map((f) => ({ name: f.name, mask: f.mask })), { name: adding.name, mask: adding.mask }]);
      const types = adding.mask;
      if (adding.skip) d.skipTypes = types;
      else d.types = types;
      adding = null;
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<div class="col">
  <label for="types">{t('Download the following files (file types)')}</label>
  <div class="row">
    <input id="types" class="text grow" bind:value={d.types} list="filters" spellcheck="false" />
    <datalist id="filters">
      {#each view.filters as f (f.name)}<option value={f.types}>{f.name} ({f.mask})</option>{/each}
    </datalist>
    <button class="btn" disabled={!view.library} onclick={() => (adding = { name: '', mask: d.types, skip: false })}>{t('Add Filter')}</button>
  </div>
  <label for="skip">{t("Don't download the following files (file types)")}</label>
  <div class="row">
    <input id="skip" class="text grow" bind:value={d.skipTypes} list="filters" placeholder={t("Don't use exclude filter")} spellcheck="false" />
    <button class="btn" disabled={!view.library} onclick={() => (adding = { name: '', mask: d.skipTypes, skip: true })}>{t('Add Filter')}</button>
  </div>
  <label class="check"><input type="checkbox" bind:checked={d.filesThisSite} />{t('Search files on this site only')}</label>
  <label class="check"><input type="checkbox" bind:checked={d.hideDuplicates} />{t('Hide duplicate files found in different locations')}</label>
  <label class="check"><input type="checkbox" bind:checked={d.downloadFound} />{t('Start downloading all matched files at once')}</label>
  <p>{t('You can explore the site at first, check the files that you need, and download them in the main Grabber window, or add them to the main list of UDM. This option downloads all explored files immediately.')}</p>
  {#if advanced}
    <div class="rule"></div>
    <div class="adv">
      <fieldset class="group sizes">
        <legend>{t('Download if file size is')}</legend>
        <label class="check"><input type="checkbox" bind:checked={d.minOn} />{t('Not less than')}</label>
        <div class="row">
          <input class="text num" type="number" min="0" bind:value={d.minSize} disabled={!d.minOn} aria-label={t('Not less than')} />
          <select class="select unit" bind:value={d.minUnit} disabled={!d.minOn} aria-label={t('Unit')}><option value="kb">KB</option><option value="mb">MB</option></select>
        </div>
        <label class="check"><input type="checkbox" bind:checked={d.maxOn} />{t('Not more than')}</label>
        <div class="row">
          <input class="text num" type="number" min="0" bind:value={d.maxSize} disabled={!d.maxOn} aria-label={t('Not more than')} />
          <select class="select unit" bind:value={d.maxUnit} disabled={!d.maxOn} aria-label={t('Unit')}><option value="kb">KB</option><option value="mb">MB</option></select>
        </div>
      </fieldset>
      <div class="col grow">
        <label class="check"><input type="checkbox" bind:checked={d.filesOnlyOn} />{t('Download the files located within the following paths/domains only:')}</label>
        <textarea class="opt-area" rows="2" bind:value={d.filesOnly} disabled={!d.filesOnlyOn} aria-label={t('Files only')} spellcheck="false"></textarea>
        <label class="check"><input type="checkbox" bind:checked={d.filesSkipOn} />{t("Don't download the files located within the following paths/domains:")}</label>
        <textarea class="opt-area" rows="2" bind:value={d.filesSkip} disabled={!d.filesSkipOn} aria-label={t("Don't download")} spellcheck="false"></textarea>
        <span>{t('Use semicolons ( ; ) to separate entries.')}</span>
      </div>
    </div>
  {/if}
</div>

{#if adding}
  <Sheet title={t('Add Filter')} width={360} okDisabled={!adding.name.trim() || !adding.mask.trim()} oncancel={() => (adding = null)} onok={() => void addFilter()}>
    <label for="filter-name">{t('Filter name')}</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="filter-name" class="text" bind:value={adding.name} autofocus />
    <label for="filter-mask">{t('File types')}</label>
    <input id="filter-mask" class="text" bind:value={adding.mask} placeholder="*.pdf;*.zip" spellcheck="false" />
    {#if error}<p class="error-text" role="alert">{error}</p>{/if}
  </Sheet>
{/if}

<style>
  .col {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .rule {
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }

  .adv {
    display: flex;
    gap: 12px;
  }

  .sizes {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 150px;
  }

  .num {
    width: 60px;
    text-align: end;
  }

  .unit {
    width: 60px;
  }

  textarea:disabled {
    background: var(--face);
    border-color: var(--border);
  }

  p {
    margin: 0;
  }
</style>
