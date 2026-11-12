<!-- Options: the settings in IDM's two rows of tabs. The pages edit a copy;
     OK sends it to the shell, which checks it and saves it. -->
<script lang="ts">
  import { closeWindow, messageOf, saveOptions } from '../../ipc';
  import type { OptionsView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import { editing } from './draft.svelte';
  import General from './General.svelte';
  import FileTypes from './FileTypes.svelte';
  import SaveTo from './SaveTo.svelte';
  import Downloads from './Downloads.svelte';
  import Connection from './Connection.svelte';
  import Proxy from './Proxy.svelte';
  import Logins from './Logins.svelte';
  import Sounds from './Sounds.svelte';

  const view: OptionsView | null = $derived(projection.view?.window === 'options' ? projection.view.view : null);

  type Page = 'general' | 'types' | 'saveTo' | 'downloads' | 'connection' | 'proxy' | 'logins' | 'sounds';
  const rows: { key: Page; label: string }[][] = [
    [
      { key: 'proxy', label: 'Proxy / Socks' },
      { key: 'logins', label: 'Sites Logins' },
      { key: 'sounds', label: 'Sounds' },
    ],
    [
      { key: 'general', label: 'General' },
      { key: 'types', label: 'File types' },
      { key: 'saveTo', label: 'Save to' },
      { key: 'downloads', label: 'Downloads' },
      { key: 'connection', label: 'Connection' },
    ],
  ];

  let page = $state<Page>('general');
  let error = $state('');
  let busy = $state(false);
  // The row holding the open page sits next to the page, as IDM's do.
  const ordered = $derived(rows[0].some((r) => r.key === page) ? [rows[1], rows[0]] : rows);
  const all = rows.flat();

  $effect(() => {
    if (view && !editing.draft) editing.draft = structuredClone($state.snapshot(view.draft));
  });

  async function ok() {
    if (!editing.draft || busy) return;
    busy = true;
    error = '';
    try {
      await saveOptions($state.snapshot(editing.draft));
    } catch (e) {
      error = messageOf(e);
    } finally {
      busy = false;
    }
  }

  function onkeydown(e: KeyboardEvent, key: Page) {
    const i = all.findIndex((x) => x.key === key);
    const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    page = all[(i + step + all.length) % all.length].key;
    queueMicrotask(() => document.getElementById(`tab-${page}`)?.focus());
  }
</script>

<Window onescape={() => void closeWindow()}>
  {#if view && editing.draft}
    <div class="options">
      <div class="tabs" role="tablist">
        {#each ordered as row, r (r)}
          <div class="tabrow" class:front={r === 1}>
            {#each row as tab (tab.key)}
              <button
                id="tab-{tab.key}"
                class="tab"
                class:on={page === tab.key}
                role="tab"
                aria-selected={page === tab.key}
                aria-controls="page"
                tabindex={page === tab.key ? 0 : -1}
                onclick={() => (page = tab.key)}
                onkeydown={(e) => onkeydown(e, tab.key)}
              >
                {t(tab.label)}
              </button>
            {/each}
          </div>
        {/each}
      </div>
      <div class="page" id="page" role="tabpanel">
        {#if page === 'general'}
          <General {view} />
        {:else if page === 'types'}
          <FileTypes {view} />
        {:else if page === 'saveTo'}
          <SaveTo />
        {:else if page === 'downloads'}
          <Downloads />
        {:else if page === 'connection'}
          <Connection {view} />
        {:else if page === 'proxy'}
          <Proxy />
        {:else if page === 'logins'}
          <Logins />
        {:else}
          <Sounds />
        {/if}
      </div>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <div class="foot">
        <button class="btn default" disabled={busy} onclick={() => void ok()}>{t('OK')}</button>
        <button class="btn" onclick={() => void closeWindow()}>{t('Cancel')}</button>
      </div>
    </div>
  {/if}
</Window>

<style>
  .options {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 8px 10px 10px;
  }

  .tabs {
    display: flex;
    flex-direction: column;
    flex: none;
  }

  .tabrow {
    display: flex;
  }

  .tab {
    flex: 1 1 auto;
    height: 18px;
    padding: 0 6px;
    margin-right: -1px;
    background: var(--face);
    border: 1px solid var(--frame);
    border-bottom: 0;
    color: var(--fg);
    white-space: nowrap;
  }

  .tabrow:not(.front) .tab {
    margin-left: 0;
  }

  .tab.on {
    position: relative;
    z-index: 1;
    background: var(--list);
    height: 20px;
    margin-top: -2px;
    margin-bottom: -1px;
  }

  .tabrow.front .tab:not(.on) {
    background: var(--face);
  }

  .page {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 8px 10px;
    background: var(--list);
    border: 1px solid var(--frame);
  }

  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 10px;
  }

  .foot .btn {
    width: 74px;
  }

  p {
    margin: 6px 0 0;
  }

  /* What the pages share. */
  :global(.opt-col) {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  :global(.opt-indent) {
    padding-left: 18px;
  }

  :global(.opt-rule) {
    height: 0;
    margin: 2px 0;
    border-top: 1px solid var(--border);
  }

  :global(.opt-area) {
    width: 100%;
    min-height: 58px;
    padding: 3px 5px;
    resize: none;
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    color: var(--fg);
    font: inherit;
    user-select: text;
  }

  :global(.opt-right) {
    align-self: flex-end;
  }

  :global(.opt-note) {
    margin: 0;
    color: var(--fg-muted);
  }
</style>
