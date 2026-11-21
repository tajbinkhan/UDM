<!-- Options > File types: which downloads UDM takes over, and from where
     it does not. -->
<script lang="ts">
  import type { OptionsView } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import ListBox from '../../components/ListBox.svelte';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  let { view }: { view: OptionsView } = $props();
  // The pages are drawn only while the draft exists.
  const d = $derived(editing.draft!);

  let list = $state<{ offer: boolean; urls: string[] } | null>(null);
  let picked = $state(-1);
  let adding = $state('');
</script>

<PageHead icon="types" title={t('Downloaded file types')} />
<div class="opt-col">
  <label for="types">{t('Automatically start downloading the following file types:')}</label>
  <textarea id="types" class="opt-area" rows="4" bind:value={d.captureTypes} spellcheck="false"></textarea>
  <button class="btn opt-right" onclick={() => (d.captureTypes = view.defaultCaptureTypes)}>{t('Default')}</button>
  <label for="sites">{t("Don't start downloading automatically from the following sites:")}</label>
  <textarea id="sites" class="opt-area" rows="3" bind:value={d.captureExcludedSites} spellcheck="false"></textarea>
  <div class="row">
    <span class="grow">{t('(separate names by spaces)')}</span>
    <button class="btn" onclick={() => (d.captureExcludedSites = view.defaultExcludedSites)}>{t('Default')}</button>
  </div>
  <span>{t("Don't start downloading automatically from the following addresses:")}</span>
  <button
    class="btn edit"
    onclick={() => {
      list = { offer: d.offerExceptions, urls: [...d.captureExcludedUrls] };
      picked = -1;
    }}>{t('Edit list ...')}</button
  >
</div>

{#if list}
  <Sheet
    title={t('The list of address exceptions')}
    width={420}
    oncancel={() => (list = null)}
    onok={() => {
      if (!list) return;
      d.offerExceptions = list.offer;
      d.captureExcludedUrls = list.urls;
      list = null;
    }}
  >
    <label class="check"><input type="checkbox" bind:checked={list.offer} />{t('Show the dialog to add an address to the list of exceptions for a twice-cancelled download')}</label>
    <span>{t("Don't start downloading automatically from the following addresses:")}</span>
    <ListBox columns={[{ title: '' }]} count={list.urls.length} bind:selected={picked} height={160} label={t('Addresses')}>
      {#snippet cell(i)}{list?.urls[i]}{/snippet}
    </ListBox>
    <span class="opt-note">{t('You may use the asterisk wildcard for the address pattern.')}</span>
    <div class="row">
      <input class="text grow" bind:value={adding} placeholder="http://*.example.com/*.zip" aria-label={t('Address')} />
      <button
        type="button"
        class="btn"
        disabled={!adding.trim()}
        onclick={() => {
          if (!list) return;
          list.urls = [...list.urls, adding.trim()];
          adding = '';
        }}>{t('Add')}</button
      >
      <button
        type="button"
        class="btn"
        disabled={picked < 0}
        onclick={() => {
          if (!list) return;
          list.urls = list.urls.filter((_, i) => i !== picked);
          picked = -1;
        }}>{t('Delete')}</button
      >
    </div>
  </Sheet>
{/if}

<style>
  .edit {
    align-self: flex-start;
    width: 134px;
    margin-inline-start: 4px;
  }
</style>
