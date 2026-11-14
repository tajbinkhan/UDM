<!-- The toolbar's icons, from Fluent UI System Icons (fluent.ts), tinted
     from the theme (AGENTS.md section 7.6: nothing drawn from IDM's images). -->
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
  import { fluent, type FluentIcon as Icon } from './fluent';
  import FluentIcon from './FluentIcon.svelte';

  let { name, size = 32, disabled = false }: { name: ToolIconName; size?: number; disabled?: boolean } = $props();

  const icons: Record<ToolIconName, { icon: Icon; tint: string }> = {
    add: { icon: fluent.addCircle24Regular, tint: 'var(--tint-green)' },
    resume: { icon: fluent.arrowCircleDown24Regular, tint: 'var(--tint-blue)' },
    stop: { icon: fluent.recordStop24Regular, tint: 'var(--fg-muted)' },
    stopAll: { icon: fluent.recordStop24Filled, tint: 'var(--tint-red)' },
    delete: { icon: fluent.delete24Regular, tint: 'var(--fg-muted)' },
    deleteCompleted: { icon: fluent.broom24Regular, tint: 'var(--tint-purple)' },
    options: { icon: fluent.settings24Regular, tint: 'var(--tint-purple)' },
    scheduler: { icon: fluent.clockAlarm24Regular, tint: 'var(--tint-blue)' },
    startQueue: { icon: fluent.navigationPlay20Regular, tint: 'var(--tint-green)' },
    stopQueue: { icon: fluent.textBulletListDismiss20Regular, tint: 'var(--tint-red)' },
    grabber: { icon: fluent.globeSearch24Regular, tint: 'var(--tint-blue)' },
  };

  const shown = $derived(icons[name]);
</script>

<FluentIcon icon={shown.icon} {size} color={disabled ? 'var(--fg-disabled)' : shown.tint} />
