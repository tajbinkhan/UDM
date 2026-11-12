<!-- A small list with columns and one selected row, for Options' lists:
     server exceptions, site logins, sounds, address exceptions. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    columns,
    count,
    selected = $bindable(-1),
    height = 120,
    fill = false,
    label,
    cell,
    onopen,
  }: {
    columns: { title: string; width?: number }[];
    count: number;
    selected?: number;
    height?: number;
    /** Take the height the parent leaves instead of `height`. */
    fill?: boolean;
    label: string;
    cell: Snippet<[number, number]>;
    onopen?: (row: number) => void;
  } = $props();

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') selected = Math.min(count - 1, selected + 1);
    else if (e.key === 'ArrowUp') selected = Math.max(0, selected - 1);
    else if (e.key === 'Enter' && selected >= 0) onopen?.(selected);
    else return;
    e.preventDefault();
  }
</script>

<div class="box" class:fill-box={fill} style:height={fill ? undefined : `${height}px`} role="grid" aria-label={label} tabindex="0" {onkeydown}>
  {#if columns.length > 1 || columns[0].title}
    <div class="head" role="row">
      {#each columns as c, j (j)}
        <span role="columnheader" style:width={c.width ? `${c.width}px` : undefined} class:fill={!c.width}>{c.title}</span>
      {/each}
    </div>
  {/if}
  <div class="rows">
    {#each Array.from({ length: count }, (_, i) => i) as i (i)}
      <div
        class="row"
        class:on={selected === i}
        role="row"
        tabindex="-1"
        aria-selected={selected === i}
        onclick={() => (selected = i)}
        ondblclick={() => onopen?.(i)}
        onkeydown={() => undefined}
      >
        {#each columns as c, j (j)}
          <span role="gridcell" style:width={c.width ? `${c.width}px` : undefined} class:fill={!c.width}>{@render cell(i, j)}</span>
        {/each}
      </div>
    {/each}
  </div>
</div>

<style>
  .box {
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--field);
    border: 1px solid var(--input-border);
    overflow: hidden;
  }

  .fill-box {
    flex: 1;
    min-height: 0;
  }

  .head,
  .row {
    display: flex;
  }

  .head {
    height: 20px;
    align-items: center;
    background: var(--header);
    border-bottom: 1px solid var(--grid);
    flex: none;
  }

  .head span {
    padding: 0 5px;
    border-right: 1px solid var(--grid);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .rows {
    flex: 1;
    overflow: auto;
  }

  .row {
    height: 18px;
    align-items: center;
  }

  .row.on {
    background: var(--selected);
  }

  .box:not(:focus-within) .row.on {
    background: var(--selected-blur);
  }

  .row span {
    display: flex;
    align-items: center;
    padding: 0 5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-right: 1px solid var(--grid);
    height: 100%;
  }

  .fill {
    flex: 1;
    min-width: 0;
  }
</style>
