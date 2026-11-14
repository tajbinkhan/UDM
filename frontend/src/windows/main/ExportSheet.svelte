<!-- Export download list: the main queue, the selection or every download,
     in IDM's export format, as text or as UDM's own list. -->
<script lang="ts">
  import { exportList, messageOf } from '../../ipc';
  import type { ExportFormat, ExportScope } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';

  let { selected, onclose }: { selected: number; onclose: () => void } = $props();
  let scope = $state<ExportScope>('queue');
  let format = $state<ExportFormat>('ef2');
  let note = $state('');

  async function save() {
    note = '';
    try {
      const done = await exportList(scope, format);
      if (done) note = done;
    } catch (e) {
      note = messageOf(e);
    }
  }
</script>

<Sheet title={t('Export download list')} width={320} oncancel={onclose} onok={() => void save()}>
  <label class="check"><input type="radio" value="queue" bind:group={scope} />{t('Export download queue')}</label>
  <label class="check"><input type="radio" value="selected" bind:group={scope} disabled={selected === 0} />{t('Export selected files')}</label>
  <label class="check"><input type="radio" value="all" bind:group={scope} />{t('Export all files')}</label>
  <label for="format">{t('Format')}</label>
  <select id="format" class="select" bind:value={format}>
    <option value="ef2">{t('IDM export file (.ef2)')}</option>
    <option value="text">{t('Text file (.txt), one address per line')}</option>
    <option value="json">{t('UDM list (.json)')}</option>
  </select>
  {#if note}<p role="status">{note}</p>{/if}
</Sheet>

<style>
  input[type='radio'] {
    margin-block: 0;
    margin-inline: 0 5px;
    accent-color: var(--focus);
  }

  p {
    margin: 0;
  }
</style>
