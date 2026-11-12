<!-- Tip of the Day. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { closeTip } from '../../ipc';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';

  let { tips, next, onStartup, onclose }: { tips: string[]; next: number; onStartup: boolean; onclose: () => void } = $props();
  // Which tip shows, and the startup box, from what the sheet opened with.
  let at = $state(untrack(() => next % Math.max(1, tips.length)));
  let startup = $state(untrack(() => onStartup));

  function close() {
    void closeTip((at + 1) % Math.max(1, tips.length), startup);
    onclose();
  }
</script>

<Sheet title={t('Tip of the Day')} width={386} oncancel={close}>
  <div class="card">
    <div class="row head">
      <svg width="30" height="34" viewBox="0 0 30 34" aria-hidden="true">
        <path d="M15 3a10 10 0 0 0-6 18v4h12v-4a10 10 0 0 0-6-18z" fill="var(--tint-folder)" stroke="var(--fg-muted)" />
        <path d="M10 28h10M11 31h8" stroke="var(--fg-muted)" stroke-width="2" />
      </svg>
      <span class="did">{t('Did you know...')}</span>
    </div>
    <p>{tips[at] ?? ''}</p>
  </div>
  <div class="row foot">
    <label class="check grow"><input type="checkbox" bind:checked={startup} />{t('Show Tips on StartUp')}</label>
    <button type="button" class="btn" onclick={() => (at = (at + 1) % Math.max(1, tips.length))}>{t('Next Tip')}</button>
  </div>
</Sheet>

<style>
  .card {
    min-height: 170px;
    padding: 12px 16px;
    background: var(--field);
    border: 1px solid var(--border);
  }

  .head {
    gap: 14px;
    margin-bottom: 14px;
  }

  .did {
    flex: 1;
    padding: 3px 6px;
    background: var(--face);
  }

  p {
    margin: 0 0 0 44px;
  }
</style>
