<!-- A 16-pixel icon for a kind of file, for the list and the tree. -->
<script lang="ts">
  import type { FileIcon } from '../ipc/types';

  let { kind, size = 16 }: { kind: FileIcon; size?: number } = $props();

  const tints: Record<FileIcon, string> = {
    archive: 'var(--tint-folder)',
    document: 'var(--tint-blue)',
    music: 'var(--tint-purple)',
    program: 'var(--tint-blue)',
    video: 'var(--tint-red)',
    image: 'var(--tint-green)',
    other: 'var(--fg-muted)',
  };
</script>

<svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke-linejoin="round" aria-hidden="true">
  {#if kind === 'archive'}
    <rect x="2.5" y="1.5" width="11" height="13" rx="1" fill="var(--tint-folder)" stroke="var(--fg-muted)" />
    <path d="M8 1.5v2M8 4.5v1M8 6.5v1" stroke="var(--fg)" />
    <rect x="6.5" y="8.5" width="3" height="3" fill="var(--field)" stroke="var(--fg)" />
  {:else if kind === 'program'}
    <rect x="1.5" y="2.5" width="13" height="11" rx="1" fill="var(--field)" stroke={tints.program} />
    <path d="M1.5 5.5h13" stroke={tints.program} stroke-width="2" />
    <path d="M5 9l2 1.5L5 12" stroke="var(--fg)" />
  {:else if kind === 'music'}
    <path d="M3.5 1.5h6l3 3v10h-9z" fill="var(--field)" stroke="var(--fg-muted)" />
    <path d="M7 12V6.5l3.5-1V11" stroke={tints.music} stroke-width="1.4" />
    <circle cx="6" cy="12" r="1.3" fill={tints.music} />
    <circle cx="9.5" cy="11" r="1.3" fill={tints.music} />
  {:else if kind === 'video'}
    <rect x="1.5" y="3.5" width="13" height="9" rx="1" fill="var(--fg-muted)" stroke="var(--fg-muted)" />
    <path d="M6.5 6v4l3.5-2z" fill="var(--field)" />
    <path d="M3 3.5v9M13 3.5v9" stroke="var(--field)" stroke-dasharray="1 1.2" />
  {:else if kind === 'image'}
    <rect x="1.5" y="2.5" width="13" height="11" rx="1" fill="var(--field)" stroke={tints.image} />
    <path d="M2 12l4-4 3 3 2-2 3.5 3.5" stroke={tints.image} />
    <circle cx="11" cy="5.5" r="1.2" fill={tints.image} />
  {:else if kind === 'document'}
    <path d="M3.5 1.5h6l3 3v10h-9z" fill="var(--field)" stroke="var(--fg-muted)" />
    <path d="M5.5 7h5M5.5 9h5M5.5 11h3.5" stroke={tints.document} />
  {:else}
    <path d="M3.5 1.5h6l3 3v10h-9z" fill="var(--field)" stroke="var(--fg-muted)" />
    <path d="M9.5 1.5v3h3" stroke="var(--fg-muted)" />
  {/if}
</svg>
