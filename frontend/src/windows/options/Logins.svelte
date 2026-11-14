<!-- Options > Sites Logins: user names and passwords for servers. The
     passwords never come back to the window; they show as stars. -->
<script lang="ts">
  import type { LoginDraft } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import ListBox from '../../components/ListBox.svelte';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  // The pages are drawn only while the draft exists.
  const d = $derived(editing.draft!);
  let picked = $state(-1);
  let sheet = $state<{ index: number; login: LoginDraft } | null>(null);

  const stars = (l: LoginDraft | undefined) => (l && (l.stored || l.password) ? '********' : '');
</script>

<PageHead icon="key" title={t('User names and passwords for servers/sites')} />
<ListBox
  columns={[{ title: t('Site/path') }, { title: t('User'), width: 80 }, { title: t('Password'), width: 70 }]}
  count={d.logins.length}
  bind:selected={picked}
  height={258}
  label={t('Sites Logins')}
  onopen={(i) => (sheet = { index: i, login: { ...d.logins[i] } })}
>
  {#snippet cell(i, j)}{j === 0 ? d.logins[i]?.site : j === 1 ? d.logins[i]?.user : stars(d.logins[i])}{/snippet}
</ListBox>
<div class="row buttons">
  <button class="btn" onclick={() => (sheet = { index: -1, login: { site: '', user: '', password: '', stored: false } })}>{t('New')}</button>
  <button class="btn" disabled={picked < 0} onclick={() => (sheet = { index: picked, login: { ...d.logins[picked] } })}>{t('Edit')}</button>
  <button
    class="btn"
    disabled={picked < 0}
    onclick={() => {
      d.logins = d.logins.filter((_, i) => i !== picked);
      picked = -1;
    }}>{t('Remove')}</button
  >
</div>

{#if sheet}
  <Sheet
    title={t('Site login')}
    width={440}
    okDisabled={!sheet.login.site.trim() || !sheet.login.user.trim()}
    oncancel={() => (sheet = null)}
    onok={() => {
      if (!sheet) return;
      if (sheet.index < 0) d.logins = [...d.logins, sheet.login];
      else d.logins[sheet.index] = sheet.login;
      sheet = null;
    }}
  >
    <label for="site">{t('Server/path')}</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="site" class="text" bind:value={sheet.login.site} autofocus placeholder="files.example.com/private/*" spellcheck="false" />
    <span class="opt-note">{t('Note: Type the path only if you have different login names for different server directories.')}</span>
    <div class="pairs">
      <label for="login-user">{t('User')}</label>
      <input id="login-user" class="text" bind:value={sheet.login.user} autocomplete="off" />
      <label for="login-pass">{t('Password')}</label>
      <input
        id="login-pass"
        class="text"
        type="password"
        bind:value={sheet.login.password}
        placeholder={sheet.login.stored ? t('(unchanged)') : ''}
        autocomplete="off"
      />
    </div>
  </Sheet>
{/if}

<style>
  .buttons {
    gap: 20px;
    margin-block: 10px 0;
    margin-inline: 10px 0;
  }

  .buttons .btn {
    width: 74px;
  }

  .pairs {
    display: grid;
    grid-template-columns: 110px 160px;
    align-items: center;
    gap: 6px 10px;
    justify-content: center;
  }

  .pairs label {
    text-align: end;
  }
</style>
