<!-- A download's site asks for a user name and password. -->
<script lang="ts">
  import { answerLogin, closeWindow, messageOf } from '../../ipc';
  import type { LoginView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';

  const view: LoginView | null = $derived(projection.view?.window === 'login' ? projection.view.view : null);
  let user = $state('');
  let password = $state('');
  let save = $state(false);
  let error = $state('');
  let busy = $state(false);
  // The user name last given for the site, once.
  let filled = false;
  $effect(() => {
    if (view && !filled) {
      filled = true;
      user = view.user;
    }
  });

  async function ok() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      await answerLogin(user, password, save);
    } catch (e) {
      error = messageOf(e);
    } finally {
      busy = false;
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
      {#if view.refused}
        <p class="error-text" role="alert">{t('{host} refused the login. Check the user name and password.', { host: view.host })}</p>
      {:else}
        <p>{t('{host} asks for a user name and password.', { host: view.host })}</p>
      {/if}
      {#if view.count > 1}<p class="muted">{t('{count} downloads from this site are waiting for it.', { count: view.count })}</p>{/if}
      {#if view.realm}<p class="muted">{t('Realm')}: {view.realm}</p>{/if}
      <div class="row">
        <label class="lead" for="user">{t('User name')}</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="user" class="text grow" bind:value={user} autofocus autocomplete="off" spellcheck="false" />
      </div>
      <div class="row">
        <label class="lead" for="password">{t('Password')}</label>
        <input id="password" class="text grow" type="password" bind:value={password} autocomplete="off" />
      </div>
      <label class="check" title={view.ownSite ? '' : t('Keep it with the site logins in Options, for every download from this site')}
        ><input type="checkbox" bind:checked={save} />{t('Save this password')}</label
      >
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <div class="row buttons">
        <button class="btn default" type="submit" disabled={!user.trim() || busy}>{t('OK')}</button>
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
    width: 80px;
  }

  .buttons .btn {
    width: 80px;
  }
</style>
