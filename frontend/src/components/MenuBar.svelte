<!-- The main window's menu bar. A click opens a menu; while one is open,
     moving over another opens it. F10 or Alt with an underlined letter
     opens a menu from the keyboard. -->
<script lang="ts">
  import { mnemonic, plain, t } from '../i18n';
  import type { MenuEntry } from './menu';
  import PopupMenu from './PopupMenu.svelte';

  let { menus }: { menus: { label: string; items: MenuEntry[] }[] } = $props();

  let open = $state(-1);
  let at = $state({ x: 0, y: 0 });
  let buttons: HTMLButtonElement[] = $state([]);

  function show(i: number) {
    const r = buttons[i]?.getBoundingClientRect();
    if (!r) return;
    at = { x: r.left, y: r.bottom };
    open = i;
  }

  function onwindowkeydown(e: KeyboardEvent) {
    if (e.key === 'F10' && !e.shiftKey) {
      e.preventDefault();
      buttons[0]?.focus();
      return;
    }
    if (e.altKey && e.key.length === 1) {
      const i = menus.findIndex((m) => mnemonic(m.label).key.toLowerCase() === e.key.toLowerCase());
      if (i >= 0) {
        e.preventDefault();
        show(i);
      }
    }
  }

  function onbarkeydown(e: KeyboardEvent, i: number) {
    if (e.key === 'ArrowRight') buttons[(i + 1) % menus.length]?.focus();
    else if (e.key === 'ArrowLeft') buttons[(i - 1 + menus.length) % menus.length]?.focus();
    else if (e.key === 'ArrowDown') show(i);
    else return;
    e.preventDefault();
  }
</script>

<svelte:window onkeydown={onwindowkeydown} />

<div class="bar" role="menubar" aria-label={t('Menu')}>
  {#each menus as m, i (m.label)}
    {@const parts = mnemonic(m.label)}
    <button
      bind:this={buttons[i]}
      class="top"
      class:open={open === i}
      role="menuitem"
      aria-haspopup="menu"
      aria-expanded={open === i}
      onclick={() => (open === i ? (open = -1) : show(i))}
      onmouseenter={() => open >= 0 && open !== i && show(i)}
      onkeydown={(e) => onbarkeydown(e, i)}
    >
      {parts.before}<span class="u">{parts.key}</span>{parts.after}
    </button>
  {/each}
</div>
{#if open >= 0}
  <!-- A new menu for each name, so it places itself under the name it
       belongs to when the mouse slides from one name to the next. -->
  {#key open}
    <PopupMenu items={menus[open].items} x={at.x} y={at.y} label={plain(menus[open].label)} onclose={() => (open = -1)} />
  {/key}
{/if}

<style>
  .bar {
    display: flex;
    gap: 2px;
    height: 28px;
    padding: 3px 6px;
    background: var(--bar);
    flex: none;
  }

  .top {
    position: relative;
    z-index: 52;
    padding: 0 9px;
    background: none;
    border: 0;
    border-radius: var(--radius);
    color: var(--fg);
  }


  .top:hover,
  .top.open {
    background: var(--menu-hover);
  }
</style>
