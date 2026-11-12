<!-- A package's archives are encrypted: the password that opens them. -->
<script lang="ts">
  import { closeWindow, messageOf, unpackPackage } from '../../ipc';
  import type { PasswordView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';

  const view: PasswordView | null = $derived(projection.view?.window === 'password' ? projection.view.view : null);
  let password = $state('');
  let show = $state(false);
  let error = $state('');

  async function ok() {
    if (!view || !password) return;
    error = '';
    try {
      await unpackPackage(view.id, password);
      password = '';
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<Window onescape={() => void closeWindow()}>
  {#if view}
    <form
      class="dialog"
      onsubmit={(e) => {
        e.preventDefault();
        void ok();
      }}
    >
      <p>{t('Package')}: <strong>{view.name}</strong></p>
      {#if view.wrong}<p class="error-text" role="alert">{t('That password did not open them. Try again.')}</p>{/if}
      <div class="row">
        <label class="lead" for="password">{t('Archive password')}</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="password" class="text grow" type={show ? 'text' : 'password'} bind:value={password} autofocus autocomplete="off" />
      </div>
      <label class="check"><input type="checkbox" bind:checked={show} />{t('Show the password')}</label>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <div class="row buttons">
        <button class="btn default" type="submit" disabled={!password}>{t('Unpack')}</button>
        <button class="btn" type="button" onclick={() => void closeWindow()}>{t('Cancel')}</button>
      </div>
    </form>
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

  .lead {
    width: 110px;
  }

  .buttons .btn {
    width: 80px;
  }
</style>
