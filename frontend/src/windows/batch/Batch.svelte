<!-- Batch download: an address with a `*`, replaced by numbers or letters,
     added to the main queue. -->
<script lang="ts">
  import { addBatch, closeWindow, messageOf, previewBatch } from '../../ipc';
  import type { BatchPreview } from '../../ipc/types';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';

  let address = $state('');
  let letters = $state(false);
  let from = $state('0');
  let to = $state('10');
  let size = $state(1);
  let useLogin = $state(false);
  let user = $state('');
  let password = $state('');
  let preview = $state<BatchPreview | null>(null);
  let error = $state('');
  let busy = $state(false);

  const input = $derived({ address, letters, from: String(from), to: String(to), size: Number(size) || 1 });

  // The shell expands the pattern; the window only shows what it says.
  $effect(() => {
    const now = input;
    const timer = setTimeout(async () => {
      if (!now.address.trim()) {
        preview = null;
        error = '';
        return;
      }
      try {
        preview = await previewBatch(now);
        error = '';
      } catch (e) {
        preview = null;
        error = messageOf(e);
      }
    }, 200);
    return () => clearTimeout(timer);
  });

  async function ok() {
    if (busy) return;
    busy = true;
    try {
      await addBatch(input, useLogin ? { user, password } : null);
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
    <p>
      {t('It is possible to add a group of sequential file names like img001.jpg, img002.jpg, etc., img100.jpg to UDM download queue. Use the asterisk wildcard for the file name pattern.')}
      {t('For example:')} https://example.com/pictures/img*.jpg
    </p>
    <div class="row">
      <label for="address" class="lead">{t('Address')}</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="address" class="text grow" bind:value={address} spellcheck="false" autofocus />
    </div>
    <fieldset class="group">
      <legend>{t('Replace asterisk to')}</legend>
      <div class="row kinds">
        <label class="check"><input type="radio" checked={!letters} onchange={() => ((letters = false), (from = '0'), (to = '10'))} />{t('Numbers')}</label>
        <label class="check"><input type="radio" checked={letters} onchange={() => ((letters = true), (from = 'a'), (to = 'z'))} />{t('Letters')}</label>
      </div>
      <div class="row">
        <label for="from">{t('From:')}</label>
        <input id="from" class="text small" bind:value={from} />
        <label for="to" class="to">{t('To:')}</label>
        <input id="to" class="text small" bind:value={to} />
        <span class="grow"></span>
        <label for="size" class:disabled={letters}>{t('Wildcard size')}</label>
        <input id="size" class="text tiny" type="number" min="1" max="20" bind:value={size} disabled={letters} />
      </div>
    </fieldset>
    <fieldset class="group">
      <legend><label class="check"><input type="checkbox" bind:checked={useLogin} />{t('Use authorization')}</label></legend>
      <div class="row" class:disabled={!useLogin}>
        <label for="user" class="right">{t('Login')}</label>
        <input id="user" class="text grow" bind:value={user} disabled={!useLogin} autocomplete="off" />
        <label for="password" class="right">{t('Password')}</label>
        <input id="password" class="text grow" type="password" bind:value={password} disabled={!useLogin} autocomplete="off" />
      </div>
    </fieldset>
    <div class="files">
      <label for="first">{t('First file:')}</label><input id="first" class="text" readonly value={preview?.first ?? ''} />
      <label for="second">{t('Second file:')}</label><input id="second" class="text" readonly value={preview?.second ?? ''} />
      <span>...</span><span></span>
      <label for="last">{t('Last file:')}</label><input id="last" class="text" readonly value={preview?.last ?? ''} />
    </div>
    {#if error}<p class="error-text" role="alert">{error}</p>{:else if preview}<p class="muted">{t('{count} files', { count: preview.count })}</p>{/if}
    <div class="buttons">
      <button class="btn default" type="submit" disabled={!preview || busy}>{t('OK')}</button>
      <button class="btn" type="button" onclick={() => void closeWindow()}>{t('Cancel')}</button>
    </div>
  </form>
</Window>

<style>
  .dialog {
    gap: 6px;
  }

  p {
    margin: 0;
  }

  .lead {
    width: 60px;
  }

  .kinds {
    gap: 50px;
    margin-bottom: 6px;
  }

  .small {
    width: 70px;
  }

  .tiny {
    width: 50px;
  }

  .to {
    margin-inline-start: 30px;
  }

  .right {
    margin-inline-start: 6px;
  }

  fieldset .row .grow {
    width: 0;
  }

  .files {
    display: grid;
    grid-template-columns: 80px 1fr;
    align-items: center;
    gap: 3px 6px;
  }

  input[type='radio'] {
    margin-block: 0;
    margin-inline: 0 5px;
    accent-color: var(--focus);
  }

  .buttons .btn {
    width: 84px;
  }
</style>
