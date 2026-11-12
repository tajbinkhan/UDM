<!-- A category's properties, or a new one: its name, file types, sites and
     folder. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { messageOf, pickPath, saveCategory } from '../../ipc';
  import type { CategoryInput } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';

  let { original, start, onclose }: { original: string | null; start: CategoryInput; onclose: () => void } = $props();
  // The sheet edits a copy of what it opened with.
  let c = $state(untrack(() => ({ ...start })));
  let error = $state('');

  async function run(p: Promise<unknown>, close: boolean) {
    error = '';
    try {
      await p;
      if (close) onclose();
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<Sheet
  title={original ? t('Editing category properties') : t('Adding a category to UDM categories list')}
  width={470}
  okDisabled={!c.name.trim()}
  oncancel={onclose}
  onok={() => void run(saveCategory(original, $state.snapshot(c)), true)}
>
  <label for="cat-name">{t('Category name')}</label>
  <!-- svelte-ignore a11y_autofocus -->
  <input id="cat-name" class="text" bind:value={c.name} autofocus />
  <label for="cat-types">{t('Automatically put in this category the following file types:')}</label>
  <input id="cat-types" class="text" bind:value={c.extensions} spellcheck="false" />
  <span class="muted">{t('Note: type file extensions separated by space (e.g. avi mpg mpeg)')}</span>
  <div class="rule"></div>
  <label class="check"><input type="checkbox" bind:checked={c.sitesOn} />{t('Automatically put in this category the files from the following sites only:')}</label>
  <input class="text" bind:value={c.sites} disabled={!c.sitesOn} aria-label={t('Sites')} spellcheck="false" />
  <span class="muted">{t('Separate sites by spaces. You may use asterisk as a wildcard pattern')}</span>
  <div class="rule"></div>
  <label for="cat-folder" class="link">{t('Save future downloads of this category to the following folder:')}</label>
  <input id="cat-folder" class="text" bind:value={c.folder} spellcheck="false" />
  <div class="row">
    <label class="check grow"><input type="checkbox" bind:checked={c.rememberFolder} />{t('Remember last save path')}</label>
    <button
      type="button"
      class="btn"
      onclick={() =>
        void run(
          pickPath('folder', c.folder).then((p) => p && (c.folder = p)),
          false,
        )}>{t('Browse...')}</button
    >
  </div>
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</Sheet>

<style>
  .rule {
    border-top: 1px solid var(--border);
    margin: 2px 0;
  }

  .link {
    color: var(--link);
  }

  p {
    margin: 0;
  }
</style>
