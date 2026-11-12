<!-- The categories tree's icons: folders for the branches, the file kinds
     for the categories. -->
<script lang="ts">
  import type { TreeIcon } from '../ipc/types';
  import FileGlyph from './FileGlyph.svelte';

  let { icon }: { icon: TreeIcon } = $props();
</script>

{#if typeof icon === 'object'}
  <FileGlyph kind={icon.category} />
{:else if icon === 'grabber'}
  <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="var(--tint-blue)" aria-hidden="true">
    <circle cx="8" cy="8" r="6.5" />
    <path d="M1.5 8h13M8 1.5c-2 2-2 11 0 13M8 1.5c2 2 2 11 0 13" />
  </svg>
{:else}
  <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
    <path
      d="M1.5 3.5h5l1.5 1.5h6.5v8.5h-13z"
      fill={icon === 'allDownloads' ? 'var(--tint-folder)' : 'var(--tint-folder)'}
      stroke="var(--fg-muted)"
      stroke-linejoin="round"
    />
    {#if icon === 'unfinished'}
      <path d="M8 6.5v4.5M6 9l2 2 2-2" stroke="var(--tint-blue)" stroke-width="1.4" />
    {:else if icon === 'finished'}
      <path d="M5.5 9l2 2 3.5-4" stroke="var(--tint-green)" stroke-width="1.6" />
    {:else if icon === 'queues' || icon === 'queue'}
      <path d="M4.5 7.5h7M4.5 9.5h7M4.5 11.5h5" stroke="var(--fg)" />
    {/if}
  </svg>
{/if}
