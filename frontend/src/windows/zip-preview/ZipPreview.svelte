<!-- The files inside a zip download, read from the end of the archive
     before the download starts. -->
<script lang="ts">
  import { closeWindow } from '../../ipc';
  import type { ZipView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import ListBox from '../../components/ListBox.svelte';

  const view: ZipView | null = $derived(projection.view?.window === 'zipPreview' ? projection.view.view : null);
  let picked = $state(-1);
</script>

<Window onescape={() => void closeWindow()}>
  {#if view}
    <div class="dialog">
      <p class="title">{view.title}</p>
      {#if view.reading}
        <p class="muted" role="status">{t('List the files inside, reading only the end of the archive')}…</p>
      {:else if view.error}
        <p class="error-text" role="alert">{view.error}</p>
      {:else}
        <p>{view.summary}</p>
        {#if view.note}<p class="muted">{view.note}</p>{/if}
        <div class="list">
          <ListBox
            columns={[{ title: t('Name') }, { title: t('Size'), width: 90 }]}
            count={view.rows.length}
            bind:selected={picked}
            fill
            label={view.title}
          >
            {#snippet cell(i, j)}{j === 0 ? view.rows[i].name : view.rows[i].size}{/snippet}
          </ListBox>
        </div>
      {/if}
      <div class="row buttons">
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn default" autofocus onclick={() => void closeWindow()}>{t('Close')}</button>
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
    overflow-wrap: anywhere;
  }

  .title {
    font-weight: 600;
  }

  .list {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .buttons {
    justify-content: flex-end;
  }
</style>
