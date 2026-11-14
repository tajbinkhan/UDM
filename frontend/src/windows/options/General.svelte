<!-- Options > General: starting with the system, the tray, the clipboard,
     the browsers' keys, menus and download panels, the theme and language. -->
<script lang="ts">
  import { openThemesFolder } from '../../ipc';
  import type { KeysDraft, OptionsView } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  let { view }: { view: OptionsView } = $props();
  // The pages are drawn only while the draft exists.
  const d = $derived(editing.draft!);

  let sheet = $state<'keys' | 'menus' | 'panels' | null>(null);
  let menuTab = $state<'chromium' | 'firefox'>('chromium');
  let keys = $state<{ skip: KeysDraft; force: KeysDraft; left: boolean; pages: boolean } | null>(null);
  let menus = $state<{ chromium: string[]; firefox: string[] } | null>(null);
  let panels = $state<{ button: boolean; mini: boolean; types: { ext: string; on: boolean }[]; auto: boolean; note: boolean } | null>(null);
  let newType = $state('');

  function openKeys() {
    keys = { skip: { ...d.skipKey }, force: { ...d.forceKey }, left: d.keysLeftClick, pages: d.forceSkipsPages };
    sheet = 'keys';
  }

  function openMenus() {
    menus = { chromium: [...d.menuChromium], firefox: [...d.menuFirefox] };
    sheet = 'menus';
  }

  function openPanels() {
    panels = { button: d.videoButton, mini: d.videoMini, types: d.videoTypes.map((v) => ({ ...v })), auto: d.videoAutoCapture, note: d.videoProtectedNote };
    sheet = 'panels';
  }

  function toggle(list: string[], id: string, on: boolean): string[] {
    return on ? [...list.filter((x) => x !== id), id] : list.filter((x) => x !== id);
  }

  const keyNames: { key: keyof KeysDraft; label: string }[] = [
    { key: 'alt', label: 'Alt' },
    { key: 'shift', label: 'Shift' },
    { key: 'ctrl', label: 'Ctrl' },
  ];
</script>

