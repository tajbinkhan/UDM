<!-- The toolbar's icons: UDM's own line drawings, tinted from the theme
     (AGENTS.md section 7.6: nothing drawn from IDM's images). -->
<script lang="ts" module>
  export type ToolIconName =
    | 'add'
    | 'resume'
    | 'stop'
    | 'stopAll'
    | 'delete'
    | 'deleteCompleted'
    | 'options'
    | 'scheduler'
    | 'startQueue'
    | 'stopQueue'
    | 'grabber';
</script>

<script lang="ts">
  let { name, size = 32, disabled = false }: { name: ToolIconName; size?: number; disabled?: boolean } = $props();

  const tints: Record<ToolIconName, string> = {
    add: 'var(--tint-green)',
    resume: 'var(--tint-blue)',
    stop: 'var(--fg-muted)',
    stopAll: 'var(--tint-red)',
    delete: 'var(--fg-muted)',
    deleteCompleted: 'var(--tint-purple)',
    options: 'var(--tint-purple)',
    scheduler: 'var(--tint-blue)',
    startQueue: 'var(--tint-green)',
    stopQueue: 'var(--tint-red)',
    grabber: 'var(--tint-blue)',
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 32 32"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  style:color={disabled ? 'var(--fg-disabled)' : tints[name]}
  aria-hidden="true"
>
  {#if name === 'add'}
    <path d="M13 4h6v9h9v6h-9v9h-6v-9H4v-6h9z" />
  {:else if name === 'resume'}
    <circle cx="16" cy="16" r="12.5" />
    <path d="M16 9v12M10.5 16l5.5 5.5 5.5-5.5" />
  {:else if name === 'stop'}
    <circle cx="16" cy="16" r="12.5" />
    <rect x="11" y="11" width="10" height="10" rx="1" />
  {:else if name === 'stopAll'}
    <path d="M10 3.5h12l6.5 6.5v12L22 28.5H10L3.5 22V10z" />
    <path d="M11 13v6M16 11v10M21 13v6" />
  {:else if name === 'delete'}
    <path d="M5 8h22M12.5 8V5h7v3M7.5 8l1.5 19.5h14L24.5 8" />
    <path d="M13 13v10M19 13v10" />
  {:else if name === 'deleteCompleted'}
    <path d="M5 8h22M12.5 8V5h7v3M7.5 8l1.5 19.5h14L24.5 8" />
    <path d="M11.5 17.5l3.5 3.5 6-7" />
  {:else if name === 'options'}
    <circle cx="16" cy="16" r="4" />
    <path
      d="M14 3.5h4l.8 3.4 2.6 1.1 3-1.8 2.8 2.8-1.8 3 1.1 2.6 3.4.8v4l-3.4.8-1.1 2.6 1.8 3-2.8 2.8-3-1.8-2.6 1.1-.8 3.4h-4l-.8-3.4-2.6-1.1-3 1.8-2.8-2.8 1.8-3-1.1-2.6-3.4-.8v-4l3.4-.8 1.1-2.6-1.8-3 2.8-2.8 3 1.8 2.6-1.1z"
    />
  {:else if name === 'scheduler'}
    <circle cx="16" cy="17.5" r="10.5" />
    <path d="M16 11v6.5l4 2.5M5 7l4-3.5M27 7l-4-3.5" />
  {:else if name === 'startQueue'}
    <path d="M4 7h15M4 13h15M4 19h9" />
    <path d="M18 17l9 5.5-9 5.5z" />
  {:else if name === 'stopQueue'}
    <path d="M4 7h15M4 13h15M4 19h9" />
    <rect x="17.5" y="17.5" width="10" height="10" rx="1" />
  {:else if name === 'grabber'}
    <circle cx="14" cy="14" r="10.5" />
    <path d="M3.5 14h21M14 3.5c-3.5 3-3.5 18 0 21M14 3.5c3.5 3 3.5 18 0 21" />
    <path d="M25 20v9M21.5 25.5 25 29l3.5-3.5" />
  {/if}
</svg>
