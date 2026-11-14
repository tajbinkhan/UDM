<!-- The Scheduler's Schedule tab for one queue. -->
<script lang="ts">
  import { messageOf, pickPath } from '../../ipc';
  import type { QueueDraft, Then, WakeWarning } from '../../ipc/types';
  import { t } from '../../i18n';

  let {
    q = $bindable(),
    maxRetries,
    wakeWarning,
  }: { q: QueueDraft; maxRetries: number; wakeWarning: WakeWarning | null } = $props();

  let off = $state<Then>(q.then === 'sleep' || q.then === 'hibernate' ? q.then : 'shutDown');
  let error = $state('');
  const turnOff = $derived(q.then === 'shutDown' || q.then === 'sleep' || q.then === 'hibernate');
  // The days in the columns IDM shows them in; 0 is Monday.
  const columns: { day: number; label: string }[][] = [
    [
      { day: 6, label: 'Sunday' },
      { day: 0, label: 'Monday' },
      { day: 1, label: 'Tuesday' },
    ],
    [
      { day: 2, label: 'Wednesday' },
      { day: 3, label: 'Thursday' },
      { day: 4, label: 'Friday' },
    ],
    [{ day: 5, label: 'Saturday' }],
  ];
  const daysOn = $derived(q.startOn && (q.sync || !q.once));

  function toggleDay(day: number, on: boolean) {
    q.days = on ? [...q.days.filter((d) => d !== day), day] : q.days.filter((d) => d !== day);
  }

  async function browse() {
    error = '';
    try {
      const p = await pickPath('program', q.openFile);
      if (p) q.openFile = p;
    } catch (e) {
      error = messageOf(e);
    }
  }
</script>

<div class="tab">
  <div class="row kinds">
    <label class="check"><input type="radio" checked={!q.sync} onchange={() => (q.sync = false)} />{t('One-time downloading')}</label>
    <label class="check"
      ><input type="radio" checked={q.sync} disabled={q.main} onchange={() => (q.sync = true)} />{t('Periodic synchronization')}</label
    >
  </div>
  <div class="rule"></div>
  <label class="check"><input type="checkbox" bind:checked={q.startOnLaunch} />{t('Start download on UDM startup')}</label>
  <div class="rule"></div>
  <div class="row">
    <label class="check lead"><input type="checkbox" bind:checked={q.startOn} />{t('Start download at')}</label>
    <input class="text time" type="time" bind:value={q.start} disabled={!q.startOn} aria-label={t('Start download at')} />
  </div>
  {#if wakeWarning && q.startOn && q.wake}
    <p class="error-text indent" role="status" title={wakeWarning.hint}>{wakeWarning.text}</p>
  {/if}
  {#if q.sync}
    <div class="row indent">
      <label class="check lead2"><input type="checkbox" bind:checked={q.repeatOn} disabled={!q.startOn} />{t('Start again every')}</label>
      <input class="text num" type="number" min="0" max="99" bind:value={q.repeatHours} disabled={!q.startOn || !q.repeatOn} aria-label={t('hours')} />
      <span class:disabled={!q.startOn || !q.repeatOn}>{t('hours')}</span>
      <input class="text num" type="number" min="0" max="59" bind:value={q.repeatMinutes} disabled={!q.startOn || !q.repeatOn} aria-label={t('min')} />
      <span class:disabled={!q.startOn || !q.repeatOn}>{t('min')}</span>
    </div>
  {:else}
    <div class="row indent">
      <label class="check lead2"><input type="radio" checked={q.once} disabled={!q.startOn} onchange={() => (q.once = true)} />{t('Once at')}</label>
      <input class="text date" type="date" bind:value={q.date} disabled={!q.startOn || !q.once} aria-label={t('Once at')} />
    </div>
  {/if}
  <div class="days indent">
    {#if q.sync}
      <span class="lead3" class:disabled={!q.startOn}>{t('Daily')}</span>
    {:else}
      <label class="check lead3"><input type="radio" checked={!q.once} disabled={!q.startOn} onchange={() => (q.once = false)} />{t('Daily')}</label>
    {/if}
    {#each columns as col, c (c)}
      <div class="col">
        {#each col as d (d.day)}
          <label class="check"
            ><input type="checkbox" checked={q.days.includes(d.day)} disabled={!daysOn} onchange={(e) => toggleDay(d.day, e.currentTarget.checked)} />{t(
              d.label,
            )}</label
          >
        {/each}
      </div>
    {/each}
  </div>
  <div class="rule"></div>
  <div class="row">
    <label class="check lead"><input type="checkbox" bind:checked={q.stopOn} disabled={!q.startOn} />{t('Stop download at')}</label>
    <input class="text time" type="time" bind:value={q.stop} disabled={!q.startOn || !q.stopOn} aria-label={t('Stop download at')} />
  </div>
  <div class="row">
    <label class="check grow"><input type="checkbox" bind:checked={q.retriesOn} />{t('Number of retries for each file if downloading failed:')}</label>
    <input class="text num" type="number" min="1" max={maxRetries} bind:value={q.retries} disabled={!q.retriesOn} aria-label={t('Retries')} />
  </div>
  <div class="rule"></div>
  <label class="check"><input type="checkbox" bind:checked={q.openOn} />{t('Open the following file when done:')}</label>
  <div class="row">
    <input class="text grow" bind:value={q.openFile} disabled={!q.openOn} aria-label={t('Open the following file when done:')} spellcheck="false" />
    <button class="btn small" disabled={!q.openOn} aria-label={t('Browse...')} onclick={() => void browse()}>...</button>
  </div>
  <label class="check"
    ><input type="checkbox" checked={q.then === 'exit'} onchange={(e) => (q.then = e.currentTarget.checked ? 'exit' : 'nothing')} />{t(
      'Exit UDM when done',
    )}</label
  >
  <div class="row">
    <label class="check grow"
      ><input type="checkbox" checked={turnOff} onchange={(e) => (q.then = e.currentTarget.checked ? off : 'nothing')} />{t(
        'Turn off computer when done',
      )}</label
    >
    <select
      class="select then"
      bind:value={off}
      disabled={!turnOff}
      onchange={() => (q.then = off)}
      aria-label={t('Turn off computer when done')}
    >
      <option value="shutDown">{t('Shut down')}</option>
      <option value="sleep">{t('Sleep')}</option>
      <option value="hibernate">{t('Hibernate')}</option>
    </select>
  </div>
  <label class="check indent"><input type="checkbox" bind:checked={q.forceClose} disabled={!turnOff} />{t('Force processes to terminate')}</label>
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
</div>

<style>
  .tab {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .kinds {
    gap: 80px;
  }

  .rule {
    border-top: 1px solid var(--border);
    margin: 2px 0;
  }

  .lead {
    width: 150px;
  }

  .lead2 {
    width: 100px;
  }

  .lead3 {
    width: 70px;
    align-self: flex-start;
  }

  .indent {
    padding-inline-start: 6px;
  }

  .time {
    width: 100px;
  }

  .date {
    width: 186px;
  }

  .num {
    width: 46px;
    text-align: end;
  }

  .days {
    display: flex;
    gap: 10px;
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: 1px;
    width: 110px;
  }

  .then {
    width: 118px;
  }

  input[type='radio'] {
    margin-block: 0;
    margin-inline: 0 5px;
    accent-color: var(--focus);
  }

  p {
    margin: 0;
  }
</style>
