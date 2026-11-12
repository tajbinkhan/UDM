<!-- The countdown before the computer sleeps, hibernates or shuts down when
     a queue or a download is done. -->
<script lang="ts">
  import { answerCountdown, messageOf } from '../../ipc';
  import type { CountdownView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';

  const view: CountdownView | null = $derived(projection.view?.window === 'countdown' ? projection.view.view : null);
  let error = $state('');

  async function answer(now: boolean) {
    error = '';
    try {
      await answerCountdown(now);
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<Window onescape={() => void answer(false)}>
  {#if view}
    <div class="dialog" role="alertdialog" aria-label={view.title}>
      <p>{view.done}</p>
      <p class="message" aria-live="polite">{view.message}</p>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <div class="row buttons">
        <button class="btn" onclick={() => void answer(true)}>{view.now}</button>
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn default" autofocus onclick={() => void answer(false)}>{t('Cancel')}</button>
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
    overflow-wrap: anywhere;
  }

  .message {
    font-weight: 600;
  }

  .buttons .btn {
    min-width: 90px;
  }
</style>
