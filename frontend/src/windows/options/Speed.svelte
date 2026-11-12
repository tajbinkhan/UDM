<!-- Options > Speed: how many downloads run at once, and the speed limit. -->
<script lang="ts">
  import { t } from '../../i18n';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  // The pages are drawn only while the draft exists.
  const e = $derived(editing.draft!.extras);
</script>

<PageHead icon="speed" title={t('Speed limit')} />
<div class="opt-col">
  <div class="row">
    <label for="at-once">{t('Downloads at once')}</label>
    <input id="at-once" class="text num" type="number" min="1" max="10" bind:value={e.maxConcurrent} />
  </div>
  <fieldset class="group opt-col">
    <legend><label class="check"><input type="checkbox" bind:checked={e.limitOn} />{t('Speed limit')}</label></legend>
    <div class="row" class:disabled={!e.limitOn}>
      <label for="limit">{t('Limit to')}</label>
      <input id="limit" class="text num" type="number" min="1" bind:value={e.limitKb} disabled={!e.limitOn} />
      <span>{t('KB/s')}</span>
    </div>
    <div class="row">
      <label class="check"><input type="checkbox" bind:checked={e.perFileOn} />{t('Limit each file to')}</label>
      <input class="text num" type="number" min="1" bind:value={e.perFileKb} disabled={!e.perFileOn} aria-label={t('Limit each file to')} />
      <span>{t('KB/s')}</span>
    </div>
    <p class="opt-note opt-indent">{t('Downloads with a speed limit of their own keep it instead')}</p>
    <label class="check"><input type="checkbox" bind:checked={e.limitWhileActive} disabled={!e.limitOn} />{t("Only while I'm using the computer")}</label>
    <div class="row opt-indent" class:disabled={!e.limitOn || !e.limitWhileActive}>
      <label for="idle">{t('Lift it after')}</label>
      <input id="idle" class="text num" type="number" min="1" max="240" bind:value={e.idleMinutes} disabled={!e.limitOn || !e.limitWhileActive} />
      <span>{t('minutes without keyboard or mouse input')}</span>
    </div>
    <label class="check"><input type="checkbox" bind:checked={e.limitOnStartup} />{t('Always turn the speed limit on when UDM starts')}</label>
  </fieldset>
</div>

<style>
  .num {
    width: 80px;
  }

  .row > label {
    white-space: nowrap;
  }
</style>
