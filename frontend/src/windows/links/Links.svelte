<!-- The links of the clipboard or of an imported list: tick them, pick a
     queue, OK. -->
<script lang="ts">
  import { addLinks, closeWindow, messageOf } from '../../ipc';
  import type { LinksView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import ListBox from '../../components/ListBox.svelte';
  import FileGlyph from '../../components/FileGlyph.svelte';

  const view: LinksView | null = $derived(projection.view?.window === 'links' ? projection.view.view : null);
  /** The rows the user unticked; every row starts ticked. */
  let unticked = $state<number[]>([]);
  let pickedQueue = $state<number | null>(null);
  let start = $state(false);
  let picked = $state(-1);
  let error = $state('');

  const ticked = $derived((view?.rows ?? []).map((r) => r.index).filter((i) => !unticked.includes(i)));
  const queue = $derived(pickedQueue ?? view?.queues[0]?.id ?? 1);

  async function ok() {
    error = '';
    try {
      await addLinks(ticked, queue, start);
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<Window onescape={() => void closeWindow()}>
  {#if view}
    <div class="dialog">
      <p>{view.note}</p>
      <div class="list">
        <ListBox
          columns={[
            { title: t('File Name'), width: 150 },
            { title: t('File Type'), width: 80 },
            { title: t('Download from'), width: 260 },
            { title: t('Save to'), width: 140 },
            { title: t('Link Text') },
          ]}
          count={view.rows.length}
          bind:selected={picked}
          fill
          label={view.title}
        >
          {#snippet cell(i, j)}
            {@const r = view.rows[i]}
            {#if j === 0}
              <input
                type="checkbox"
                checked={!unticked.includes(r.index)}
                onchange={(e) => (unticked = e.currentTarget.checked ? unticked.filter((x) => x !== r.index) : [...unticked, r.index])}
                aria-label={r.name}
              /><FileGlyph kind={r.icon} /><span class="name">{r.name}</span>
            {:else if j === 1}{r.kind}{:else if j === 2}{r.url}{:else if j === 3}{r.saveTo}{:else}{r.text}{/if}
          {/snippet}
        </ListBox>
      </div>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <div class="row">
        <button class="btn" onclick={() => (unticked = [])}>{t('Check All')}</button>
        <button class="btn" onclick={() => (unticked = view?.rows.map((r) => r.index) ?? [])}>{t('Uncheck All')}</button>
        <span class="grow"></span>
        <label for="queue">{t('Add to queue')}</label>
        <select id="queue" class="select queue" value={queue} onchange={(e) => (pickedQueue = Number(e.currentTarget.value))}>
          {#each view.queues as q (q.id)}<option value={q.id}>{q.name}</option>{/each}
        </select>
        <label class="check"><input type="checkbox" bind:checked={start} />{t('Start it')}</label>
        <button class="btn default" disabled={ticked.length === 0 || !view.connected} onclick={() => void ok()}>{t('OK')}</button>
        <button class="btn" onclick={() => void closeWindow()}>{t('Cancel')}</button>
      </div>
    </div>
  {/if}
</Window>

<style>
  .dialog {
    gap: 6px;
  }

  p {
    margin: 0;
  }

  .list {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    padding-inline-start: 2px;
  }

  .queue {
    width: 150px;
  }
</style>
