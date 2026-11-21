<!-- The categories tree's icons, from Fluent UI System Icons (fluent.ts):
     a folder for all downloads, a sign for each other branch, and the file
     kinds for the categories. -->
<script lang="ts">
  import type { TreeIcon } from '../ipc/types';
  import FileGlyph from './FileGlyph.svelte';
  import { fluent, type FluentIcon as Icon } from './fluent';
  import FluentIcon from './FluentIcon.svelte';

  let { icon }: { icon: TreeIcon } = $props();

  const branches: Record<Exclude<TreeIcon, object>, { icon: Icon; tint: string }> = {
    allDownloads: { icon: fluent.folder16Filled, tint: 'var(--tint-folder)' },
    unfinished: { icon: fluent.arrowDownload16Regular, tint: 'var(--tint-blue)' },
    finished: { icon: fluent.checkmarkCircle16Regular, tint: 'var(--tint-green)' },
    grabber: { icon: fluent.globe16Regular, tint: 'var(--tint-blue)' },
    queues: { icon: fluent.textBulletListSquare16Regular, tint: 'var(--fg-muted)' },
    queue: { icon: fluent.textBulletListSquare16Regular, tint: 'var(--fg-muted)' },
  };
</script>

{#if typeof icon === 'object'}
  <FileGlyph kind={icon.category} />
{:else}
  <FluentIcon icon={branches[icon].icon} size={16} color={branches[icon].tint} />
{/if}
