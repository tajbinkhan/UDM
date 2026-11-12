<!-- Confirm deletion of downloads. -->
<script lang="ts">
  import { closeWindow, removeDownloads } from '../../ipc';
  import type { RemoveView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';

  const view: RemoveView | null = $derived(projection.view?.window === 'remove' ? projection.view.view : null);
  let deleteFiles = $state(false);
  let dontAsk = $state(false);
</script>

<Window onescape={() => void closeWindow()}>
  {#if view}
    <div class="dialog">
      <p>{t('Are you sure you want to delete selected downloads from UDM list of downloads?')}</p>
      <label class="check"
        ><input type="checkbox" bind:checked={deleteFiles} disabled={!view.hasFiles} />{t(
          'Delete completely downloaded files from your hard disk as well. Be careful !',
        )}</label
      >
      <label class="check"><input type="checkbox" bind:checked={dontAsk} />{t("Don't show this dialog again")}</label>
      <div class="row buttons">
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn default" autofocus onclick={() => void removeDownloads(deleteFiles, dontAsk)}>{t('Yes')}</button>
        <button class="btn" onclick={() => void closeWindow()}>{t('No')}</button>
      </div>
    </div>
  {/if}
</Window>

<style>
  .dialog {
    gap: 7px;
    padding: 12px 14px;
  }

  p {
    margin: 0 0 10px;
  }

  .buttons {
    justify-content: center;
    gap: 34px;
    margin-top: 10px;
  }

  .buttons .btn {
    width: 90px;
  }
</style>
