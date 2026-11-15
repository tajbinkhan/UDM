<!-- File Properties: a download's file, address, description, page,
     referrer and login. Move picks a new name and folder. -->
<script lang="ts">
  import { closeWindow, messageOf, moveDownload, openDownload, saveProperties } from '../../ipc';
  import type { PropertiesView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import FileGlyph from '../../components/FileGlyph.svelte';

  const view: PropertiesView | null = $derived(projection.view?.window === 'properties' ? projection.view.view : null);
  let draft = $state({ address: '', description: '', referrer: '', loginUser: '', loginPassword: '' });
  let loaded = $state(false);
  let error = $state('');

  $effect(() => {
    if (view && !loaded) {
      draft = { address: view.address, description: view.description, referrer: view.referrer, loginUser: view.loginUser, loginPassword: '' };
      loaded = true;
    }
  });

  async function run(p: Promise<unknown>) {
    error = '';
    try {
      await p;
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
        void run(saveProperties($state.snapshot(draft)));
      }}
    >
      <div class="row head"><FileGlyph kind={view.icon} size={32} /><span>{view.name}</span></div>
      <div class="rule"></div>
      <div class="facts">
        <span>{t('Type:')}</span><span>{view.kind}</span>
        <span>{t('Status:')}</span><span>{view.status}</span>
        <span>{t('Size:')}</span><span>{view.size}</span>
      </div>
      <div class="grid">
        <label for="save-to">{t('Save To:')}</label>
        <div class="row">
          <input id="save-to" class="text grow" readonly value={view.saveTo} />
          <button type="button" class="btn" disabled={!view.canMove} onclick={() => void run(moveDownload(view.id, view.name))}>{t('Move')}</button>
        </div>
        <label for="address">{t('Address:')}</label>
        <input id="address" class="text" bind:value={draft.address} readonly={!view.editable} spellcheck="false" />
        <label for="description">{t('Description:')}</label>
        <input id="description" class="text" bind:value={draft.description} />
      </div>
      <div class="rule"></div>
      <span>{t('The web page from which this file was obtained:')}</span>
      <span class="page" title={view.page}>{view.page}</span>
      <div class="rule"></div>
      <div class="grid">
        <label for="referrer">{t('Referer:')}</label>
        <input id="referrer" class="text" bind:value={draft.referrer} spellcheck="false" />
      </div>
      <div class="login">
        <label for="login-user">{t('Login')}</label>
        <input id="login-user" class="text" bind:value={draft.loginUser} disabled={!view.editable} autocomplete="off" />
        <label for="login-pass">{t('Password')}</label>
        <input id="login-pass" class="text" type="password" bind:value={draft.loginPassword} disabled={!view.editable} autocomplete="off" />
      </div>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <div class="rule"></div>
      <div class="buttons">
        <button type="button" class="btn" disabled={!view.canOpen} onclick={() => void run(openDownload(view.id, 'file'))}>{t('Open')}</button>
        <button type="submit" class="btn default">{t('OK')}</button>
      </div>
    </form>
  {/if}
</Window>

<style>
  .dialog {
    gap: 6px;
  }

  .head {
    gap: 22px;
    padding-inline-start: 10px;
  }

  .rule {
    border-top: 1px solid var(--border);
  }

  .facts {
    display: grid;
    grid-template-columns: 80px 1fr;
    gap: 2px 0;
  }

  .grid {
    display: grid;
    grid-template-columns: 80px 1fr;
    align-items: center;
    gap: 6px 0;
  }

  .page {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .login {
    display: grid;
    grid-template-columns: 110px 190px;
    justify-content: start;
    align-items: center;
    gap: 6px 10px;
  }

  .login label {
    text-align: end;
  }

  .buttons .btn {
    width: 74px;
  }

  p {
    margin: 0;
  }
</style>
