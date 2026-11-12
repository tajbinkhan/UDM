<!-- The Scheduler's Files in the queue tab: how many run at once, and the
     queue's downloads in order. -->
<script lang="ts">
  import { messageOf, moveQueued, removeQueued } from '../../ipc';
  import type { QueueDraft, QueueFiles } from '../../ipc/types';
  import { t } from '../../i18n';
  import ListBox from '../../components/ListBox.svelte';
  import FileGlyph from '../../components/FileGlyph.svelte';

  let { q = $bindable(), files, maxAtOnce }: { q: QueueDraft; files: QueueFiles | undefined; maxAtOnce: number } = $props();

  let picked = $state(-1);
  let error = $state('');
  const rows = $derived(files?.rows ?? []);

  async function run(p: Promise<unknown>) {
    error = '';
    try {
      await p;
    } catch (e) {
      error = messageOf(e);
    }
  }

  function move(offset: number) {
    const row = rows[picked];
    if (!row) return;
    picked = Math.max(0, Math.min(rows.length - 1, picked + offset));
    void run(moveQueued(row.id, offset));
  }
</script>

<div class="tab">
  <div class="row count">
    <label for="at-once">{t('Download')}</label>
    <input id="at-once" class="text num" type="number" min="1" max={maxAtOnce} bind:value={q.maxConcurrent} />
    <span>{t('files at the same time')}</span>
  </div>
  <ListBox
    columns={[{ title: t('File Name') }, { title: t('Size'), width: 68 }, { title: t('Status'), width: 52 }, { title: t('Time left'), width: 70 }]}
    count={rows.length}
    bind:selected={picked}
    height={262}
    label={t('Files in the queue')}
  >
    {#snippet cell(i, j)}
      {@const r = rows[i]}
      {#if j === 0}<FileGlyph kind={r.icon} /><span class="name">{r.name}</span>{:else if j === 1}{r.size}{:else if j === 2}{r.status}{:else}{r.timeLeft}{/if}
    {/snippet}
  </ListBox>
  <div class="row tools">
    <button class="tool" disabled={picked < 0 || picked >= rows.length - 1} aria-label={t('Move down')} title={t('Move down')} onclick={() => move(1)}>
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M5.5 1.5h5v7h3L8 14.5 2.5 8.5h3z" fill="var(--tint-folder)" stroke="var(--tint-blue)" /></svg>
    </button>
    <button class="tool" disabled={picked <= 0} aria-label={t('Move up')} title={t('Move up')} onclick={() => move(-1)}>
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M5.5 14.5h5v-7h3L8 1.5 2.5 7.5h3z" fill="var(--tint-folder)" stroke="var(--tint-blue)" /></svg>
    </button>
    <button
      class="tool"
      disabled={picked < 0}
      aria-label={t('Delete from queue')}
      title={t('Delete from queue')}
      onclick={() => {
        const row = rows[picked];
        picked = -1;
        if (row) void run(removeQueued(row.id));
      }}
    >
      <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M3 3l10 10M13 3L3 13" stroke="var(--tint-red)" stroke-width="2.4" /></svg>
    </button>
  </div>
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</div>

<style>
  .tab {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .count {
    padding-left: 12px;
  }

  .num {
    width: 46px;
    text-align: right;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    padding-left: 3px;
  }

  .tools {
    gap: 6px;
    padding-left: 10px;
  }

  .tool {
    width: 24px;
    height: 24px;
    padding: 0;
    display: grid;
    place-items: center;
    background: var(--button);
    border: 1px solid var(--button-border);
  }

  .tool:disabled {
    opacity: 0.4;
  }

  p {
    margin: 0;
  }
</style>
