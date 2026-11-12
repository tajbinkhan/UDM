<!-- After a captured download's save-as dialog is canceled twice: keep its
     site, or the address, in the browser from now on. -->
<script lang="ts">
  import { answerException, messageOf } from '../../ipc';
  import type { ExceptionChoice, ExceptionView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';

  const view: ExceptionView | null = $derived(projection.view?.window === 'exception' ? projection.view.view : null);
  let dontShow = $state(false);
  let error = $state('');

  async function answer(choice: ExceptionChoice) {
    error = '';
    try {
      await answerException(choice, dontShow);
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<Window onescape={() => void answer('keep')}>
  {#if view}
    <div class="dialog">
      <p>{t('You canceled this download twice. UDM can leave it to the browser next time.')}</p>
      <p class="muted url" title={view.url}>{view.url}</p>
      <label class="check"><input type="checkbox" bind:checked={dontShow} />{t("Don't show this again")}</label>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <div class="row buttons">
        {#if view.host}
          <button class="btn" disabled={!view.connected} onclick={() => void answer('site')}>{t("Don't capture {host}", { host: view.host })}</button>
        {/if}
        <button class="btn" disabled={!view.connected} onclick={() => void answer('address')}>{t("Don't capture this address")}</button>
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn default" autofocus onclick={() => void answer('keep')}>{t('Keep capturing')}</button>
      </div>
    </div>
  {/if}
</Window>

<style>
  .dialog {
    gap: 8px;
    padding: 12px 14px;
  }

  p {
    margin: 0;
  }

  .url {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .buttons {
    justify-content: flex-end;
    flex-wrap: wrap;
  }
</style>
