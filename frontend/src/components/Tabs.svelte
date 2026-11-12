<!-- Tabs over a framed page, as a desktop property sheet draws them. Left
     and Right move between tabs. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    tabs,
    current = $bindable(),
    children,
  }: { tabs: { key: string; label: string }[]; current: string; children: Snippet } = $props();

  let buttons: HTMLButtonElement[] = $state([]);

  function onkeydown(e: KeyboardEvent, i: number) {
    const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const j = (i + step + tabs.length) % tabs.length;
    current = tabs[j].key;
    buttons[j]?.focus();
  }
</script>

<div class="tabs">
  <div class="strip" role="tablist">
    {#each tabs as tab, i (tab.key)}
      <button
        bind:this={buttons[i]}
        class="tab"
        class:on={current === tab.key}
        role="tab"
        aria-selected={current === tab.key}
        tabindex={current === tab.key ? 0 : -1}
        onclick={() => (current = tab.key)}
        onkeydown={(e) => onkeydown(e, i)}
      >
        {tab.label}
      </button>
    {/each}
  </div>
  <div class="page" role="tabpanel">
    {@render children()}
  </div>
</div>

<style>
  .tabs {
    display: flex;
    flex-direction: column;
  }

  .strip {
    display: flex;
    align-items: flex-end;
    height: 20px;
    padding-left: 2px;
  }

  .tab {
    position: relative;
    height: 18px;
    margin-right: -1px;
    padding: 0 6px;
    background: var(--face);
    border: 1px solid var(--frame);
    border-bottom: 0;
    color: var(--fg);
  }

  .tab.on {
    z-index: 1;
    height: 20px;
    margin: 0 -1px -1px -2px;
    padding: 0 8px;
    background: var(--face);
    border-color: var(--fg);
  }

  .page {
    flex: 1;
    border: 1px solid var(--frame);
    padding: 8px 14px;
  }
</style>
