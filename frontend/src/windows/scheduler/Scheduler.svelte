<!-- The Scheduler: the queues at the left, the chosen queue's schedule and
     downloads at the right, or the download limits. Apply saves the page
     shown; Close leaves what was not applied. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { closeWindow, controlQueue, createQueue, deleteQueue, messageOf, saveQueue, saveQuota } from '../../ipc';
  import type { QueueDraft, QuotaDraft, SchedulerView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import Sheet from '../../components/Sheet.svelte';
  import TreeGlyph from '../../components/TreeGlyph.svelte';
  import ToolIcon from '../../components/ToolIcon.svelte';
  import ScheduleTab from './ScheduleTab.svelte';
  import FilesTab from './FilesTab.svelte';

  const view: SchedulerView | null = $derived(projection.view?.window === 'scheduler' ? projection.view.view : null);

  /** The page shown: a queue's id, or the download limits. */
  let shown = $state<number | 'limits'>(1);
  let tab = $state<'schedule' | 'files'>('schedule');
  /** Each queue's edits, and what the shell last sent for it. */
  let drafts = $state<Record<number, QueueDraft>>({});
  let bases: Record<number, string> = {};
  let quota = $state<QuotaDraft | null>(null);
  let quotaBase = '';
  let naming = $state<string | null>(null);
  let error = $state('');
  let busy = $state(false);

  const same = (a: unknown, b: string) => JSON.stringify(a) === b;

  // Take the shell's queues, keeping the edits not applied yet.
  $effect(() => {
    const v = view;
    if (v) untrack(() => merge(v));
  });

  function merge(view: SchedulerView) {
    for (const q of view.queues) {
      const fresh = JSON.stringify(q);
      const draft = drafts[q.id];
      if (!draft || same($state.snapshot(draft), bases[q.id] ?? '')) drafts[q.id] = $state.snapshot(q);
      else drafts[q.id].running = q.running;
      bases[q.id] = fresh;
    }
    const freshQuota = JSON.stringify(view.quota);
    if (!quota || same($state.snapshot(quota), quotaBase)) quota = { ...view.quota };
    quotaBase = freshQuota;
    if (shown !== 'limits' && !view.queues.some((q) => q.id === shown)) shown = view.queues[0]?.id ?? 'limits';
  }

  const q = $derived(shown === 'limits' ? undefined : drafts[shown]);
  const dirty = $derived(
    shown === 'limits' ? !!quota && !same($state.snapshot(quota), quotaBase) : !!q && !same($state.snapshot(q), bases[q.id] ?? ''),
  );

  async function run(p: Promise<unknown>): Promise<boolean> {
    if (busy) return false;
    busy = true;
    error = '';
    try {
      await p;
      return true;
    } catch (e) {
      error = messageOf(e);
      return false;
    } finally {
      busy = false;
    }
  }

  async function apply() {
    if (shown === 'limits') {
      if (quota) await run(saveQuota($state.snapshot(quota)));
    } else if (q) await run(saveQueue($state.snapshot(q)));
  }
</script>

<Window onescape={() => void closeWindow()}>
  {#if view}
    <div class="scheduler">
      <div class="left">
        <div class="head">
          <ToolIcon name="scheduler" size={30} />
          <span>{t('Queues')}</span>
        </div>
        <div class="list" role="listbox" aria-label={t('Queues')}>
          {#each view.queues as item (item.id)}
            <div
              class="item"
              class:on={shown === item.id}
              role="option"
              tabindex="0"
              aria-selected={shown === item.id}
              onclick={() => (shown = item.id)}
              onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && (shown = item.id)}
            >
              <TreeGlyph icon="queue" /><span>{t(item.name)}</span>
            </div>
          {/each}
          <div
            class="item"
            class:on={shown === 'limits'}
            role="option"
            tabindex="0"
            aria-selected={shown === 'limits'}
            onclick={() => (shown = 'limits')}
            onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && (shown = 'limits')}
          >
            <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
              <path d="M1.5 3.5h5l1.5 1.5h6.5v8.5h-13z" fill="var(--tint-folder)" stroke="var(--fg-muted)" />
              <path d="M5 7l5 5M10 7l-5 5" stroke="var(--tint-red)" stroke-width="2" />
            </svg>
            <span>{t('Download limits')}</span>
          </div>
        </div>
        <div class="row pair">
          <button class="btn" disabled={!view.connected} onclick={() => (naming = view?.newName ?? '')}>{t('New queue')}</button>
          <button class="btn" disabled={!q || q.main || !view.connected} onclick={() => q && void run(deleteQueue(q.id))}>{t('Delete')}</button>
        </div>
      </div>
      <div class="right">
        {#if q}
          <div class="title">{t(q.name)}</div>
          <div class="tabs" role="tablist">
            <button class="tab" class:on={tab === 'schedule'} role="tab" aria-selected={tab === 'schedule'} onclick={() => (tab = 'schedule')}
              >{t('Schedule')}</button
            >
            <button class="tab" class:on={tab === 'files'} role="tab" aria-selected={tab === 'files'} onclick={() => (tab = 'files')}
              >{t('Files in the queue')}</button
            >
          </div>
          <div class="page" role="tabpanel">
            {#if tab === 'schedule'}
              {#key q.id}<ScheduleTab bind:q={drafts[q.id]} maxRetries={view.maxRetries} />{/key}
            {:else}
              <FilesTab bind:q={drafts[q.id]} files={view.files.find((f) => f.queue === q?.id)} maxAtOnce={view.maxAtOnce} />
            {/if}
          </div>
        {:else if quota}
          <div class="limits">
            <label class="check"><input type="checkbox" bind:checked={quota.enabled} />{t('Download limits')}</label>
            <div class="quota" class:disabled={!quota.enabled}>
              <label for="mb">{t('Download no more than')}</label>
              <input id="mb" class="text num" type="number" min="1" bind:value={quota.mb} disabled={!quota.enabled} />
              <span>{t('MBytes')}</span>
              <label for="hours">{t('every')}</label>
              <input id="hours" class="text num" type="number" min="1" max="168" bind:value={quota.hours} disabled={!quota.enabled} />
              <span>{t('hours')}</span>
            </div>
            <label class="check"><input type="checkbox" bind:checked={quota.warn} disabled={!quota.enabled} />{t('Show warning before stopping downloads')}</label>
          </div>
        {/if}
        {#if error}<p class="error-text" role="alert">{error}</p>{/if}
        <div class="row foot">
          <button class="btn" disabled={!q || q.running || !view.connected} onclick={() => q && void run(controlQueue(q.id, true))}>{t('Start now')}</button>
          <button class="btn" disabled={!q || !q.running || !view.connected} onclick={() => q && void run(controlQueue(q.id, false))}>{t('Stop')}</button>
          <span class="grow"></span>
          <button class="btn" disabled={!dirty || busy || !view.connected} onclick={() => void apply()}>{t('Apply')}</button>
          <button class="btn" onclick={() => void closeWindow()}>{t('Close')}</button>
        </div>
      </div>
    </div>
    {#if naming !== null}
      <Sheet
        title={t('Enter queue name')}
        width={290}
        okDisabled={!naming.trim()}
        oncancel={() => (naming = null)}
        onok={async () => {
          if (naming !== null && (await run(createQueue(naming)))) naming = null;
        }}
      >
        <label for="queue-name">{t('Enter a name for the new queue that will be displayed in the list of queues')}</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="queue-name" class="text" bind:value={naming} autofocus />
      </Sheet>
    {/if}
  {/if}
</Window>

<style>
  .scheduler {
    display: flex;
    height: 100%;
    padding: 6px 14px 8px;
    gap: 14px;
  }

  .left {
    width: 174px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-right: 14px;
    border-right: 1px solid var(--border);
  }

  .head {
    display: flex;
    align-items: flex-end;
    gap: 14px;
    height: 32px;
  }

  .list {
    flex: 1;
    padding: 2px;
    background: var(--field);
    border: 1px solid var(--input-border);
    overflow: auto;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 16px;
    white-space: nowrap;
  }

  .item span {
    padding: 0 2px;
  }

  .item.on span {
    background: var(--selected);
  }

  .pair {
    justify-content: space-between;
  }

  .pair .btn {
    min-width: 72px;
    padding: 0 6px;
  }

  .right {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .title {
    text-align: center;
    height: 20px;
    line-height: 20px;
  }

  .tabs {
    display: flex;
  }

  .tab {
    height: 18px;
    padding: 0 6px;
    margin-right: -1px;
    background: var(--face);
    border: 1px solid var(--frame);
    border-bottom: 0;
    color: var(--fg);
  }

  .tabs {
    align-items: flex-end;
    height: 20px;
  }

  .tab.on {
    position: relative;
    z-index: 1;
    height: 20px;
    margin-bottom: -1px;
    background: var(--list);
  }

  .page {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 6px 10px;
    background: var(--list);
    border: 1px solid var(--frame);
  }

  .limits {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 20px 26px;
  }

  .quota {
    display: grid;
    grid-template-columns: auto 52px auto;
    align-items: center;
    justify-content: start;
    gap: 8px 6px;
    padding-left: 24px;
  }

  .quota label {
    text-align: right;
  }

  .num {
    text-align: right;
  }

  .foot {
    margin-top: 8px;
  }

  .foot .btn {
    min-width: 74px;
  }

  p {
    margin: 4px 0 0;
  }
</style>
