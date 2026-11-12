<!-- Convert: make a new file from a finished video with ffmpeg. -->
<script lang="ts">
  import { convertDownload, messageOf } from '../../ipc';
  import type { ConvertTarget, Row } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';

  let { row, onclose }: { row: Row; onclose: () => void } = $props();
  const targets: [ConvertTarget, string][] = [
    ['mp4', 'An MP4 video'],
    ['mp3', 'Its sound as an MP3'],
    ['m4a', 'Its sound as an M4A'],
    ['opus', 'Its sound as an Opus file'],
    ['flac', 'Its sound as a FLAC file'],
  ];
  let target = $state<ConvertTarget>('mp3');
  let deleteOriginal = $state(false);
  let error = $state('');

  async function convert() {
    error = '';
    try {
      await convertDownload(row.id, target, deleteOriginal);
      onclose();
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<Sheet title={t('Convert')} width={400} oncancel={onclose} onok={() => void convert()}>
  <p>{t('Make a new file from {name} with ffmpeg.', { name: row.name })}</p>
  <fieldset class="group opts">
    {#each targets as [value, label] (value)}
      <label class="check"><input type="radio" name="target" {value} bind:group={target} />{t(label)}</label>
    {/each}
  </fieldset>
  <label class="check"><input type="checkbox" bind:checked={deleteOriginal} />{t('Then send the original to the Recycle Bin')}</label>
  <p class="muted">{t('It shows in the list with its progress. Streams are copied when they fit, else encoded.')}</p>
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</Sheet>

<style>
  p {
    margin: 0 0 6px;
    overflow-wrap: anywhere;
  }

  .opts {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-bottom: 6px;
  }
</style>
