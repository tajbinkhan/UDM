<!-- A page's heading in Options: its icon, and its title over a rule. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    icon,
    title,
    children,
  }: {
    icon: 'general' | 'types' | 'folder' | 'download' | 'network' | 'proxy' | 'key' | 'sound' | 'speed' | 'browser' | 'archive' | 'tools' | 'remote';
    title: string;
    children?: Snippet;
  } = $props();
</script>

<div class="head">
  <svg width="34" height="30" viewBox="0 0 34 30" fill="none" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
    {#if icon === 'general'}
      <rect x="2.5" y="4.5" width="22" height="17" rx="1.5" fill="var(--field)" stroke="var(--tint-blue)" />
      <path d="M2.5 8.5h22" stroke="var(--tint-blue)" stroke-width="2" />
      <circle cx="24" cy="20" r="6" fill="var(--tint-green)" />
      <path d="M22 17.5l4 2.5-4 2.5z" fill="var(--field)" />
    {:else if icon === 'types'}
      <path d="M4.5 3.5h11l4 4v17h-15z" fill="var(--field)" stroke="var(--fg-muted)" />
      <path d="M12.5 7.5h11l4 4v16h-15z" fill="var(--field)" stroke="var(--fg-muted)" />
      <path d="M15.5 15h9M15.5 18h9M15.5 21h6" stroke="var(--tint-blue)" />
    {:else if icon === 'folder'}
      <path d="M2.5 7.5h10l2.5 2.5h14v15h-26.5z" fill="var(--tint-folder)" stroke="var(--fg-muted)" />
      <path d="M2.5 25l4-11h26l-4 11z" fill="var(--tint-folder)" stroke="var(--fg-muted)" />
    {:else if icon === 'download'}
      <circle cx="16" cy="15" r="12" fill="var(--field)" stroke="var(--tint-blue)" stroke-width="1.5" />
      <path d="M16 8v12M11 15.5l5 5 5-5" stroke="var(--tint-green)" stroke-width="2.5" />
    {:else if icon === 'network'}
      <circle cx="18" cy="16" r="11" fill="var(--field)" stroke="var(--tint-blue)" />
      <path d="M7 16h22M18 5c-4 3-4 19 0 22M18 5c4 3 4 19 0 22" stroke="var(--tint-blue)" />
      <rect x="2.5" y="2.5" width="11" height="8" rx="1" fill="var(--field)" stroke="var(--fg-muted)" />
    {:else if icon === 'proxy'}
      <rect x="6.5" y="3.5" width="18" height="23" rx="1.5" fill="var(--field)" stroke="var(--fg-muted)" />
      <path d="M10 9h11M10 13h11M10 17h11" stroke="var(--tint-blue)" />
      <circle cx="21" cy="22" r="1.5" fill="var(--tint-green)" />
    {:else if icon === 'key'}
      <circle cx="11" cy="11" r="6.5" fill="var(--tint-folder)" stroke="var(--fg-muted)" />
      <circle cx="11" cy="11" r="2" fill="var(--field)" />
      <path d="M15.5 15.5l11 11M22 22l3-3M25 25l2.5-2.5" stroke="var(--fg-muted)" stroke-width="2" />
    {:else if icon === 'speed'}
      <path d="M4.5 22a12.5 12.5 0 1 1 25 0z" fill="var(--field)" stroke="var(--tint-blue)" stroke-width="1.5" />
      <path d="M17 22l6-8" stroke="var(--tint-red)" stroke-width="2" />
      <circle cx="17" cy="22" r="2" fill="var(--fg-muted)" />
    {:else if icon === 'browser'}
      <rect x="3.5" y="4.5" width="26" height="20" rx="1.5" fill="var(--field)" stroke="var(--tint-blue)" />
      <path d="M3.5 9.5h26" stroke="var(--tint-blue)" stroke-width="2" />
      <path d="M16.5 12v8M13 17l3.5 3.5L20 17" stroke="var(--tint-green)" stroke-width="2" />
    {:else if icon === 'archive'}
      <path d="M5.5 9.5h22v16h-22z" fill="var(--tint-folder)" stroke="var(--fg-muted)" />
      <path d="M4 5.5h25v4H4z" fill="var(--tint-folder)" stroke="var(--fg-muted)" />
      <path d="M16.5 9.5v10M14 13h5M14 16h5" stroke="var(--fg-muted)" />
    {:else if icon === 'tools'}
      <path d="M21 4a6 6 0 0 0-5.5 8.3L5 22.8 8.2 26l10.5-10.5A6 6 0 0 0 27 10l-3.5 3.5-3-3L24 7a6 6 0 0 0-3-3z" fill="var(--field)" stroke="var(--fg-muted)" />
      <circle cx="7.5" cy="23.5" r="1" fill="var(--tint-blue)" />
    {:else if icon === 'remote'}
      <rect x="10.5" y="2.5" width="13" height="25" rx="2" fill="var(--field)" stroke="var(--fg-muted)" />
      <path d="M10.5 6.5h13M10.5 22.5h13" stroke="var(--fg-muted)" />
      <path d="M14 14h6M17 11v6" stroke="var(--tint-green)" stroke-width="2" />
    {:else}
      <path d="M4 11h6l7-6v20l-7-6H4z" fill="var(--tint-purple)" stroke="var(--fg-muted)" />
      <path d="M21 10c2 2 2 8 0 10M25 7c4 4 4 12 0 16" stroke="var(--tint-purple)" stroke-width="2" />
    {/if}
  </svg>
  <h2>{title}</h2>
  {#if children}<div class="extra">{@render children()}</div>{/if}
</div>

<style>
  .head {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    margin-bottom: 6px;
  }

  .extra {
    flex: none;
    padding-bottom: 2px;
  }

  h2 {
    flex: 1;
    margin: 0 0 2px;
    padding-bottom: 3px;
    font-size: 13px;
    font-weight: 700;
    text-align: end;
    border-bottom: 1px solid var(--border);
  }
</style>
