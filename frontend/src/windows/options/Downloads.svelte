<!-- Options > Downloads: the save-as and completion dialogs, duplicates,
     the User-Agent, the progress window and the virus scan. -->
<script lang="ts">
  import { messageOf, pickPath } from '../../ipc';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  // The pages are drawn only while the draft exists.
  const d = $derived(editing.draft!);

  let progress = $state<{ show: boolean; details: boolean; speed: boolean; done: boolean; hide: boolean } | null>(null);
  let scan = $state<{ on: boolean; custom: boolean; command: string } | null>(null);
  let error = $state('');
</script>

<PageHead icon="download" title={t('Default download settings')} />
<div class="opt-col">
  <div class="row end">
    <span>{t('Customize "Download progress" dialog')}</span>
    <button
      class="btn"
      onclick={() =>
        (progress = {
          show: d.progressOnStart,
          details: d.progressStartDetails,
          speed: d.progressSpeedTab,
          done: d.progressDoneTab,
          hide: d.progressHideButtons,
        })}>{t('Edit...')}</button
    >
  </div>
  <label class="check"><input type="checkbox" bind:checked={d.askWhereToSave} />{t('Show start download dialog')}</label>
  <label class="check opt-indent"
    ><input type="checkbox" bind:checked={d.addOnly} disabled={!d.askWhereToSave} />{t('Do not start downloading, only add files to the queue')}</label
  >
  <label class="check"><input type="checkbox" bind:checked={d.doneDialog} />{t('Show download complete dialog')}</label>
  <span class="opt-note">{t("Note: These settings don't relate to queue processing")}</span>
  <div class="opt-rule"></div>
  <label class="check"
    ><input type="checkbox" bind:checked={d.startWhileAsking} />{t('Start downloading immediately while displaying "Download File Info" dialog')}</label
  >
  <label class="check"><input type="checkbox" bind:checked={d.laterPanel} />{t('Show queue selection panel on pressing "Download Later" button')}</label>
  <label class="check"><input type="checkbox" bind:checked={d.listQueuePanel} />{t('Show queue selection panel on closing batch download dialogs')}</label>
  <label class="check"><input type="checkbox" bind:checked={d.resumeIgnoresDate} />{t('Ignore file modification time changes when resuming a download')}</label>
  <div class="opt-rule"></div>
  <label for="dup">{t('If a duplicate download link is added:')}</label>
  <select id="dup" class="select" bind:value={d.ifDuplicate}>
    <option value="ask">{t('Show a dialog and ask what to do.')}</option>
    <option value="newName">{t('Add the duplicate with a numbered file name.')}</option>
    <option value="overwrite">{t('Add the duplicate and overwrite the existing file.')}</option>
    <option value="existing">{t('Show the finished download, or resume the unfinished one.')}</option>
  </select>
  <div class="opt-rule"></div>
  <label for="ua">{t('User-Agent for manually added downloads:')}</label>
  <input id="ua" class="text" bind:value={d.userAgent} placeholder={t("UDM's own")} spellcheck="false" />
  <div class="opt-rule"></div>
  <div class="row end">
    <span>{t('Virus checking settings')}</span>
    <button class="btn" onclick={() => (scan = { on: d.scanAfterDownload, custom: d.scannerCustom, command: d.scannerCommand })}>{t('Edit...')}</button>
  </div>
</div>

{#if progress}
  <Sheet
    title={t('Customize download progress dialog')}
    width={400}
    oncancel={() => (progress = null)}
    onok={() => {
      if (!progress) return;
      d.progressOnStart = progress.show;
      d.progressStartDetails = progress.details;
      d.progressSpeedTab = progress.speed;
      d.progressDoneTab = progress.done;
      d.progressHideButtons = progress.hide;
      progress = null;
    }}
  >
    <label for="start-view">{t('Start view:')}</label>
    <select
      id="start-view"
      class="select view"
      value={progress.show ? 'show' : 'hide'}
      onchange={(e) => progress && (progress.show = e.currentTarget.value === 'show')}
    >
      <option value="show">{t('Show normally sized')}</option>
      <option value="hide">{t("Don't show")}</option>
    </select>
    <p class="opt-note">
      {t('Note: "Start view" doesn\'t relate to queue processing. While UDM runs a queue, its downloads start without the dialog.')}
    </p>
    <label class="check"><input type="checkbox" bind:checked={progress.details} />{t('Start with the details shown')}</label>
    <label class="check"><input type="checkbox" bind:checked={progress.speed} />{t('Show "Speed Limiter" tab')}</label>
    <label class="check"><input type="checkbox" bind:checked={progress.done} />{t('Show "Options on completion" tab')}</label>
    <label class="check"><input type="checkbox" bind:checked={progress.hide} />{t('Show "Hide tab" buttons')}</label>
  </Sheet>
{:else if scan}
  <Sheet
    title={t('Virus checking')}
    width={480}
    oncancel={() => (scan = null)}
    onok={() => {
      if (!scan) return;
      d.scanAfterDownload = scan.on;
      d.scannerCustom = scan.custom;
      d.scannerCommand = scan.command;
      scan = null;
    }}
  >
    <label class="check"><input type="checkbox" bind:checked={scan.on} />{t('Check each finished download for viruses')}</label>
    <label for="scanner">{t('Virus scanner program')}</label>
    <select id="scanner" class="select" value={scan.custom ? 'custom' : 'defender'} disabled={!scan.on} onchange={(e) => scan && (scan.custom = e.currentTarget.value === 'custom')}>
      <option value="defender">{t('Microsoft Defender')}</option>
      <option value="custom">{t('Another program')}</option>
    </select>
    <label for="command">{t('Command line parameters')}</label>
    <div class="row">
      <input id="command" class="text grow" bind:value={scan.command} disabled={!scan.on || !scan.custom} spellcheck="false" />
      <button
        type="button"
        class="btn"
        disabled={!scan.on || !scan.custom}
        onclick={async () => {
          error = '';
          try {
            const p = await pickPath('program', '');
            if (p && scan) scan.command = `"${p}" {path}`;
          } catch (e) {
            error = messageOf(e);
          }
        }}>{t('Browse')}</button
      >
    </div>
    <p class="opt-note">
      {t(
        'Type the program and its parameters, with {path} where the finished file goes. Programs with real-time protection already check every file UDM writes.',
      )}
    </p>
    {#if error}<p class="error-text" role="alert">{error}</p>{/if}
  </Sheet>
{/if}

<style>
  .end {
    justify-content: flex-end;
  }

  .view {
    width: 180px;
  }

  p {
    margin: 0;
  }
</style>
