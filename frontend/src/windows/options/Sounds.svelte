<!-- Options > Sounds: a sound for each of UDM's events. -->
<script lang="ts">
  import { messageOf, pickPath, playSound } from '../../ipc';
  import { t } from '../../i18n';
  import ListBox from '../../components/ListBox.svelte';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  // The pages are drawn only while the draft exists.
  const d = $derived(editing.draft!);
  const events = ['Download complete', 'Download failed', 'Queue processing started', 'Queue processing stopped/finished'];
  let picked = $state(-1);
  let error = $state('');

  async function browse() {
    if (picked < 0) return;
    error = '';
    try {
      const p = await pickPath('sound', d.sounds[picked].file);
      if (p) {
        d.sounds[picked].file = p;
        d.sounds[picked].on = true;
      }
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<PageHead icon="sound" title={t('Sound settings')} />
<fieldset class="group">
  <legend>{t('Select sounds for UDM events')}</legend>
  <ListBox
    columns={[{ title: t('Event'), width: 235 }, { title: t('Sound file') }]}
    count={events.length}
    bind:selected={picked}
    height={240}
    label={t('Select sounds for UDM events')}
    onopen={() => void browse()}
  >
    {#snippet cell(i, j)}
      {#if j === 0}
        <input type="checkbox" bind:checked={d.sounds[i].on} aria-label={t(events[i])} />{t(events[i])}
      {:else}
        {d.sounds[i].file || t('(the system sound)')}
      {/if}
    {/snippet}
  </ListBox>
  <div class="row buttons">
    <button class="btn" disabled={picked < 0} onclick={() => void browse()}>{t('Browse...')}</button>
    <button class="btn" disabled={picked < 0 || !d.sounds[picked]?.file} onclick={() => picked >= 0 && (d.sounds[picked].file = '')}>{t('Default')}</button>
    <button class="btn" disabled={picked < 0} onclick={() => picked >= 0 && void playSound(picked, d.sounds[picked].file)}>{t('Play')}</button>
  </div>
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</fieldset>

<style>
  .buttons {
    justify-content: center;
    gap: 50px;
    margin-top: 10px;
  }

  .buttons .btn {
    width: 74px;
  }

  p {
    margin: 4px 0 0;
  }
</style>
