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
    if (!el) return;
    el.showModal();
    // Focus what the dialog is for: its first text box or list, else the
    // button that closes it, as a Windows dialog focuses its default button.
    const field = el.querySelector<HTMLElement>(
      '.body input:not([type=checkbox]):not([type=radio]):not(:disabled), .body select:not(:disabled), .body textarea:not(:disabled)',
    );
    const button = el.querySelector<HTMLElement>('.buttons:not([hidden]) .btn.default:not(:disabled)');
    const first = el.querySelector<HTMLElement>('.body :is(button, input, select, textarea):not(:disabled)');
    (field ?? button ?? first)?.focus();
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
    <!-- Out of the Tab order, as a title bar's close button is; Escape
         does the same from the keyboard. -->
    <button class="x" type="button" tabindex="-1" aria-label={t('Close')} onclick={oncancel}>✕</button>
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
      <button class="btn" class:default={!onok} type="button" onclick={oncancel}>{onok ? t('Cancel') : t('Close')}</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    padding: 0;
    border: 1px solid var(--frame);
    border-radius: var(--radius-large);
    background: var(--face);
    color: var(--fg);
    box-shadow: 0 16px 32px var(--shadow);
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
    height: 32px;
    padding-block: 0;
    padding-inline: 12px 0;
    background: var(--bar);
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }

  .x {
    width: 40px;
    height: 32px;
    background: none;
    border: 0;
    border-radius: 0;
    border-start-end-radius: var(--radius-large);
    color: var(--fg);
    font-weight: 400;
  }

  .x:hover {
    background: var(--error);
    color: var(--on-error);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px 14px 14px;
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
