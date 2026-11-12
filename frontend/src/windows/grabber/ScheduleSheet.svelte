<!-- Schedule the grabber project: when it runs by itself, what a run does,
     and what happens when it is done. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import type { GrabScheduleDraft, QueueItem, Then } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';

  let {
    start,
    queues,
    busy,
    error,
    onok,
    oncancel,
  }: {
    start: GrabScheduleDraft;
    queues: QueueItem[];
    busy: boolean;
    error: string;
    onok: (s: GrabScheduleDraft) => void;
    oncancel: () => void;
  } = $props();
  // The sheet edits a copy of what it opened with.
  let s = $state(untrack(() => structuredClone($state.snapshot(start))));
  const days: { day: number; label: string }[] = [
    { day: 6, label: 'Sunday' },
    { day: 0, label: 'Monday' },
    { day: 1, label: 'Tuesday' },
    { day: 2, label: 'Wednesday' },
    { day: 3, label: 'Thursday' },
    { day: 4, label: 'Friday' },
    { day: 5, label: 'Saturday' },
  ];
  const turnOff = $derived(s.then === 'shutDown' || s.then === 'sleep' || s.then === 'hibernate');
  const thens: { value: Then; label: string }[] = [
    { value: 'nothing', label: 'Do nothing' },
    { value: 'exit', label: 'Exit UDM' },
    { value: 'sleep', label: 'Sleep' },
    { value: 'hibernate', label: 'Hibernate' },
    { value: 'shutDown', label: 'Shut down' },
  ];
</script>

<Sheet title={t('Schedule the grabber project')} width={440} okDisabled={busy} {oncancel} onok={() => onok($state.snapshot(s))}>
  <label class="check"><input type="checkbox" bind:checked={s.on} />{t('Run this project on a schedule')}</label>
  <div class="body" class:disabled={!s.on}>
    <div class="row">
      <span class="lead">{t('Start at')}</span>
      <input class="text time" type="time" bind:value={s.start} disabled={!s.on} aria-label={t('Start at')} />
      <label class="check"><input type="checkbox" bind:checked={s.wake} disabled={!s.on} />{t('Wake the computer to start it')}</label>
    </div>
    <div class="row">
      <label class="check lead"><input type="radio" checked={s.once} disabled={!s.on} onchange={() => (s.once = true)} />{t('Once at')}</label>
      <input class="text" type="date" bind:value={s.date} disabled={!s.on || !s.once} aria-label={t('Once at')} />
    </div>
    <div class="row top">
      <label class="check lead"><input type="radio" checked={!s.once} disabled={!s.on} onchange={() => (s.once = false)} />{t('Daily')}</label>
      <div class="days">
        {#each days as d (d.day)}
          <label class="check"
            ><input
              type="checkbox"
              checked={s.days.includes(d.day)}
              disabled={!s.on || s.once}
              onchange={(e) => (s.days = e.currentTarget.checked ? [...s.days, d.day] : s.days.filter((x) => x !== d.day))}
            />{t(d.label)}</label
          >
        {/each}
      </div>
    </div>
    <div class="row">
      <label class="check lead"><input type="checkbox" bind:checked={s.repeatOn} disabled={!s.on} />{t('Start again every')}</label>
      <input class="text num" type="number" min="0" max="99" bind:value={s.repeatHours} disabled={!s.on || !s.repeatOn} aria-label={t('hours')} />
      <span>{t('hours')}</span>
      <input class="text num" type="number" min="0" max="59" bind:value={s.repeatMinutes} disabled={!s.on || !s.repeatOn} aria-label={t('min')} />
      <span>{t('min')}</span>
    </div>
    <div class="row">
      <label class="check lead"><input type="checkbox" bind:checked={s.stopOn} disabled={!s.on} />{t('Stop at')}</label>
      <input class="text time" type="time" bind:value={s.stop} disabled={!s.on || !s.stopOn} aria-label={t('Stop at')} />
    </div>
  </div>
  <div class="rule"></div>
  <label for="run">{t('Each run')}</label>
  <select id="run" class="select" bind:value={s.run}>
    <option value="explore">{t('Explores the site and keeps what it finds')}</option>
    <option value="exploreAndDownload">{t('Explores the site and downloads the files earlier runs did not find')}</option>
    <option value="downloadChecked">{t('Downloads the files checked in the project')}</option>
  </select>
  <div class="row">
    <label for="queue">{t('in the queue')}</label>
    <select id="queue" class="select grow" value={s.queue} onchange={(e) => (s.queue = Number(e.currentTarget.value))}>
      {#each queues as q (q.id)}<option value={q.id}>{q.name}</option>{/each}
    </select>
  </div>
  <label class="check"><input type="checkbox" bind:checked={s.sync} />{t('Download again what earlier runs downloaded, as a synchronization queue does')}</label>
  <div class="rule"></div>
  <label class="check"><input type="checkbox" bind:checked={s.openOn} />{t('Open the following file when done:')}</label>
  <input class="text" bind:value={s.openFile} disabled={!s.openOn} aria-label={t('Open the following file when done:')} spellcheck="false" />
  <div class="row">
    <label for="then">{t('When done')}</label>
    <select id="then" class="select grow" bind:value={s.then}>
      {#each thens as th (th.value)}<option value={th.value}>{t(th.label)}</option>{/each}
    </select>
  </div>
  <label class="check"><input type="checkbox" bind:checked={s.forceClose} disabled={!turnOff} />{t('Force processes to terminate')}</label>
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</Sheet>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-left: 18px;
  }

  .lead {
    width: 110px;
  }

  .top {
    align-items: flex-start;
  }

  .days {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 2px 8px;
  }

  .time {
    width: 96px;
  }

  .num {
    width: 44px;
    text-align: right;
  }

  .rule {
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }

  input[type='radio'] {
    margin: 0 5px 0 0;
    accent-color: var(--focus);
  }

  p {
    margin: 0;
  }
</style>
