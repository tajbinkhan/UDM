<!-- What every window shares: the theme, the language, and Escape closing a
     dialog. Draws its content once the first snapshot came. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { projection } from '../stores/projection.svelte';
  import { applyLook } from '../theme';

  let { children, onescape }: { children: Snippet; onescape?: () => void } = $props();

  $effect(() => applyLook(projection.look));
</script>

<svelte:window
  onkeydown={(e) => {
    // A sheet open inside the window takes its own Escape.
    if (e.key === 'Escape' && onescape && !e.defaultPrevented && !document.querySelector('dialog[open]')) {
      e.preventDefault();
      onescape();
    }
  }}
  oncontextmenu={(e) => e.preventDefault()}
/>

{#if projection.look && projection.view}
  {@render children()}
{/if}
