<!-- Options > Save to: the categories, their file types and folders, and
     the temporary folder. -->
<script lang="ts">
  import { messageOf, pickPath } from '../../ipc';
  import type { CategoryDraft, PickKind } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  // The pages are drawn only while the draft exists.
  const d = $derived(editing.draft!);

  /** The category shown: -1 for General, else its place in the list. */
  let at = $state(-1);
  let sheet = $state<{ index: number; c: CategoryDraft } | null>(null);
  let error = $state('');

  const name = $derived(at < 0 ? 'General' : (d.categories[at]?.name ?? 'General'));

  async function browse(kind: PickKind, start: string, done: (path: string) => void) {
    error = '';
    try {
      const path = await pickPath(kind, start);
      if (path) done(path);
    } catch (e) {
      error = messageOf(e);
    }
  }

  function edit(index: number) {
    const blank: CategoryDraft = { original: null, name: '', extensions: '', sitesOn: false, sites: '', folder: '', rememberFolder: false };
    sheet = { index, c: index < 0 ? blank : { ...d.categories[index] } };
  }
</script>

<PageHead icon="folder" title={t('Categories, file types, folders')} />
<div class="opt-col">
  <fieldset class="group opt-col">
    <legend>{t('Save To...')}</legend>
    <div class="row">
      <label for="category" class="grow">{t('Category')}</label>
      <button class="btn" onclick={() => edit(-1)}>{t('New')}</button>
    </div>
    <div class="row">
      <select id="category" class="select grow" value={at} onchange={(e) => (at = Number(e.currentTarget.value))}>
        <option value={-1}>{t('General')}</option>
        {#each d.categories as c, i (i)}<option value={i}>{t(c.name)}</option>{/each}
      </select>
      <button class="btn" disabled={at < 0} onclick={() => edit(at)}>{t('Edit...')}</button>
    </div>
    <span>{t('Automatically put in "{category}" category the following file types:', { category: t(name) })}</span>
    <input
      class="text"
      readonly
      value={at < 0 ? t('The file types that are not listed in any other category') : d.categories[at]?.extensions.toUpperCase()}
      aria-label={t('File types')}
    />
    <label for="folder">{t('Default download directory for "{category}" category', { category: t(name) })}</label>
    <div class="row">
      {#if at < 0}
        <input id="folder" class="text grow" bind:value={d.downloadDir} placeholder={t('The Downloads folder')} spellcheck="false" />
        <button class="btn" onclick={() => browse('folder', d.downloadDir, (p) => (d.downloadDir = p))}>{t('Browse')}</button>
      {:else if d.categories[at]}
        <input id="folder" class="text grow" bind:value={d.categories[at].folder} placeholder={t('A folder named after the category')} spellcheck="false" />
        <button class="btn" onclick={() => browse('folder', d.categories[at].folder, (p) => (d.categories[at].folder = p))}>{t('Browse')}</button>
      {/if}
    </div>
    {#if at < 0}
      <label class="check"><input type="checkbox" bind:checked={d.generalRememberFolder} />{t('Change folder for "{category}" category on last selected', { category: t(name) })}</label>
    {:else if d.categories[at]}
      <label class="check"
        ><input type="checkbox" bind:checked={d.categories[at].rememberFolder} />{t('Change folder for "{category}" category on last selected', {
          category: t(name),
        })}</label
      >
    {/if}
  </fieldset>
  <label class="check"><input type="checkbox" bind:checked={d.serverFileDate} />{t('Set file creation date as provided by the server')}</label>
  <fieldset class="group opt-col">
    <legend>{t('Temporary directory')}</legend>
    <div class="row">
      <input class="text grow" bind:value={d.tempDir} placeholder={t("The system's temporary folder")} aria-label={t('Temporary directory')} spellcheck="false" />
      <button class="btn" onclick={() => browse('folder', d.tempDir, (p) => (d.tempDir = p))}>{t('Browse')}</button>
    </div>
    <p class="opt-note">
      {t(
        'UDM keeps the parts of a download here while it runs. A folder on another drive than the "Save To" folders makes assembling the finished files slower; one on the same drive is fastest.',
      )}
    </p>
  </fieldset>
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</div>

{#if sheet}
  <Sheet
    title={sheet.index < 0 ? t('Adding a category to UDM categories list') : t('Category properties')}
    width={460}
    okDisabled={!sheet.c.name.trim()}
    oncancel={() => (sheet = null)}
    onok={() => {
      if (!sheet) return;
      if (sheet.index < 0) {
        d.categories = [...d.categories, sheet.c];
        at = d.categories.length - 1;
      } else d.categories[sheet.index] = sheet.c;
      sheet = null;
    }}
  >
    <label for="cat-name">{t('Category name')}</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="cat-name" class="text" bind:value={sheet.c.name} autofocus />
    <label for="cat-types">{t('Automatically put in this category the following file types:')}</label>
    <input id="cat-types" class="text" bind:value={sheet.c.extensions} spellcheck="false" />
    <span class="opt-note">{t('Note: type file extensions separated by space (e.g. avi mpg mpeg)')}</span>
    <div class="opt-rule"></div>
    <label class="check"><input type="checkbox" bind:checked={sheet.c.sitesOn} />{t('Automatically put in this category the files from the following sites only:')}</label>
    <input class="text" bind:value={sheet.c.sites} disabled={!sheet.c.sitesOn} aria-label={t('Sites')} spellcheck="false" />
    <span class="opt-note">{t('Separate sites by spaces. You may use asterisk as a wildcard pattern')}</span>
    <div class="opt-rule"></div>
    <label for="cat-folder" class="link">{t('Save future downloads of this category to the following folder:')}</label>
    <input id="cat-folder" class="text" bind:value={sheet.c.folder} spellcheck="false" />
    <div class="row">
      <label class="check grow"><input type="checkbox" bind:checked={sheet.c.rememberFolder} />{t('Remember last save path')}</label>
      {#if sheet.index >= 0}
        <button
          type="button"
          class="btn"
          onclick={() => {
            if (!sheet) return;
            const gone = sheet.index;
            d.categories = d.categories.filter((_, i) => i !== gone);
            at = -1;
            sheet = null;
          }}>{t('Delete')}</button
        >
      {/if}
      <button type="button" class="btn" onclick={() => sheet && browse('folder', sheet.c.folder, (p) => sheet && (sheet.c.folder = p))}
        >{t('Browse...')}</button
      >
    </div>
  </Sheet>
{/if}

<style>
  fieldset {
    gap: 5px;
  }

  .link {
    color: var(--link);
  }

  p {
    margin: 0;
  }
</style>
