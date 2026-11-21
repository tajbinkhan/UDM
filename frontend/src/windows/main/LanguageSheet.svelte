<!-- The first start's language picker: which language the window speaks,
     starting on the system's. View > Language changes it later. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { setLanguage } from '../../ipc';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import type { Choice } from '../../ipc';

  let { languages, now, onclose }: { languages: Choice[]; now: string; onclose: () => void } = $props();
  let chosen = $state(untrack(() => now));

  // Closing without a choice keeps the system's language, and stops asking.
  function answer(code: string) {
    void setLanguage(code);
    onclose();
  }
</script>

<Sheet title={t('Language')} width={340} oncancel={() => answer('')} onok={() => answer(chosen)}>
  <label for="language-list">{t('Which language should UDM use? You can change it later in View > Language.')}</label>
  <select id="language-list" class="select" size="12" bind:value={chosen} ondblclick={() => answer(chosen)}>
    {#each languages.filter((c) => c.value !== '') as c (c.value)}
      <option value={c.value} lang={c.value}>{c.label}</option>
    {/each}
  </select>
</Sheet>

<style>
  label {
    display: block;
    margin-block-end: 8px;
    line-height: 1.4;
  }

  select {
    inline-size: 100%;
  }
</style>
