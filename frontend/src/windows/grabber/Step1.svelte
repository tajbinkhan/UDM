<!-- Step 1: the project's name, start page, template and login. -->
<script lang="ts">
  import { grabTemplate, messageOf } from '../../ipc';
  import type { GrabberView } from '../../ipc/types';
  import { t } from '../../i18n';
  import { grab } from './grab.svelte';

  let { view, name = $bindable(), advanced }: { view: GrabberView; name: string; advanced: boolean } = $props();
  // The steps are drawn only while the draft exists.
  const d = $derived(grab.draft!);
  let template = $state('');
  let error = $state('');

  async function useTemplate(value: string) {
    template = value;
    error = '';
    if (!value) return;
    try {
      const fresh = await grabTemplate(value);
      const keep = { start: d.start, loginOn: d.loginOn, loginUser: d.loginUser, loginPassword: d.loginPassword, loginPageOn: d.loginPageOn, loginPage: d.loginPage };
      grab.draft = { ...fresh, ...keep };
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<div class="col">
  <label for="project">{t('Grabber Project Name')}</label>
  <input id="project" class="text half" bind:value={name} placeholder={t('project_1')} />
  <label for="start">{t('Start page/address')}</label>
  <!-- svelte-ignore a11y_autofocus -->
  <input id="start" class="text" bind:value={d.start} placeholder="https://example.com/" spellcheck="false" autofocus />
  <label for="template">{t('Project template')}</label>
  <select id="template" class="select" value={template} onchange={(e) => void useTemplate(e.currentTarget.value)} disabled={!view.library}>
    <option value="">{t('Custom settings')}</option>
    {#each view.templates as name (name)}<option value={name}>{t(name)}</option>{/each}
  </select>
  <p class="note">
    {t('If you select a project template, the wizard will make the required project settings for the selected template on the next steps. Note that you may always change any settings manually.')}
  </p>
  <div class="rule"></div>
  <label class="check"><input type="checkbox" bind:checked={d.loginOn} />{t('Use authorization')}</label>
  <div class="row login" class:disabled={!d.loginOn}>
    <label for="user">{t('Login')}</label>
    <input id="user" class="text grow" bind:value={d.loginUser} disabled={!d.loginOn} autocomplete="off" />
    <label for="password">{t('Password')}</label>
    <input id="password" class="text grow" type="password" bind:value={d.loginPassword} disabled={!d.loginOn} autocomplete="off" placeholder={d.loginOn && d.loginUser ? t('(unchanged)') : ''} />
  </div>
  {#if advanced}
    <div class="rule"></div>
    <label class="check"><input type="checkbox" bind:checked={d.loginPageOn} />{t('Enter login and password manually at the following web page:')}</label>
    <input class="text" bind:value={d.loginPage} disabled={!d.loginPageOn} aria-label={t('Login page')} spellcheck="false" />
    <label class="check"><input type="checkbox" bind:checked={d.logoutOn} />{t("Don't open the logout page:")}</label>
    <p class="note">{t('Many sites have a logout page that the grabber should not open. UDM leaves out pages whose address names logout, logoff or sign-out; Step 3 lists them.')}</p>
  {/if}
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</div>

<style>
  .col {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .half {
    width: 250px;
  }

  .note {
    margin: 0;
    color: var(--fg-muted);
  }

  .rule {
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }

  .login {
    padding-left: 30px;
  }

  p {
    margin: 0;
  }
</style>
