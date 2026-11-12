<!-- Find file: select the next download whose name, description or
     address matches. -->
<script lang="ts">
  import { findDownload, messageOf } from '../../ipc';
  import type { FindInput } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';

  let { query = $bindable(), onclose }: { query: FindInput; onclose: () => void } = $props();
  let note = $state('');

  async function find() {
    note = '';
    try {
      if (await findDownload($state.snapshot(query))) onclose();
      else note = t('No download matches.');
    } catch (e) {
      note = messageOf(e);
    }
  }
</script>

<Sheet title={t('Find file')} width={350} okDisabled={!query.text.trim()} oncancel={onclose} onok={() => void find()}>
  <label for="find-text">{t('Find:')}</label>
  <!-- svelte-ignore a11y_autofocus -->
  <input id="find-text" class="text" bind:value={query.text} autofocus />
  <label class="check"><input type="checkbox" bind:checked={query.name} />{t('File name or part of the name')}</label>
  <label class="check"><input type="checkbox" bind:checked={query.description} />{t('Description or part of the description')}</label>
  <label class="check"><input type="checkbox" bind:checked={query.sites} />{t('Site name/Download link/Parent web page/Referer')}</label>
  <div class="rule"></div>
  <label class="check"><input type="checkbox" bind:checked={query.matchCase} />{t('Match case')}</label>
  <label class="check"><input type="checkbox" bind:checked={query.whole} />{t('Match whole string only')}</label>
  {#if note}<p role="status">{note}</p>{/if}
</Sheet>

<style>
  .rule {
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }

  p {
    margin: 0;
  }
</style>
