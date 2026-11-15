<!-- A gallery page's pictures, read with gallery-dl: tick them, choose the
     folder and the queue. -->
<script lang="ts">
  import { addPicks, closeWindow, messageOf, pickPath } from '../../ipc';
  import type { GalleryView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import ListBox from '../../components/ListBox.svelte';

  const view: GalleryView | null = $derived(projection.view?.window === 'gallery' ? projection.view.view : null);
  /** The rows the user unticked; every picture starts ticked. */
  let unticked = $state<number[]>([]);
  let typedFolder = $state<string | null>(null);
  let pickedQueue = $state<number | null>(null);
  let start = $state(true);
  let picked = $state(-1);
  let error = $state('');
  let busy = $state(false);

  const ticked = $derived((view?.rows ?? []).map((r) => r.index).filter((i) => !unticked.includes(i)));
  const queue = $derived(pickedQueue ?? view?.queues[0]?.id ?? 1);
  const folder = $derived(typedFolder ?? view?.folder ?? '');

  async function browse() {
    error = '';
    try {
      const path = await pickPath('folder', folder);
      if (path) typedFolder = path;
    } catch (e) {
      error = messageOf(e);
    }
  }

  async function ok() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      await addPicks({ indices: ticked, queue, start, package: null, rename: '', quality: 0, folder });
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
      <p class="title">{t('Download gallery')} <strong>{view.title || view.page}</strong></p>
      <div class="list">
        {#if view.reading}
          <p class="state" role="status">{t('Reading the gallery with gallery-dl...')}</p>
        {:else if view.error}
          <p class="state error-text" role="alert">{view.error}</p>
        {:else}
          <ListBox
            columns={[{ title: t('Name'), width: 220 }, { title: t('Size'), width: 100 }, { title: t('Link') }]}
            count={view.rows.length}
            bind:selected={picked}
            fill
            label={t('Download gallery')}
          >
            {#snippet cell(i, j)}
              {@const r = view.rows[i]}
              {#if j === 0}
                <input
                  type="checkbox"
                  checked={!unticked.includes(r.index)}
                  onchange={(e) => (unticked = e.currentTarget.checked ? unticked.filter((x) => x !== r.index) : [...unticked, r.index])}
                  aria-label={r.name}
                /><span class="name">{r.name}</span>
              {:else if j === 1}{r.size}{:else}{r.url}{/if}
            {/snippet}
          </ListBox>
        {/if}
      </div>
      <div class="row">
        <button class="btn" disabled={view.rows.length === 0} onclick={() => (unticked = [])}>{t('Check all')}</button>
        <button class="btn" disabled={view.rows.length === 0} onclick={() => (unticked = view?.rows.map((r) => r.index) ?? [])}
          >{t('Uncheck all')}</button
        >
        <span class="grow"></span>
        <span class="muted">{ticked.length} / {view.rows.length}</span>
      </div>
      <div class="row">
        <label for="folder">{t('Save the files to')}</label>
        <input id="folder" class="text grow" value={folder} oninput={(e) => (typedFolder = e.currentTarget.value)} spellcheck="false" />
        <button class="btn small" onclick={() => void browse()} aria-label={t('Choose the folder to save the files to.')}>...</button>
      </div>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <div class="row">
        <label for="queue">{t('Queue')}</label>
        <select id="queue" class="select queue" value={queue} onchange={(e) => (pickedQueue = Number(e.currentTarget.value))}>
          {#each view.queues as q (q.id)}<option value={q.id}>{q.name}</option>{/each}
        </select>
        <label class="check"><input type="checkbox" bind:checked={start} />{t('Start the queue now')}</label>
        <span class="grow"></span>
        <button class="btn default" disabled={ticked.length === 0 || !folder.trim() || !view.connected || busy} onclick={() => void ok()}
          >{t('OK')}</button
        >
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

  .state {
    padding: 20px;
    text-align: center;
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
