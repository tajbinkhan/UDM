<!-- A 16-pixel icon for a kind of file, for the list and the tree, from
     Fluent UI System Icons (fluent.ts), tinted by kind from the theme. -->
<script lang="ts">
  import type { FileIcon } from '../ipc/types';
  import { fluent, type FluentIcon as Icon } from './fluent';
  import FluentIcon from './FluentIcon.svelte';

  let { kind, size = 16 }: { kind: FileIcon; size?: number } = $props();

  const icons: Record<FileIcon, { icon: Icon; tint: string }> = {
    archive: { icon: fluent.folderZip16Regular, tint: 'var(--tint-folder)' },
    document: { icon: fluent.documentText16Regular, tint: 'var(--tint-blue)' },
    music: { icon: fluent.musicNote216Regular, tint: 'var(--tint-purple)' },
    program: { icon: fluent.windowApps16Regular, tint: 'var(--tint-blue)' },
    video: { icon: fluent.videoClip16Regular, tint: 'var(--tint-red)' },
    image: { icon: fluent.image16Regular, tint: 'var(--tint-green)' },
    other: { icon: fluent.document16Regular, tint: 'var(--fg-muted)' },
  };

  const shown = $derived(icons[kind]);
</script>

<FluentIcon icon={shown.icon} {size} color={shown.tint} />
