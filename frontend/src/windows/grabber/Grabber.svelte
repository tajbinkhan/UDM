<!-- The site grabber: a four-step wizard, then the crawl's results. The
     Project menu opens, saves and starts projects; Options holds the crawl's
     pace. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import {
    closeWindow,
    grabAdd,
    grabNew,
    grabOpen,
    grabSave,
    grabSaveTemplate,
    grabStart,
    messageOf,
    setWindowSize,
    setWindowTitle,
  } from '../../ipc';
  import type { GrabberView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import MenuBar from '../../components/MenuBar.svelte';
  import Sheet from '../../components/Sheet.svelte';
  import { item, sep } from '../../components/menu';
  import GrabArt from './GrabArt.svelte';
  import Step1 from './Step1.svelte';
  import Step2 from './Step2.svelte';
  import Step3 from './Step3.svelte';
  import Step4 from './Step4.svelte';
  import Results from './Results.svelte';
  import ScheduleSheet from './ScheduleSheet.svelte';
  import { grab } from './grab.svelte';

  const view: GrabberView | null = $derived(projection.view?.window === 'grabber' ? projection.view.view : null);

  const titles = ['Set start page', 'Save files to', 'Set explorer filters', 'Set file filters'];
  let step = $state(1);
  let advanced = $state([false, false, false, false]);
  let name = $state('');
  /** Back in the wizard while results are shown. */
  let rewinding = $state(false);
  let sheet = $state<'open' | 'save' | 'template' | 'settings' | 'closing' | 'mustSave' | 'schedule' | null>(null);
  let afterSave: 'stay' | 'close' | 'schedule' = 'stay';
  let entry = $state('');
  let error = $state('');
  let busy = $state(false);
  let baseJson = '';
  let scheduleJson = '';

  // The draft starts from the shell's base until the user changes it; new
  // results tick every file afresh.
  $effect(() => {
    const v = view;
    if (!v) return;
    untrack(() => {
      const fresh = JSON.stringify(v.base);
      if (!grab.draft || JSON.stringify($state.snapshot(grab.draft)) === baseJson) grab.draft = $state.snapshot(v.base);
      baseJson = fresh;
      if (v.results !== grab.results) {
        grab.checked = [...v.checked];
        grab.results = v.results;
        rewinding = false;
      }
      if (v.project) name = name || v.project;
      // A project opened or saved brings its schedule.
      if (!grab.schedule || v.results !== grab.results || JSON.stringify(v.schedule) !== scheduleJson) grab.schedule = $state.snapshot(v.schedule);
      scheduleJson = JSON.stringify(v.schedule);
    });
  });

  const wizard = $derived(!view || view.phase === 'setup' || rewinding);

  $effect(() => {
    const size: [number, number] = wizard ? [615, advanced[step - 1] ? 541 : 388] : [778, 527];
    void setWindowSize(...size).catch(() => undefined);
  });

  $effect(() => {
    const title = wizard ? t('UDM Site Grabber. Step {n} --- {title}', { n: step, title: t(titles[step - 1]) }) : t('UDM Site Grabber');
    void setWindowTitle(title).catch(() => undefined);
  });

  async function run<T>(p: Promise<T>): Promise<T | undefined> {
    if (busy) return undefined;
    busy = true;
    error = '';
    try {
      return await p;
    } catch (e) {
      error = messageOf(e);
      return undefined;
    } finally {
      busy = false;
    }
  }

  async function next() {
    if (step < 4) {
      step += 1;
      return;
    }
    if (!grab.draft) return;
    const started = await run(grabStart($state.snapshot(grab.draft)).then(() => true));
    if (started) {
      rewinding = false;
      step = 1;
    }
  }

  async function finishClose() {
    const ticked = grab.checked.flatMap((on, i) => (on ? [i] : []));
    if (view?.addOnClose && view.phase === 'done' && ticked.length && view.queues[0]) {
      await grabAdd(ticked, view.queues[0].id, false).catch(() => undefined);
    }
    await closeWindow();
  }

  function close() {
    if (view?.unsaved && view.library) sheet = 'closing';
    else void finishClose();
  }

  async function save() {
    if (!grab.draft) return;
    if (!grab.schedule) return;
    const schedule = $state.snapshot(grab.schedule);
    const ok = await run(grabSave(entry, $state.snapshot(grab.draft), $state.snapshot(grab.checked), schedule).then(() => true));
    if (!ok) return;
    name = entry;
    sheet = afterSave === 'schedule' ? 'schedule' : null;
    if (afterSave === 'close') void finishClose();
  }

  function newProject() {
    void grabNew();
    if (view) grab.draft = $state.snapshot(view.base);
    name = '';
    step = 1;
    rewinding = false;
  }

  const menus = $derived([
    {
      label: '&Project',
      items: [
        item('&New project', newProject),
        item('&Open project...', view?.library ? () => ((entry = view?.recent[0] ?? ''), (sheet = 'open')) : undefined),
        item('&Save project', view?.library ? () => ((entry = name || t('project_1')), (afterSave = 'stay'), (sheet = 'save')) : undefined),
        item('Save as &template...', view?.library ? () => ((entry = name || t('project_1')), (sheet = 'template')) : undefined),
        sep,
        item('&Close', close),
      ],
    },
    {
      label: '&Options',
      items: [
        item('&Settings...', () => (sheet = 'settings')),
        item('S&chedule...', view?.library ? () => (sheet = view?.project ? 'schedule' : 'mustSave') : undefined),
      ],
    },
    { label: '&Help', items: [item('&Grabber Help', undefined)] },
  ]);
</script>

<Window onescape={close}>
  {#if view && grab.draft}
    <div class="grabber">
      <MenuBar {menus} />
      {#if wizard}
        <div class="wizard">
          <div class="art-col"><GrabArt /></div>
          <div class="step">
            {#if step === 1}
              <Step1 {view} bind:name advanced={advanced[0]} />
            {:else if step === 2}
              <Step2 {view} />
            {:else if step === 3}
              <Step3 advanced={advanced[2]} />
            {:else}
              <Step4 {view} advanced={advanced[3]} />
            {/if}
            {#if step !== 2}
              <div class="row adv">
                <span class="grow muted">{step === 1 ? t('Press Advanced button to enable manual login or to disable a logout page') : ''}</span>
                <button class="btn" onclick={() => (advanced[step - 1] = !advanced[step - 1])}>
                  {advanced[step - 1] ? t('Advanced <<') : t('Advanced >>')}
                </button>
              </div>
            {/if}
          </div>
        </div>
        {#if error}<p class="error-text" role="alert">{error}</p>{/if}
        <div class="foot">
          <button class="btn" disabled={step === 1} onclick={() => (step -= 1)}>{t('< Back')}</button>
          <button class="btn default" disabled={busy || (step === 4 && !view.connected)} onclick={() => void next()}>{t('Next >')}</button>
          <span class="space"></span>
          <button class="btn" onclick={close}>{t('Close')}</button>
        </div>
      {:else}
        <div class="results-wrap">
          <Results {view} onsettings={() => ((rewinding = true), (step = 1))} />
        </div>
      {/if}
    </div>

    {#if sheet === 'open'}
      {#if view.recent.length === 0}
        <Sheet title={t('UDM Grabber')} width={280} oncancel={() => (sheet = null)}>
          <p>{t('There are no saved projects of UDM grabber')}</p>
        </Sheet>
      {:else}
        <Sheet
          title={t('Open project')}
          width={320}
          oncancel={() => (sheet = null)}
          onok={async () => {
            const opened = await run(grabOpen(entry));
            if (opened) {
              grab.draft = opened.draft;
              name = opened.name;
              step = 1;
              sheet = null;
            }
          }}
        >
          <label for="projects">{t('Saved projects')}</label>
          <select id="projects" class="select" size="8" bind:value={entry}>
            {#each view.recent as p (p)}<option value={p}>{p}</option>{/each}
          </select>
          {#if error}<p class="error-text" role="alert">{error}</p>{/if}
        </Sheet>
      {/if}
    {:else if sheet === 'save'}
      <Sheet title={t('Saving the project of UDM Grabber')} width={330} okDisabled={!entry.trim() || busy} oncancel={() => (sheet = null)} onok={() => void save()}>
        <label for="save-name">{t('Save this project with the following name')}</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="save-name" class="text" bind:value={entry} autofocus />
        {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      </Sheet>
    {:else if sheet === 'template'}
      <Sheet
        title={t('Template Name')}
        width={290}
        okDisabled={!entry.trim() || busy}
        oncancel={() => (sheet = null)}
        onok={async () => {
          if (grab.draft && (await run(grabSaveTemplate(entry, $state.snapshot(grab.draft)).then(() => true)))) sheet = null;
        }}
      >
        <label for="template-name">{t('Please enter a name for the grabber template')}</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="template-name" class="text" bind:value={entry} autofocus />
        {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      </Sheet>
    {:else if sheet === 'settings'}
      <Sheet title={t('Settings of UDM Grabber')} width={440} oncancel={() => (sheet = null)} onok={() => (sheet = null)}>
        <div class="row"><input class="text num" type="number" min="1" max="10" bind:value={grab.draft.exploreAtOnce} aria-label={t('files to explore at the same time (1 to 10)')} /><span>{t('files to explore at the same time (1 to 10)')}</span></div>
        <div class="row"><input class="text num" type="number" min="1" max="10" bind:value={grab.draft.downloadAtOnce} aria-label={t('files to download at the same time (1 to 10)')} /><span>{t('files to download at the same time (1 to 10)')}</span></div>
        <p class="muted">{t('Please note that a web server may reject requests if you set a large number of files to explore (download) at the same time.')}</p>
        <div class="row"><input class="text num" type="number" min="1" max="10000" bind:value={grab.draft.maxPages} aria-label={t('pages at most')} /><span>{t('pages at most')}</span></div>
        <div class="row"><input class="text num" type="number" min="1" max="2000" bind:value={grab.draft.maxMb} aria-label={t('MB of pages at most')} /><span>{t('MB of pages at most')}</span></div>
        <div class="row"><input class="text num" type="number" min="0" max="60" bind:value={grab.draft.delaySecs} aria-label={t('seconds between pages')} /><span>{t('seconds between pages')}</span></div>
        <div class="rule"></div>
        <label class="check"><input type="checkbox" bind:checked={grab.draft.linkText} />{t('Use link texts as download descriptions when adding files to UDM main list')}</label>
        <label class="check"><input type="checkbox" bind:checked={grab.draft.renderScripts} />{t('Use Advanced web page processing')}</label>
      </Sheet>
    {:else if sheet === 'mustSave'}
      <Sheet title={t('Schedule the grabber project')} width={380} buttons={false} oncancel={() => (sheet = null)}>
        <p>{t('To use project scheduler you should save the project at first. Save the project now?')}</p>
        <div class="row yes-no">
          <button type="button" class="btn default" onclick={() => ((entry = name || t('project_1')), (afterSave = 'schedule'), (sheet = 'save'))}>{t('Yes')}</button>
          <button type="button" class="btn" onclick={() => (sheet = null)}>{t('No')}</button>
        </div>
      </Sheet>
    {:else if sheet === 'schedule' && grab.schedule}
      <ScheduleSheet
        start={grab.schedule}
        queues={view.queues}
        {busy}
        {error}
        oncancel={() => (sheet = null)}
        onok={async (s) => {
          if (!grab.draft) return;
          const ok = await run(grabSave(name || view.project, $state.snapshot(grab.draft), $state.snapshot(grab.checked), s).then(() => true));
          if (ok) {
            grab.schedule = s;
            sheet = null;
          }
        }}
      />
    {:else if sheet === 'closing'}
      <Sheet title={t('UDM Grabber')} width={290} buttons={false} oncancel={() => (sheet = null)}>
        <p>{t('Save the grabber project?')}</p>
        <div class="row yes-no">
          <button type="button" class="btn default" onclick={() => ((entry = name || t('project_1')), (afterSave = 'close'), (sheet = 'save'))}>{t('Yes')}</button>
          <button type="button" class="btn" onclick={() => void finishClose()}>{t('No')}</button>
        </div>
      </Sheet>
    {/if}
  {/if}
</Window>

<style>
  .grabber {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .wizard {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 12px;
    padding: 8px 12px 0 14px;
  }

  .art-col {
    flex: none;
    height: 260px;
  }

  .step {
    flex: 1;
    min-width: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .adv {
    margin-top: 4px;
  }

  .adv .btn {
    width: 108px;
  }

  .foot {
    display: flex;
    gap: 12px;
    padding: 8px 36px 10px 222px;
    margin: 0 12px;
    border-top: 1px solid var(--border);
  }

  .foot .btn {
    width: 88px;
  }

  .space {
    flex: 1;
  }

  .results-wrap {
    flex: 1;
    min-height: 0;
    display: flex;
    padding: 8px 10px 10px;
  }

  .num {
    width: 50px;
    text-align: right;
  }

  .rule {
    border-top: 1px solid var(--border);
  }

  .yes-no {
    justify-content: center;
    gap: 20px;
  }

  .yes-no .btn {
    width: 84px;
  }

  p {
    margin: 0;
  }

  /* Options' shared text area, reused by the steps. */
  :global(.opt-area) {
    width: 100%;
    padding: 3px 5px;
    resize: none;
    background: var(--input-bg);
    border: 1px solid var(--input-border);
    color: var(--fg);
    font: inherit;
  }
</style>
