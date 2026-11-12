<!-- A dialog inside a window, such as Options' Site login: modal, with a
     title bar of its own. Escape or Cancel closes it. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '../i18n';

  let {
    title,
    width = 380,
    onok,
    oncancel,
    okDisabled = false,
    buttons = true,
    children,
  }: {
    title: string;
    width?: number;
    onok?: () => void;
    oncancel: () => void;
    okDisabled?: boolean;
    /** False for a question that brings its own buttons, such as Yes and No. */
    buttons?: boolean;
    children: Snippet;
  } = $props();

  let el: HTMLDialogElement | undefined = $state();

  $effect(() => {
    el?.showModal();
  });
</script>

<dialog
  bind:this={el}
  style:width="{width}px"
  aria-label={title}
  oncancel={(e) => {
    e.preventDefault();
    oncancel();
  }}
>
  <div class="bar">
    <span>{title}</span>
    <button class="x" aria-label={t('Close')} onclick={oncancel}>✕</button>
  </div>
  <form
    class="body"
    onsubmit={(e) => {
      e.preventDefault();
      if (!okDisabled) onok?.();
    }}
  >
    {@render children()}
    <div class="buttons" hidden={!buttons}>
      {#if onok}<button class="btn default" type="submit" disabled={okDisabled}>{t('OK')}</button>{/if}
      <button class="btn" type="button" onclick={oncancel}>{onok ? t('Cancel') : t('Close')}</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    padding: 0;
    border: 1px solid var(--frame);
    background: var(--face);
    color: var(--fg);
    box-shadow: 0 4px 16px var(--shadow);
    max-height: calc(100vh - 16px);
    max-width: calc(100vw - 16px);
    overflow: auto;
  }

  dialog::backdrop {
    background: var(--shadow);
  }

  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 26px;
    padding: 0 0 0 10px;
    background: var(--bar);
    border-bottom: 1px solid var(--border);
  }

  .x {
    width: 34px;
    height: 26px;
    background: none;
    border: 0;
    color: var(--fg);
  }

  .x:hover {
    background: var(--error);
    color: var(--field);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px 12px;
  }

  .buttons {
    display: flex;
    justify-content: center;
    gap: 40px;
    margin-top: 8px;
  }

  .buttons .btn {
    width: 90px;
  }
</style>
