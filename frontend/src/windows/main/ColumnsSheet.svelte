<!-- Columns: which of the list's columns show, in what order, how wide. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { setColumns } from '../../ipc';
  import type { ColumnChoice } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import ListBox from '../../components/ListBox.svelte';

  let { start, defaults, onclose }: { start: ColumnChoice[]; defaults: ColumnChoice[]; onclose: () => void } = $props();
  // The sheet edits a copy of what it opened with.
  let list = $state(untrack(() => start.map((c) => ({ ...c }))));
  let picked = $state(0);
  const c = $derived(list[picked]);

  function move(step: number) {
    const to = picked + step;
    if (to < 0 || to >= list.length) return;
    const next = [...list];
    [next[picked], next[to]] = [next[to], next[picked]];
    list = next;
    picked = to;
  }
</script>

<Sheet
  title={t('Columns')}
  width={350}
  oncancel={onclose}
  onok={() => {
    void setColumns($state.snapshot(list));
    onclose();
  }}
>
  <p>{t('Check the columns that you would like visible in this list. Use the Move Up and Move Down buttons to reorder the columns however you like.')}</p>
  <div class="row top">
    <div class="grow">
      <ListBox columns={[{ title: '' }]} count={list.length} bind:selected={picked} height={140} label={t('Columns')}>
        {#snippet cell(i)}
          <input type="checkbox" bind:checked={list[i].shown} disabled={list[i].key === 'name'} aria-label={list[i].title} />{list[i].title}
        {/snippet}
      </ListBox>
    </div>
    <div class="side">
      <button type="button" class="btn" disabled={picked <= 0} onclick={() => move(-1)}>{t('Move Up')}</button>
      <button type="button" class="btn" disabled={picked >= list.length - 1} onclick={() => move(1)}>{t('Move Down')}</button>
      <button type="button" class="btn" disabled={!c || c.shown} onclick={() => c && (c.shown = true)}>{t('Show')}</button>
      <button type="button" class="btn" disabled={!c || !c.shown || c.key === 'name'} onclick={() => c && (c.shown = false)}>{t('Hide')}</button>
      <button type="button" class="btn" onclick={() => ((list = defaults.map((d) => ({ ...d }))), (picked = 0))}>{t('Reset')}</button>
    </div>
  </div>
  {#if c}
    <div class="row width">
      <label for="width">{t('The selected column should be')}</label>
      <input id="width" class="text num" type="number" min="24" max="800" bind:value={c.width} />
      <span>{t('pixels wide')}</span>
    </div>
  {/if}
</Sheet>

<style>
  p {
    margin: 0;
  }

  .top {
    align-items: flex-start;
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 104px;
  }

  .width {
    justify-content: center;
  }

  .num {
    width: 50px;
    text-align: right;
  }
</style>