<PageHead icon="general" title={t('General settings')} />
<div class="opt-col">
  <label class="check" title={view.portable ? t('A portable copy does not start when you sign in, so it leaves the computer as it found it.') : undefined}
    ><input type="checkbox" bind:checked={d.startWithSystem} disabled={view.portable} />{t('Launch UDM on startup')}</label
  >
  <label class="check"><input type="checkbox" bind:checked={d.trayIcon} />{t('Show the UDM icon in the tray')}</label>
  <label class="check opt-indent"><input type="checkbox" bind:checked={d.closeToTray} disabled={!d.trayIcon} />{t('Closing the window puts it in the tray')}</label>
  <label class="check opt-indent"><input type="checkbox" bind:checked={d.minimizeToTray} disabled={!d.trayIcon} />{t('Minimizing the window puts it in the tray')}</label>
  <label class="check"><input type="checkbox" bind:checked={d.keepAwake} />{t('Keep the computer awake while downloading')}</label>
  <label class="check"><input type="checkbox" bind:checked={d.checkUpdates} />{t('Check for updates once a week')}</label>
  <div class="opt-rule"></div>
  <label class="check"><input type="checkbox" bind:checked={d.clipboardMonitor} />{t('Offer to download links copied to the clipboard')}</label>
  <label class="check opt-indent"><input type="checkbox" bind:checked={d.clipboardStart} disabled={!d.clipboardMonitor} />{t('Start them without asking')}</label>
  <label class="check"><input type="checkbox" bind:checked={d.capturePrivate} />{t('Take over downloads in private windows')}</label>
  <div class="row buttons-row">
    <span class="grow">{t('Browser integration')}</span>
    <button class="btn" onclick={openKeys}>{t('Keys...')}</button>
    <button class="btn" onclick={openMenus}>{t('Context menu...')}</button>
    <button class="btn" onclick={openPanels}>{t('Download panels...')}</button>
  </div>
  <div class="opt-rule"></div>
  <div class="grid">
    <label for="theme">{t('Theme')}</label>
    <div class="row">
      <select id="theme" class="select grow" bind:value={d.theme}>
        {#each view.themes as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
      </select>
      <button class="btn" title={t('A .theme file put there is listed here within seconds')} onclick={() => void openThemesFolder()}
        >{t('Open themes folder')}</button
      >
    </div>
    <label for="language">{t('Language')}</label>
    <select id="language" class="select" bind:value={d.language}>
      {#each view.languages as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
    </select>
  </div>
  {#each view.themeNotes as note, i (i)}<p class="error-text">{note}</p>{/each}
</div>

{#if sheet === 'keys' && keys}
  <Sheet
    title={t('Using special keys')}
    width={440}
    oncancel={() => (sheet = null)}
    onok={() => {
      if (!keys) return;
      d.skipKey = keys.skip;
      d.forceKey = keys.force;
      d.keysLeftClick = keys.left;
      d.forceSkipsPages = keys.pages;
      sheet = null;
    }}
  >
    <label class="check"><input type="checkbox" bind:checked={keys.skip.on} />{t('Use the following key(s) to prevent downloading with UDM for any links:')}</label>
    <div class="keys">
      {#each keyNames as k (k.key)}
        <label class="check"><input type="checkbox" bind:checked={keys.skip[k.key]} disabled={!keys.skip.on} />{k.label}</label>
      {/each}
    </div>
    <div class="opt-rule"></div>
    <label class="check"><input type="checkbox" bind:checked={keys.force.on} />{t('Use the following key(s) to force downloading with UDM for any links:')}</label>
    <div class="keys">
      {#each keyNames as k (k.key)}
        <label class="check"><input type="checkbox" bind:checked={keys.force[k.key]} disabled={!keys.force.on} />{k.label}</label>
      {/each}
      <label class="check"><input type="checkbox" bind:checked={keys.force.insert} disabled={!keys.force.on} />Ins</label>
    </div>
    <p class="opt-note">{t('Some sites open a page that starts the download a few seconds later. To take over such links, also check the left mouse button:')}</p>
    <label class="check opt-indent"><input type="checkbox" bind:checked={keys.left} disabled={!keys.force.on} />{t('Check for a left click on a link with the special key(s) held to force downloading the link')}</label>
    <p class="opt-note">{t('While a key is held, the browser also asks for scripts, pages and pictures. UDM can leave those to the browser:')}</p>
    <label class="check opt-indent"><input type="checkbox" bind:checked={keys.pages} disabled={!keys.force.on} />{t('While holding a special key, do not take over web pages, pictures and scripts')}</label>
  </Sheet>
{:else if sheet === 'menus' && menus}
  <Sheet
    title={t('Customize UDM menu items in context menu of browsers')}
    width={420}
    oncancel={() => (sheet = null)}
    onok={() => {
      if (!menus) return;
      d.menuChromium = menus.chromium;
      d.menuFirefox = menus.firefox;
      sheet = null;
    }}
  >
    <div class="mini-tabs" role="tablist">
      <button type="button" role="tab" class:on={menuTab === 'chromium'} aria-selected={menuTab === 'chromium'} onclick={() => (menuTab = 'chromium')}
        >{t('Chrome, Edge and others')}</button
      >
      <button type="button" role="tab" class:on={menuTab === 'firefox'} aria-selected={menuTab === 'firefox'} onclick={() => (menuTab = 'firefox')}
        >{t('Firefox')}</button
      >
    </div>
    <div class="mini-page">
      <p class="opt-note">{t('Selected menu items will appear in browser context menu:')}</p>
      {#each view.menuItems as m (m.value)}
        <label class="check">
          <input
            type="checkbox"
            checked={menus[menuTab].includes(m.value)}
            onchange={(e) => menus && (menus[menuTab] = toggle(menus[menuTab], m.value, e.currentTarget.checked))}
          />{m.label}
        </label>
      {/each}
    </div>
  </Sheet>
{:else if sheet === 'panels' && panels}
  <Sheet
    title={t('Customize UDM download panels in browsers')}
    width={460}
    oncancel={() => (sheet = null)}
    onok={() => {
      if (!panels) return;
      d.videoButton = panels.button;
      d.videoMini = panels.mini;
      d.videoTypes = panels.types;
      d.videoAutoCapture = panels.auto;
      d.videoProtectedNote = panels.note;
      sheet = null;
    }}
  >
    <label class="check"><input type="checkbox" bind:checked={panels.button} />{t('Show a download panel on web players')}</label>
    <div class="row">
      <span>{t('Panel view:')}</span>
      <label class="check"><input type="radio" name="view" checked={!panels.mini} onchange={() => panels && (panels.mini = false)} />{t('Full mode')}</label>
      <label class="check"><input type="radio" name="view" checked={panels.mini} onchange={() => panels && (panels.mini = true)} />{t('Mini mode')}</label>
    </div>
    <p class="opt-note">{t('Show UDM download panel for the following file types:')}</p>
    <div class="row top">
      <div class="types">
        {#each panels.types as v, i (v.ext + i)}
          <label class="check"><input type="checkbox" bind:checked={v.on} />{v.ext.toUpperCase()}</label>
        {/each}
      </div>
      <div class="opt-col side">
        <div class="row">
          <input class="text grow" bind:value={newType} aria-label={t('File type')} />
          <button
            type="button"
            class="btn small"
            disabled={!newType.trim()}
            onclick={() => {
              if (!panels) return;
              panels.types = [...panels.types, { ext: newType.trim().replace(/^\./, '').toLowerCase(), on: true }];
              newType = '';
            }}>{t('Add')}</button
          >
        </div>
        <button type="button" class="btn" onclick={() => panels && panels.types.forEach((v) => (v.on = true))}>{t('Check All')}</button>
        <button type="button" class="btn" onclick={() => panels && panels.types.forEach((v) => (v.on = false))}>{t('Clear All')}</button>
      </div>
    </div>
    <label class="check"><input type="checkbox" checked={!panels.auto} onchange={(e) => panels && (panels.auto = !e.currentTarget.checked)} />{t("Don't capture downloads from web-players automatically")}</label>
    <label class="check"><input type="checkbox" bind:checked={panels.note} />{t('Show download panel for protected content which UDM may not download')}</label>
  </Sheet>
{/if}

<style>
  .buttons-row .btn {
    min-width: 0;
    padding: 0 8px;
  }

  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 6px 10px;
  }

  .keys {
    display: flex;
    gap: 22px;
    padding-inline-start: 120px;
  }

  .mini-tabs {
    display: flex;
    gap: 0;
  }

  .mini-tabs button {
    height: 20px;
    padding: 0 8px;
    margin-inline-end: -1px;
    background: var(--face);
    border: 1px solid var(--frame);
    border-bottom: 0;
    color: var(--fg);
  }

  .mini-tabs button.on {
    background: var(--bg);
  }

  .mini-page {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    background: var(--bg);
    border: 1px solid var(--frame);
  }

  .top {
    align-items: flex-start;
  }

  .types {
    flex: 1;
    height: 150px;
    overflow: auto;
    padding: 4px 6px;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    align-content: start;
    gap: 3px;
    background: var(--field);
    border: 1px solid var(--input-border);
  }

  .side {
    width: 140px;
  }

  input[type='radio'] {
    margin-block: 0;
    margin-inline: 10px 4px;
    accent-color: var(--focus);
  }
</style>
