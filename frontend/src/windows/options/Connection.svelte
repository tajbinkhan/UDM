<!-- Options > Connection: connections per download and per server, and the
     download quota. -->
<script lang="ts">
  import type { OptionsView, ServerLimitDraft } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import ListBox from '../../components/ListBox.svelte';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  let { view }: { view: OptionsView } = $props();
  // The pages are drawn only while the draft exists.
  const d = $derived(editing.draft!);

  let picked = $state(-1);
  let sheet = $state<{ index: number; limit: ServerLimitDraft } | null>(null);
</script>

<PageHead icon="network" title={t('Connections and Limits')} />
<div class="opt-col">
  <fieldset class="group opt-col">
    <legend>{t('Max. connections number')}</legend>
    <div class="row end">
      <label for="conns">{t('Default max. conn. number')}</label>
      <select id="conns" class="select conns" bind:value={d.connections}>
        {#each view.connections as n (n)}<option value={n}>{n}</option>{/each}
      </select>
    </div>
    <div class="opt-rule"></div>
    <span>{t('Exceptions:')}</span>
    <div class="row top">
      <div class="grow">
        <ListBox
          columns={[{ title: t('Server') }, { title: t('Number'), width: 60 }]}
          count={d.serverLimits.length}
          bind:selected={picked}
          height={118}
          label={t('Exceptions:')}
          onopen={(i) => (sheet = { index: i, limit: { ...d.serverLimits[i] } })}
        >
          {#snippet cell(i, j)}{j === 0 ? d.serverLimits[i]?.site : d.serverLimits[i]?.max}{/snippet}
        </ListBox>
      </div>
      <div class="opt-col">
        <button class="btn" onclick={() => (sheet = { index: -1, limit: { site: '', max: 1 } })}>{t('New')}</button>
        <button
          class="btn"
          disabled={picked < 0}
          onclick={() => {
            d.serverLimits = d.serverLimits.filter((_, i) => i !== picked);
            picked = -1;
          }}>{t('Delete')}</button
        >
        <button class="btn" disabled={picked < 0} onclick={() => (sheet = { index: picked, limit: { ...d.serverLimits[picked] } })}>{t('Edit')}</button>
      </div>
    </div>
  </fieldset>
  <fieldset class="group opt-col">
    <legend><label class="check"><input type="checkbox" bind:checked={d.quotaEnabled} />{t('Download limits')}</label></legend>
    <div class="quota" class:disabled={!d.quotaEnabled}>
      <label for="mb">{t('Download no more than')}</label>
      <input id="mb" class="text num" type="number" min="1" bind:value={d.quotaMb} disabled={!d.quotaEnabled} />
      <span>{t('MBytes')}</span>
      <label for="hours">{t('every')}</label>
      <input id="hours" class="text num" type="number" min="1" max="168" bind:value={d.quotaHours} disabled={!d.quotaEnabled} />
      <span>{t('hours')}</span>
    </div>
    <label class="check opt-indent"><input type="checkbox" bind:checked={d.quotaWarn} disabled={!d.quotaEnabled} />{t('Show warning before stopping downloads')}</label>
  </fieldset>
</div>

{#if sheet}
  <Sheet
    title={t('Max. connections number for a server')}
    width={330}
    okDisabled={!sheet.limit.site.trim()}
    oncancel={() => (sheet = null)}
    onok={() => {
      if (!sheet) return;
      if (sheet.index < 0) d.serverLimits = [...d.serverLimits, sheet.limit];
      else d.serverLimits[sheet.index] = sheet.limit;
      sheet = null;
    }}
  >
    <label for="server">{t('Server')}</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="server" class="text" bind:value={sheet.limit.site} autofocus placeholder="files.example.com" spellcheck="false" />
    <span class="opt-note">{t('You may use asterisk as a wildcard pattern')}</span>
    <div class="row end">
      <label for="max">{t('Max. connections number')}</label>
      <select id="max" class="select conns" bind:value={sheet.limit.max}>
        {#each view.serverConnections as n (n)}<option value={n}>{n}</option>{/each}
      </select>
    </div>
  </Sheet>
{/if}

<style>
  .end {
    justify-content: flex-end;
  }

  .top {
    align-items: flex-start;
  }

  .conns {
    width: 100px;
  }

  .quota {
    display: grid;
    grid-template-columns: auto 64px auto;
    align-items: center;
    justify-content: start;
    gap: 4px 6px;
    padding-left: 18px;
  }

  .quota label {
    text-align: right;
  }

  .num {
    text-align: right;
  }
</style>
