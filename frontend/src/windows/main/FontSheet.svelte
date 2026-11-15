<!-- Font: the download list's font, the window's own or a fixed-width one,
     and its size. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { setListFont } from '../../ipc';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';

  let { mono, size, onclose }: { mono: boolean; size: number; onclose: () => void } = $props();
  // The sheet edits a copy of what it opened with.
  let choice = $state(untrack(() => ({ mono, size })));
  const sizes = [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24];
</script>

<Sheet
  title={t('Font')}
  width={420}
  oncancel={onclose}
  onok={() => {
    void setListFont($state.snapshot(choice));
    onclose();
  }}
>
  <div class="cols">
    <div class="col">
      <label for="family">{t('Font:')}</label>
      <select id="family" class="select" size="4" value={choice.mono ? 'mono' : 'ui'} onchange={(e) => (choice.mono = e.currentTarget.value === 'mono')}>
        <option value="ui">{t("The window's font")}</option>
        <option value="mono">{t('Fixed-width font')}</option>
      </select>
    </div>
    <div class="col narrow">
      <label for="size">{t('Size:')}</label>
      <select id="size" class="select" size="6" bind:value={choice.size}>
        {#each sizes as s (s)}<option value={s}>{s}</option>{/each}
      </select>
    </div>
  </div>
  <fieldset class="group">
    <legend>{t('Sample')}</legend>
    <div class="sample" class:mono={choice.mono} style:font-size="{choice.size}px">AaBbYyZz</div>
  </fieldset>
  <button type="button" class="btn reset" onclick={() => (choice = { mono: false, size: 12 })}>{t('Reset to default')}</button>
</Sheet>

<style>
  .cols {
    display: flex;
    gap: 14px;
  }

  .col {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .narrow {
    flex: 0 0 80px;
  }

  .select[size] {
    height: auto;
    background-image: none;
    padding-inline-end: 4px;
  }

  .sample {
    height: 56px;
    display: grid;
    place-items: center;
  }

  .sample.mono {
    font-family: ui-monospace, 'Cascadia Mono', Consolas, 'DejaVu Sans Mono', monospace;
  }

  .reset {
    align-self: flex-start;
  }
</style>
