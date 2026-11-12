<!-- The crawl's results: its progress, the files by page and site, and
     adding the ticked ones to the list. -->
<script lang="ts">
  import { grabAdd, grabSelect, grabStop, messageOf } from '../../ipc';
  import type { GrabberView } from '../../ipc/types';
  import { t } from '../../i18n';
  import ListBox from '../../components/ListBox.svelte';
  import FileGlyph from '../../components/FileGlyph.svelte';
  import TreeGlyph from '../../components/TreeGlyph.svelte';
  import { grab } from './grab.svelte';

  let { view, onsettings }: { view: GrabberView; onsettings: () => void } = $props();

  let pickedQueue = $state<number | null>(null);
  const queue = $derived(pickedQueue ?? view.queues[0]?.id ?? 1);
  let start = $state(true);
  let picked = $state(-1);
  let note = $state('');
  let error = $state('');
  const running = $derived(view.phase === 'running');
  const ticked = $derived(grab.checked.filter(Boolean).length);

  function tickAll(on: boolean) {
    grab.checked = grab.checked.map(() => on);
  }

  async function add() {
    error = '';
    note = '';
    try {
      const indices = grab.checked.flatMap((on, i) => (on ? [i] : []));
      await grabAdd(indices, queue, start);
      note = t('Added {count} files to the list.', { count: indices.length });
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<div class="results">
  <div class="bar">
    {#if running}
      <button class="btn" onclick={() => void grabStop()}>{t('Stop')}</button>
    {/if}
    <button class="btn" disabled={running} onclick={onsettings}>{t('< Settings')}</button>
    <button class="btn" disabled={running} onclick={() => tickAll(true)}>{t('Check All')}</button>
    <button class="btn" disabled={running} onclick={() => tickAll(false)}>{t('Uncheck All')}</button>
    <span class="grow"></span>
    <select class="select queue" value={queue} onchange={(e) => (pickedQueue = Number(e.currentTarget.value))} aria-label={t('Queue')} disabled={running}>
      {#each view.queues as q (q.id)}<option value={q.id}>{q.name}</option>{/each}
    </select>
    <label class="check"><input type="checkbox" bind:checked={start} disabled={running} />{t('Start it')}</label>
    <button class="btn default" disabled={running || ticked === 0 || !view.connected} onclick={() => void add()}>{t('Download checked')}</button>
  </div>
  <div class="stats" role="status">
    <span><b>{t('Status:')}</b> {view.stats.status}</span>
    <span>{t('Web pages processed')}: {view.stats.pages}</span>
    <span>{t('Explored')}: {view.stats.explored}</span>
    <span>{t('Matched')}: {view.stats.matched}</span>
    <span>{t('Downloaded')}: {view.stats.added}</span>
    {#if view.stats.stopped}<span class="error-text">{view.stats.stopped}</span>{/if}
  </div>
  {#if error}<p class="error-text" role="alert">{error}</p>{:else if note}<p role="status">{note}</p>{/if}
  <div class="body">
    <div class="tree" role="tree" aria-label={t('Results')}>
      {#each view.tree as n (n.key)}
        <div
          class="node"
          class:on={n.selected}
          role="treeitem"
          tabindex="0"
          aria-selected={n.selected}
          style:padding-left="{4 + n.depth * 16}px"
          onclick={() => n.open === null && void grabSelect(n.key)}
          onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && n.open === null && void grabSelect(n.key)}
        >
          <TreeGlyph icon={n.icon} /><span title={n.label}>{n.label}</span>
        </div>
      {/each}
    </div>
    <div class="files">
      <ListBox
        columns={[
          { title: t('File Name'), width: 150 },
          { title: t('File Type'), width: 80 },
          { title: t('Size'), width: 80 },
          { title: t('Link Text'), width: 100 },
          { title: t('Download from') },
        ]}
        count={view.rows.length}
        bind:selected={picked}
        fill
        label={t('Files')}
      >
        {#snippet cell(i, j)}
          {@const r = view.rows[i]}
          {#if j === 0}
            <input
              type="checkbox"
              checked={grab.checked[r.index] ?? false}
              onchange={(e) => (grab.checked[r.index] = e.currentTarget.checked)}
              onclick={(e) => e.stopPropagation()}
              aria-label={r.name}
            /><FileGlyph kind={r.icon} /><span class="name">{r.name}</span>
          {:else if j === 1}{r.kind}{:else if j === 2}{r.size}{:else if j === 3}{r.linkText}{:else}<span title={r.url}>{r.url}</span>{/if}
        {/snippet}
      </ListBox>
    </div>
  </div>
</div>

<style>
  .results {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .bar .btn {
    min-width: 0;
    padding: 0 8px;
  }

  .queue {
    width: 118px;
  }

  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
    padding: 3px 6px;
    border: 1px solid var(--border);
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 10px;
  }

  .tree {
    width: 150px;
    overflow: auto;
    padding: 2px;
    background: var(--field);
    border: 1px solid var(--input-border);
  }

  .node {
    display: flex;
    align-items: center;
    gap: 3px;
    height: 17px;
    white-space: nowrap;
  }

  .node span {
    padding: 0 2px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .node.on span {
    background: var(--selected);
  }

  .files {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    padding-left: 2px;
  }

  p {
    margin: 0;
  }
</style>
