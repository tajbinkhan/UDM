<!-- Customize Toolbar: which buttons the toolbar holds, in what order, with
     separators. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { setToolbar } from '../../ipc';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import ToolIcon, { type ToolIconName } from '../../components/ToolIcon.svelte';

  let {
    current,
    small,
    labels,
    onclose,
  }: { current: string[]; small: boolean; labels: Record<string, string>; onclose: () => void } = $props();

  const all = ['add', 'resume', 'stop', 'stopAll', 'delete', 'deleteCompleted', 'options', 'scheduler', 'startQueue', 'stopQueue', 'grabber'];
  // The sheet edits a copy of what it opened with.
  let shown = $state(untrack(() => [...current]));
  let left = $state(0);
  let right = $state(-1);
  const available = $derived(['separator', ...all.filter((id) => !shown.includes(id))]);

  function add() {
    const id = available[left];
    if (!id) return;
    const at = right < 0 ? shown.length : right + 1;
    shown = [...shown.slice(0, at), id, ...shown.slice(at)];
    right = at;
  }

  function move(step: number) {
    const to = right + step;
    if (right < 0 || to < 0 || to >= shown.length) return;
    const next = [...shown];
    [next[right], next[to]] = [next[to], next[right]];
    shown = next;
    right = to;
  }

  const name = (id: string) => (id === 'separator' ? t('Separator') : t(labels[id] ?? id));
</script>

<Sheet
  title={t('Customize Toolbar')}
  width={540}
  oncancel={onclose}
  onok={() => {
    void setToolbar(small, $state.snapshot(shown));
    onclose();
  }}
>
  <div class="cols">
    <div class="col">
      <span>{t('Available toolbar buttons:')}</span>
      <div class="box" role="listbox" aria-label={t('Available toolbar buttons:')}>
        {#each available as id, i (id + i)}
          <div class="entry" class:on={left === i} class:sep={id === 'separator'} role="option" aria-selected={left === i} tabindex="0" onclick={() => (left = i)} onkeydown={(e) => e.key === 'Enter' && add()}>
            {#if id !== 'separator'}<ToolIcon name={id as ToolIconName} size={24} />{/if}<span>{name(id)}</span>
          </div>
        {/each}
      </div>
    </div>
    <div class="mid">
      <button type="button" class="btn" disabled={!available[left]} onclick={add}>{t('Add ->')}</button>
      <button
        type="button"
        class="btn"
        disabled={right < 0}
        onclick={() => {
          shown = shown.filter((_, i) => i !== right);
          right = Math.min(right, shown.length - 1);
        }}>{t('<- Remove')}</button
      >
    </div>
    <div class="col">
      <span>{t('Current toolbar buttons:')}</span>
      <div class="box" role="listbox" aria-label={t('Current toolbar buttons:')}>
        {#each shown as id, i (id + i)}
          <div class="entry" class:on={right === i} class:sep={id === 'separator'} role="option" aria-selected={right === i} tabindex="0" onclick={() => (right = i)} onkeydown={() => undefined}>
            {#if id !== 'separator'}<ToolIcon name={id as ToolIconName} size={24} />{/if}<span>{name(id)}</span>
          </div>
        {/each}
      </div>
    </div>
    <div class="side">
      <button type="button" class="btn" onclick={() => ((shown = [...all]), (right = -1))}>{t('Reset')}</button>
      <span class="grow"></span>
      <button type="button" class="btn" disabled={right <= 0} onclick={() => move(-1)}>{t('Move Up')}</button>
      <button type="button" class="btn" disabled={right < 0 || right >= shown.length - 1} onclick={() => move(1)}>{t('Move Down')}</button>
    </div>
  </div>
</Sheet>

<style>
  .cols {
    display: flex;
    gap: 10px;
    height: 170px;
  }

  .col {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .box {
    flex: 1;
    overflow: auto;
    background: var(--field);
    border: 1px solid var(--input-border);
  }

  .entry {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 8px;
  }

  .entry.sep {
    justify-content: center;
    color: var(--fg-muted);
  }

  .entry.on {
    background: var(--selected);
  }

  .mid,
  .side {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 8px;
  }

  .side {
    justify-content: flex-start;
  }

  .mid .btn,
  .side .btn {
    min-width: 84px;
  }
</style>
