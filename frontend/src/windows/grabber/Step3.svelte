<!-- Step 3: which pages to explore. -->
<script lang="ts">
  import { t } from '../../i18n';
  import { grab } from './grab.svelte';

  let { advanced }: { advanced: boolean } = $props();
  const d = $derived(grab.draft!);
</script>

<div class="col">
  <p>{t('At this step you should specify what web pages to explore to find the required files. At the next step, you will be able to set file types, location, and other filters.')}</p>
  <label class="check"><input type="radio" checked={d.wholeSite} onchange={() => (d.wholeSite = true)} />{t('Explore the whole site')}</label>
  <label class="check"><input type="radio" checked={!d.wholeSite} onchange={() => (d.wholeSite = false)} />{t('Explore the specified number of link levels:')}</label>
  <div class="row indent" class:disabled={d.wholeSite}>
    <input class="text num" type="number" min="0" max="10" bind:value={d.depth} disabled={d.wholeSite} aria-label={t('levels within the base site')} />
    <span class="gap">{t('levels within the base site')}</span>
    <input class="text num" type="number" min="0" max="10" bind:value={d.otherDepth} disabled={d.wholeSite} aria-label={t('levels on other sites')} />
    <span>{t('levels on other sites')}</span>
  </div>
  <label class="check"><input type="checkbox" bind:checked={d.ignorePopups} />{t('Ignore popup windows')}</label>
  <label class="check"><input type="checkbox" bind:checked={d.noParent} />{t("Don't explore parent directories")}</label>
  <label class="check"><input type="checkbox" bind:checked={d.subdomains} />{t('Explore all sites within the main domain')}</label>
  <label class="check"><input type="checkbox" bind:checked={d.renderScripts} />{t('Process JavaScript')}</label>
  <span class="muted indent">{t('Use this option carefully: pages open in the browser to run their scripts, which takes longer.')}</span>
  {#if advanced}
    <div class="rule"></div>
    <label class="check"><input type="checkbox" bind:checked={d.exploreOnlyOn} />{t('Explore web pages within the following paths/domains only:')}</label>
    <textarea class="opt-area" rows="2" bind:value={d.exploreOnly} disabled={!d.exploreOnlyOn} aria-label={t('Explore only')} spellcheck="false"></textarea>
    <label class="check"><input type="checkbox" bind:checked={d.exploreSkipOn} />{t("Don't explore web pages within the following paths/domains:")}</label>
    <textarea class="opt-area" rows="2" bind:value={d.exploreSkip} disabled={!d.exploreSkipOn} aria-label={t("Don't explore")} spellcheck="false"></textarea>
    <span>{t('Use semicolons ( ; ) to separate entries.')}</span>
  {/if}
</div>

<style>
  .col {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .indent {
    margin-inline-start: 22px;
  }

  .num {
    width: 42px;
    text-align: end;
  }

  .gap {
    margin-inline-end: 24px;
  }

  .rule {
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }

  input[type='radio'] {
    margin-block: 0;
    margin-inline: 0 5px;
    accent-color: var(--focus);
  }

  textarea:disabled {
    background: var(--face);
    border-color: var(--border);
  }

  p {
    margin: 0;
  }
</style>
