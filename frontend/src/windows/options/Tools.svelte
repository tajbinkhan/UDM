<!-- Options > Tools: ffmpeg, which joins streamed videos, and yt-dlp's
     updates. -->
<script lang="ts">
  import { installTool, messageOf, pickPath } from '../../ipc';
  import type { OptionsView } from '../../ipc/types';
  import { t } from '../../i18n';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  let { view }: { view: OptionsView } = $props();
  // The pages are drawn only while the draft exists.
  const e = $derived(editing.draft!.extras);
  let error = $state('');

  async function browse() {
    error = '';
    try {
      const path = await pickPath('program', e.ffmpegPath);
      if (path) e.ffmpegPath = path;
    } catch (err) {
      error = messageOf(err);
    }
  }
</script>

<PageHead icon="tools" title={t('Tools')} />
<div class="opt-col">
  <div class="row">
    <label for="ffmpeg">ffmpeg</label>
    <input id="ffmpeg" class="text grow" bind:value={e.ffmpegPath} placeholder={t('Empty looks for it on PATH')} spellcheck="false" />
    <button class="btn small" aria-label={t('Where is ffmpeg.exe?')} onclick={() => void browse()}>...</button>
  </div>
  <p class="opt-note">{t('Joins streamed videos into one .mp4. Without it they are saved as .ts, or as separate video and audio.')}</p>
  <div class="opt-rule"></div>
  <label class="check"><input type="checkbox" bind:checked={e.ytdlpAutoUpdate} />{t('Keep yt-dlp up to date (looks once a day)')}</label>
  <p class="opt-note opt-indent">{t('Only a yt-dlp UDM installed is updated')}</p>
  <div class="opt-rule"></div>
  {#each view.tools as tool (tool.tool)}
    <div class="tool">
      <div class="row">
        <span class="grow status">{tool.status}</span>
        <button class="btn" disabled={!tool.installable || tool.busy} title={tool.hint} onclick={() => void installTool(tool.tool)}
          >{tool.busy ? t('Installing...') : tool.action}</button
        >
      </div>
      {#if !tool.installable}<p class="opt-note">{tool.hint}</p>{/if}
    </div>
  {/each}
  {#if view.toolNote}<p class:error-text={view.toolNote.error} class:opt-note={!view.toolNote.error} role="status">{view.toolNote.text}</p>{/if}
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</div>

<style>
  .status {
    overflow-wrap: anywhere;
  }

  p {
    margin: 0;
  }
</style>
