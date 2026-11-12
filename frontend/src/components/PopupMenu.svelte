<!-- A popup menu at a point: the menu bar's menus, the list's and the
     categories' right-click menus. Keyboard: Up and Down move, Right opens a
     submenu, Left or Escape closes, Enter or Space picks. -->
<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { mnemonic, plain } from '../i18n';
  import type { MenuEntry } from './menu';
  import PopupMenu from './PopupMenu.svelte';

  let {
    items,
    x,
    y,
    onclose,
    nested = false,
    label = '',
  }: {
    items: MenuEntry[];
    x: number;
    y: number;
    onclose: (picked: boolean) => void;
    nested?: boolean;
    label?: string;
  } = $props();

  let el: HTMLDivElement | undefined = $state();
  let left = $state(0);
  let top = $state(0);
  let active = $state(-1);
  let open = $state(-1);
  let subAt = $state({ x: 0, y: 0 });

  const usable = (i: number) => {
    const e = items[i];
    return e && e.kind !== 'sep' && !e.disabled;
  };

  onMount(async () => {
    left = x;
    top = y;
    await tick();
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (left + r.width > window.innerWidth) left = nested ? Math.max(0, x - r.width - 200) : Math.max(0, window.innerWidth - r.width - 2);
    if (top + r.height > window.innerHeight) top = Math.max(0, window.innerHeight - r.height - 2);
    el.focus();
  });

  function pick(i: number, row?: HTMLElement) {
    const e = items[i];
    if (!e || e.kind === 'sep' || e.disabled) return;
    if (e.kind === 'sub') {
      const r = (row ?? el?.querySelectorAll<HTMLElement>('[data-row]')[i])?.getBoundingClientRect();
      if (r) subAt = { x: r.right - 2, y: r.top - 3 };
      open = i;
      return;
    }
    onclose(true);
    e.run?.();
  }

  function move(step: number) {
    for (let n = 1; n <= items.length; n++) {
      const i = (active + step * n + items.length * 2) % items.length;
      if (usable(i)) {
        active = i;
        return;
      }
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (open >= 0) return;
    const keys: Record<string, () => void> = {
      ArrowDown: () => move(1),
      ArrowUp: () => move(-1),
      ArrowRight: () => {
        if (items[active]?.kind === 'sub') pick(active);
      },
      ArrowLeft: () => nested && onclose(false),
      Escape: () => onclose(false),
      Enter: () => pick(active),
      ' ': () => pick(active),
    };
    const run = keys[e.key];
    if (run) {
      e.preventDefault();
      e.stopPropagation();
      run();
      return;
    }
    const letter = e.key.toLowerCase();
    const i = items.findIndex((it) => it.kind !== 'sep' && !it.disabled && mnemonic(it.label).key.toLowerCase() === letter);
    if (i >= 0) {
      e.preventDefault();
      active = i;
      pick(i);
    }
  }
</script>

{#if !nested}
  <div class="backdrop" role="presentation" onmousedown={() => onclose(false)} oncontextmenu={(e) => { e.preventDefault(); onclose(false); }}></div>
{/if}
<div
  class="menu"
  role="menu"
  aria-label={label}
  tabindex="-1"
  bind:this={el}
  style:left="{left}px"
  style:top="{top}px"
  {onkeydown}
>
  {#each items as e, i (i)}
    {#if e.kind === 'sep'}
      <div class="sep" role="separator"></div>
    {:else}
      {@const m = mnemonic(e.label)}
      <div
        data-row
        class="entry"
        class:active={active === i}
        class:disabled={e.disabled}
        role={e.kind === 'item' && e.checked !== undefined ? 'menuitemcheckbox' : 'menuitem'}
        aria-checked={e.kind === 'item' && e.checked !== undefined ? e.checked : undefined}
        aria-disabled={e.disabled}
        aria-haspopup={e.kind === 'sub' ? 'menu' : undefined}
        aria-label={plain(e.label)}
        tabindex="-1"
        onmouseenter={(ev) => {
          active = i;
          if (e.kind === 'sub' && !e.disabled) pick(i, ev.currentTarget as HTMLElement);
          else open = -1;
        }}
        onclick={(ev) => pick(i, ev.currentTarget as HTMLElement)}
        onkeydown={() => undefined}
      >
        <span class="mark">{#if e.kind === 'item' && e.checked}✓{/if}</span>
        <span class="label">{m.before}<span class="u">{m.key}</span>{m.after}</span>
        {#if e.kind === 'item' && e.shortcut}<span class="shortcut">{e.shortcut}</span>{/if}
        {#if e.kind === 'sub'}<span class="arrow" aria-hidden="true">›</span>{/if}
      </div>
    {/if}
  {/each}
</div>
{#if open >= 0}
  {@const s = items[open]}
  {#if s && s.kind === 'sub'}
    <PopupMenu
      nested
      items={s.items}
      x={subAt.x}
      y={subAt.y}
      label={plain(s.label)}
      onclose={(picked) => {
        open = -1;
        if (picked) onclose(true);
        else el?.focus();
      }}
    />
  {/if}
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
  }

  .menu {
    position: fixed;
    z-index: 51;
    min-width: 180px;
    padding: 3px 2px;
    background: var(--menu);
    border: 1px solid var(--border);
    box-shadow: 2px 3px 6px var(--shadow);
    color: var(--fg);
  }

  .entry {
    display: flex;
    align-items: center;
    height: 22px;
    padding: 0 10px 0 0;
    white-space: nowrap;
  }

  .entry.active:not(.disabled) {
    background: var(--menu-hover);
  }

  .entry.disabled {
    color: var(--fg-disabled);
  }

  .mark {
    width: 30px;
    text-align: center;
    flex: none;
  }

  .label {
    flex: 1;
    padding-right: 24px;
  }

  .shortcut {
    padding-left: 12px;
  }

  .arrow {
    font-size: 15px;
    line-height: 1;
  }

  .sep {
    height: 1px;
    margin: 4px 1px 4px 30px;
    background: var(--border);
  }
</style>
