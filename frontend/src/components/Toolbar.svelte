<!-- The main window's toolbar: large icons with their names under them.
     Start Queue and Stop Queue have an arrow for picking another queue. -->
<script lang="ts" module>
  import type { ToolIconName } from './ToolIcon.svelte';
  import type { MenuEntry } from './menu';

  export type Tool = {
    icon: ToolIconName;
    label: string;
    hint: string;
    enabled: boolean;
    run: () => void;
    more?: MenuEntry[];
  };
</script>

<script lang="ts">
  import { t } from '../i18n';
  import ToolIcon from './ToolIcon.svelte';
  import PopupMenu from './PopupMenu.svelte';

  let { tools, small = false }: { tools: (Tool | 'separator')[]; small?: boolean } = $props();

  let menu: { items: MenuEntry[]; x: number; y: number } | null = $state(null);

  function openMore(e: MouseEvent, more: MenuEntry[]) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { items: more, x: r.left - 50, y: r.bottom };
  }
</script>

<div class="toolbar" class:small role="toolbar" aria-label={t('Toolbar')}>
  {#each tools as tool, i (tool === 'separator' ? `sep-${i}` : tool.icon)}
    {#if tool === 'separator'}
      <div class="sep" role="separator"></div>
    {:else}
    <div class="slot" class:wide={!!tool.more}>
      <button class="tool" disabled={!tool.enabled} title={t(tool.hint)} aria-label={t(tool.label)} onclick={tool.run}>
        <ToolIcon name={tool.icon} disabled={!tool.enabled} size={small ? 20 : 32} />
        {#if !small}<span class="name">{t(tool.label)}</span>{/if}
      </button>
      {#if tool.more}
        <button
          class="more"
          disabled={!tool.enabled || tool.more.length === 0}
          aria-label={t(tool.hint)}
          aria-haspopup="menu"
          onclick={(e) => tool.more && openMore(e, tool.more)}
        >
          <svg width="7" height="4" viewBox="0 0 7 4" aria-hidden="true"><path d="M0 0h7L3.5 4z" fill="currentColor" /></svg>
        </button>
      {/if}
    </div>
    {/if}
  {/each}
</div>
{#if menu}
  <PopupMenu items={menu.items} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: stretch;
    height: 62px;
    padding: 2px 6px 0;
    background: var(--face);
    flex: none;
    overflow: hidden;
  }

  .toolbar.small {
    height: 34px;
  }

  .small .slot {
    width: 34px;
  }

  .small .slot.wide {
    width: 46px;
  }

  .small .more {
    top: 8px;
  }

  .sep {
    width: 1px;
    margin: 6px 6px;
    background: var(--border);
    flex: none;
  }

  .slot {
    position: relative;
    display: flex;
    width: 68px;
    flex: none;
  }

  .slot.wide {
    width: 80px;
  }

  .tool {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    padding: 2px 2px 3px;
    background: none;
    border: 1px solid transparent;
    color: var(--fg);
  }

  .tool:hover:not(:disabled),
  .more:hover:not(:disabled) {
    background: var(--hover);
    border-color: var(--border);
  }

  .tool:active:not(:disabled) {
    background: var(--button-pressed);
  }

  .tool:disabled {
    color: var(--fg-disabled);
  }

  .name {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }

  .tool:disabled .name {
    font-weight: 400;
  }

  .more {
    position: absolute;
    right: 1px;
    top: 14px;
    width: 12px;
    height: 18px;
    padding: 0;
    background: none;
    border: 1px solid transparent;
    color: var(--fg-muted);
    display: grid;
    place-items: center;
  }
</style>
