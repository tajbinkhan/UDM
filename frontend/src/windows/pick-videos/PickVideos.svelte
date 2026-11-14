<!-- A playlist's or channel's videos: tick them, choose the quality and the
     queue. -->
<script lang="ts">
  import { addPicks, closeWindow, messageOf } from '../../ipc';
  import type { PickVideosView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import ListBox from '../../components/ListBox.svelte';

  const view: PickVideosView | null = $derived(projection.view?.window === 'pickVideos' ? projection.view.view : null);
  /** The rows the user unticked; every video starts ticked. */
  let unticked = $state<number[]>([]);
  let quality = $state(0);
  let pickedQueue = $state<number | null>(null);
  let start = $state(true);
  let picked = $state(-1);
  let error = $state('');
  let busy = $state(false);

  const ticked = $derived((view?.rows ?? []).map((r) => r.index).filter((i) => !unticked.includes(i)));
  const queue = $derived(pickedQueue ?? view?.queues[0]?.id ?? 1);

  async function ok() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      await addPicks({ indices: ticked, queue, start, package: null, rename: '', quality, folder: '' });
    } catch (e) {
      error = messageOf(e);
    } finally {
      busy = false;
    }
  }
</script>

<Window onescape={() => void closeWindow()}>
  {#if view}
    <div class="dialog">
      <p class="title">{t('Download videos from the playlist')} <strong>{view.title}</strong></p>
      <div class="list">
        <ListBox
          columns={[{ title: t('Title'), width: 330 }, { title: t('Length'), width: 70 }, { title: t('Link') }]}
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
                aria-label={r.title}
              /><span class="name">{r.title}</span>
            {:else if j === 1}{r.duration}{:else}{r.url}{/if}
          {/snippet}
        </ListBox>
      </div>
      <div class="row">
        <button class="btn" onclick={() => (unticked = [])}>{t('Check all')}</button>
        <button class="btn" onclick={() => (unticked = view?.rows.map((r) => r.index) ?? [])}>{t('Uncheck all')}</button>
        <span class="grow"></span>
        <label for="quality">{t('Quality')}</label>
        <select id="quality" class="select quality" bind:value={quality}>
          {#each view.qualities as q, i (i)}<option value={i}>{q}</option>{/each}
        </select>
      </div>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <div class="row">
        <label for="queue">{t('Queue')}</label>
        <select id="queue" class="select queue" value={queue} onchange={(e) => (pickedQueue = Number(e.currentTarget.value))}>
          {#each view.queues as q (q.id)}<option value={q.id}>{q.name}</option>{/each}
        </select>
        <label class="check"><input type="checkbox" bind:checked={start} />{t('Start the queue now')}</label>
        <span class="grow"></span>
        <button class="btn default" disabled={ticked.length === 0 || !view.connected || busy} onclick={() => void ok()}>{t('OK')}</button>
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

  .title {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
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

  .quality,
  .queue {
    width: 150px;
  }
</style>
