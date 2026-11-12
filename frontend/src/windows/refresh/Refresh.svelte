<!-- New link: a fresh address for a download whose link expired, typed or
     taken from the next download the browser starts. -->
<script lang="ts">
  import { cancelRefresh, closeWindow, messageOf, refreshLink } from '../../ipc';
  import type { RefreshView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';

  const view: RefreshView | null = $derived(projection.view?.window === 'refresh' ? projection.view.view : null);
  let url = $state('');
  let error = $state('');

  async function run(p: () => Promise<unknown>) {
    error = '';
    try {
      await p();
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<Window onescape={() => void closeWindow()}>
  {#if view}
    <div class="dialog">
      <p>{t('A fresh link for {name}', { name: view.name })}</p>
      <p class="muted">{t('The part already downloaded is kept if the new link is for the same file.')}</p>
      {#if view.mismatch}
        <p class="error-text" role="alert">{view.mismatch.reason}</p>
        <p>{t('Start over from the beginning with the new link?')}</p>
        <div class="row buttons">
          <button
            class="btn default"
            disabled={!view.connected}
            onclick={() => view?.mismatch && void run(() => refreshLink(view!.id, view!.mismatch!.url, true))}>{t('Start over')}</button
          >
          <button class="btn" onclick={() => void closeWindow()}>{t('Cancel')}</button>
        </div>
      {:else}
        <form
          class="row"
          onsubmit={(e) => {
            e.preventDefault();
            if (view && url.trim()) void run(() => refreshLink(view!.id, url, false));
          }}
        >
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="text grow"
            bind:value={url}
            placeholder={t('Paste the new address')}
            aria-label={t('Paste the new address')}
            autofocus
            spellcheck="false"
            autocomplete="off"
          />
          <button class="btn default" type="submit" disabled={!view.connected || !url.trim()}>{t('Use this link')}</button>
        </form>
        {#if view.waiting}
          <div class="row">
            <span class="grow" role="status">{t('Waiting for your next download in the browser...')}</span>
            <button class="btn" onclick={() => void run(cancelRefresh)}>{t('Stop waiting')}</button>
          </div>
        {:else}
          <div class="row">
            <button
              class="btn"
              disabled={!view.connected}
              title={t('Start the download again on its web page; UDM takes the new link from it')}
              onclick={() => view && void run(() => refreshLink(view!.id, null, false))}>{t('Use my next download in the browser')}</button
            >
          </div>
        {/if}
        {#if error}<p class="error-text" role="alert">{error}</p>{/if}
        <div class="row buttons">
          <button class="btn" onclick={() => void closeWindow()}>{t('Close')}</button>
        </div>
      {/if}
    </div>
  {/if}
</Window>

<style>
  .dialog {
    gap: 6px;
    padding: 10px 12px;
  }

  p {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .buttons {
    justify-content: flex-end;
  }
</style>
