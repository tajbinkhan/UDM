<!-- Download complete: where the file went, and opening it. -->
<script lang="ts">
  import { closeWindow, dragFile, hideDoneDialogs, openDownload, openWith } from '../../ipc';
  import type { DoneView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';

  const view: DoneView | null = $derived(projection.view?.window === 'done' ? projection.view.view : null);
  let dontShow = $state(false);

  async function close() {
    if (dontShow) await hideDoneDialogs().catch(() => undefined);
    await closeWindow();
  }

  /** On Windows the folder icon drags the file out, as the list's rows do. */
  function drag(e: MouseEvent) {
    if (!view?.dragOut || e.button !== 0) return;
    const id = view.id;
    const start = { x: e.clientX, y: e.clientY };
    const move = (ev: MouseEvent) => {
      if (Math.abs(ev.clientX - start.x) + Math.abs(ev.clientY - start.y) < 5) return;
      stop();
      if (ev.buttons & 1) void dragFile(id);
    };
    const stop = () => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', stop);
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', stop);
  }

  async function open(how: 'file' | 'folder') {
    if (!view) return;
    await openDownload(view.id, how);
    await close();
  }
</script>

<Window onescape={() => void close()}>
  {#if view}
    <div class="dialog">
      <div class="row head">
        <svg width="34" height="34" viewBox="0 0 34 34" aria-hidden="true" class:grab={view.dragOut} onmousedown={drag}>
          <path d="M2.5 8.5h11l3 3h15v19h-29z" fill="var(--tint-folder)" stroke="var(--fg-muted)" stroke-linejoin="round" />
          <path d="M9 4.5h12l4 4v14H9z" fill="var(--field)" stroke="var(--fg-muted)" stroke-linejoin="round" />
          <path d="M2.5 30.5l3-15h28l-3 15z" fill="var(--tint-folder)" stroke="var(--fg-muted)" stroke-linejoin="round" />
        </svg>
        <div>
          <div class:error-text={view.failed}>{view.headline}</div>
          <div>{view.sizeLine}</div>
        </div>
      </div>
      <label for="address">{t('Address')}</label>
      <input id="address" class="text" value={view.address} readonly />
      <label for="saved">{t('The file saved as')}</label>
      <input id="saved" class="text" value={view.savedAs} readonly />
      <div class="row buttons">
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn default" disabled={!view.canOpen} autofocus onclick={() => void open('file')}>{t('Open')}</button>
        <button class="btn" disabled={!view.canOpenWith} onclick={() => void openWith(view.id)}>{t('Open with...')}</button>
        <button class="btn" disabled={!view.savedAs} onclick={() => void open('folder')}>{t('Open folder')}</button>
        <button class="btn" onclick={() => void close()}>{t('Close')}</button>
      </div>
      <label class="check"><input type="checkbox" bind:checked={dontShow} />{t("Don't show this dialog again")}</label>
    </div>
  {/if}
</Window>

<style>
  .dialog {
    gap: 3px;
    padding: 8px 12px 8px 14px;
  }

  .grab {
    cursor: grab;
  }

  .head {
    gap: 10px;
    margin-bottom: 8px;
  }

  .buttons {
    justify-content: space-between;
    margin: 10px 0 8px;
  }

  .buttons .btn {
    width: 90px;
  }

  .text {
    width: 100%;
    margin-bottom: 6px;
  }
</style>
