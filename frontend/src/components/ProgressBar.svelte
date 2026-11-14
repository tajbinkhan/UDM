<!-- A download's progress bar: the filled share on a light track. -->
<script lang="ts">
  import { t } from '../i18n';

  let { fraction, height = 16 }: { fraction: number; height?: number } = $props();
  const percent = $derived(Math.round(Math.max(0, Math.min(1, fraction)) * 100));
</script>

<div class="bar" style:height="{height}px" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent} aria-label={t('Downloaded')}>
  <div class="fill" style:width="{percent}%"></div>
</div>

<style>
  .bar {
    background: var(--progress-track);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--progress);
    border-radius: var(--radius);
    transition: width 0.2s linear;
  }

</style>
