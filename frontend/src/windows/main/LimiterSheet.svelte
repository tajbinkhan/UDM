<!-- Speed Limiter settings. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { messageOf, setLimiter } from '../../ipc';
  import type { LimiterDraft } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';

  let { start, onclose }: { start: LimiterDraft; onclose: () => void } = $props();
  // The sheet edits a copy of what it opened with.
  let draft = $state(untrack(() => ({ ...start })));
  let error = $state('');

  async function save() {
    error = '';
    try {
      await setLimiter({ ...draft, kbPerSec: Math.round(Number(draft.kbPerSec)) || 0 });
      onclose();
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<Sheet title={t('Speed Limiter settings')} width={430} oncancel={onclose} onok={() => void save()}>
  <div class="row speed">
    <label for="kb">{t('Maximum download speed')}</label>
    <input id="kb" class="text num" type="number" min="1" bind:value={draft.kbPerSec} />
    <span>{t('KBytes/sec')}</span>
  </div>
  <p>{t("Note: Internet servers may break the connection when the speed is too limited. It's not recommended to use the Speed Limiter to download from servers that do not support resuming.")}</p>
  <label class="check"><input type="checkbox" bind:checked={draft.onStartup} />{t('Always turn on Speed Limiter on UDM startup')}</label>
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</Sheet>

<style>
  .speed {
    justify-content: center;
  }

  .num {
    width: 70px;
    text-align: right;
  }

  p {
    margin: 0;
  }
</style>
