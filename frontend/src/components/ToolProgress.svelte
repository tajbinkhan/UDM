<!-- How far a tool's download is while UDM installs it: a bar when the size
     is known, and the shell's words ("12.3 MB of 40.0 MB (30%)"). -->
<script lang="ts">
  import type { ToolProgressView } from '../ipc/types';
  import ProgressBar from './ProgressBar.svelte';

  let { progress }: { progress: ToolProgressView } = $props();
</script>

<div class="tool-progress" role="status">
  {#if progress.percent != null}<ProgressBar fraction={progress.percent / 100} height={8} />{/if}
  <span class="text">{progress.text}</span>
</div>

<style>
  .tool-progress {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-block-start: 4px;
  }

  .text {
    color: var(--fg-muted);
    font-variant-numeric: tabular-nums;
  }
</style>
