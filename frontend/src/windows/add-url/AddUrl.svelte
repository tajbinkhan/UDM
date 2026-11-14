<!-- Add URL ("Enter new address to download"): an address, and a login for
     its server. -->
<script lang="ts">
  import { addDownload, closeWindow, messageOf } from '../../ipc';
  import type { AddUrlView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';

  let address = $state('');
  const view: AddUrlView | null = $derived(projection.view?.window === 'addUrl' ? projection.view.view : null);
  // A link the clipboard monitor caught fills the field, each new one once.
  let offered: string | null = null;
  $effect(() => {
    const caught = view?.address ?? null;
    if (caught && caught !== offered) {
      offered = caught;
      address = caught;
    }
  });
  let useLogin = $state(false);
  let user = $state('');
  let password = $state('');
  let error = $state('');
  let busy = $state(false);

  async function ok() {
    if (!address.trim() || busy) return;
    busy = true;
    error = '';
    try {
      await addDownload(address, useLogin ? { user, password } : null);
    } catch (e) {
      error = messageOf(e);
    } finally {
      busy = false;
    }
  }
</script>

<Window onescape={() => void closeWindow()}>
  <form
    class="dialog"
    onsubmit={(e) => {
      e.preventDefault();
      void ok();
    }}
  >
    <div class="row">
      <label class="lead" for="address">{t('Address')}</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="address" class="text grow" bind:value={address} autofocus spellcheck="false" autocomplete="off" />
      <button class="btn default" type="submit" disabled={!address.trim() || busy}>{t('OK')}</button>
    </div>
    <div class="row top">
      <fieldset class="group grow">
        <legend><label class="check"><input type="checkbox" bind:checked={useLogin} />{t('Use authorization')}</label></legend>
        <div class="row">
          <label class="login" for="user" class:disabled={!useLogin}>{t('Login')}</label>
          <input id="user" class="text cred" bind:value={user} disabled={!useLogin} autocomplete="off" />
          <label class="login" for="password" class:disabled={!useLogin}>{t('Password')}</label>
          <input id="password" class="text cred" type="password" bind:value={password} disabled={!useLogin} autocomplete="off" />
        </div>
      </fieldset>
      <button class="btn" type="button" onclick={() => void closeWindow()}>{t('Cancel')}</button>
    </div>
    {#if error}<p class="error-text" role="alert">{error}</p>{/if}
  </form>
</Window>

<style>
  .dialog {
    gap: 4px;
    padding: 8px 10px;
  }

  .lead {
    width: 52px;
    padding-inline-start: 4px;
  }

  .top {
    align-items: flex-start;
  }

  fieldset {
    margin-top: -4px;
    padding: 0 8px 8px;
  }

  .login {
    margin-inline-start: auto;
    text-align: end;
  }

  .cred {
    width: 115px;
  }

  p {
    margin: 0;
  }
</style>
