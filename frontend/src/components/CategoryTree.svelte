<!-- The categories tree at the window's left. Up and Down move, Right and
     Left open and close a branch, Space or Enter shows its downloads. -->
<script lang="ts">
  import type { TreeNode } from '../ipc/types';
  import { t } from '../i18n';
  import TreeGlyph from './TreeGlyph.svelte';

  let {
    nodes,
    onselect,
    ontoggle,
    onclose,
    onmenu,
  }: {
    nodes: TreeNode[];
    onselect: (key: string) => void;
    ontoggle: (key: string) => void;
    onclose: () => void;
    onmenu: (node: TreeNode, x: number, y: number) => void;
  } = $props();

  let focus = $state(0);
  let items: HTMLElement[] = $state([]);

  function onkeydown(e: KeyboardEvent, i: number) {
    const n = nodes[i];
    const go = (j: number) => {
      focus = Math.max(0, Math.min(nodes.length - 1, j));
      items[focus]?.focus();
    };
    switch (e.key) {
      case 'ArrowDown':
        go(i + 1);
        break;
      case 'ArrowUp':
        go(i - 1);
        break;
      case 'ArrowRight':
        if (n.open === false) ontoggle(n.key);
        break;
      case 'ArrowLeft':
        if (n.open === true) ontoggle(n.key);
        break;
      case 'Enter':
      case ' ':
        onselect(n.key);
        break;
      default:
        return;
    }
    e.preventDefault();
  }
</script>

<div class="panel">
  <div class="head">
    <span>{t('Categories')}</span>
    <button class="close" aria-label={t('Hide categories')} title={t('Hide categories')} onclick={onclose}>✕</button>
  </div>
  <div class="tree" role="tree" aria-label={t('Categories')}>
    {#each nodes as n, i (n.key)}
      <div
        bind:this={items[i]}
        class="node"
        class:selected={n.selected}
        role="treeitem"
        aria-selected={n.selected}
        aria-expanded={n.open ?? undefined}
        aria-level={n.depth + 1}
        tabindex={i === focus ? 0 : -1}
        style:padding-left="{4 + n.depth * 18}px"
        onclick={() => {
          focus = i;
          onselect(n.key);
        }}
        ondblclick={() => n.open !== null && n.open !== undefined && ontoggle(n.key)}
        oncontextmenu={(e) => {
          e.preventDefault();
          focus = i;
          onselect(n.key);
          onmenu(n, e.clientX, e.clientY);
        }}
        onkeydown={(e) => onkeydown(e, i)}
      >
        {#if n.open !== null && n.open !== undefined}
          <button
            class="twist"
            tabindex="-1"
            aria-hidden="true"
            onclick={(e) => {
              e.stopPropagation();
              ontoggle(n.key);
            }}
          >
            <svg width="9" height="9" viewBox="0 0 9 9"><path d={n.open ? 'M1 2.5l3.5 4 3.5-4' : 'M2.5 1l4 3.5-4 3.5'} fill="none" stroke="currentColor" stroke-width="1.3" /></svg>
          </button>
        {:else}
          <span class="twist"></span>
        {/if}
        <TreeGlyph icon={n.icon} />
        <span class="label">{n.label}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--tree);
    border: 1px solid var(--frame);
    min-width: 0;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 17px;
    padding: 0 2px 0 4px;
    background: var(--face);
    border-bottom: 1px solid var(--frame);
    flex: none;
  }

  .close {
    width: 15px;
    height: 15px;
    padding: 0;
    font-size: 11px;
    line-height: 1;
    background: none;
    border: 1px solid transparent;
    color: var(--fg);
  }

  .close:hover {
    border-color: var(--border);
    background: var(--hover);
  }

  .tree {
    flex: 1;
    overflow: auto;
    padding: 2px 0;
  }

  .node {
    display: flex;
    align-items: center;
    gap: 3px;
    height: 16px;
    white-space: nowrap;
  }

  .label {
    padding: 0 3px;
    line-height: 16px;
  }

  .node.selected .label {
    background: var(--selected);
    color: var(--selected-fg);
  }

  .twist {
    width: 12px;
    height: 12px;
    padding: 0;
    flex: none;
    display: grid;
    place-items: center;
    background: none;
    border: 0;
    color: var(--fg-muted);
  }
</style>
