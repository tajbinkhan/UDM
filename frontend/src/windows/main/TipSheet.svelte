<!-- Tip of the Day. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { closeTip } from '../../ipc';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import FluentIcon from '../../components/FluentIcon.svelte';
  import { fluent } from '../../components/fluent';

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
    <FluentIcon icon={fluent.lightbulb32Regular} size={32} color="var(--tint-folder)" />
    <div class="words">
      <p class="did">{t('Did you know...')}</p>
      <p>{tips[at] ?? ''}</p>
    </div>
  </div>
  <div class="row foot">
    <label class="check grow"><input type="checkbox" bind:checked={startup} />{t('Show Tips on StartUp')}</label>
    <button type="button" class="btn" onclick={() => (at = (at + 1) % Math.max(1, tips.length))}>{t('Next Tip')}</button>
  </div>
</Sheet>

<style>
  .card {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    min-height: 150px;
    padding: 16px;
    background: var(--field);
    border: 1px solid var(--frame);
    border-radius: var(--radius-large);
  }

  .words {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 4px;
  }

  p {
    margin: 0;
    line-height: 1.45;
  }

  .did {
    font-size: calc(var(--text) + 2px);
    font-weight: 600;
  }

  .foot {
    margin-top: 4px;
  }
</style>
