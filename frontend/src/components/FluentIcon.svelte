<!-- One icon from Fluent UI System Icons (fluent.ts), drawn at any size in
     the color it is given. -->
<script lang="ts">
  import type { FluentIcon } from './fluent';

  let { icon, size, color }: { icon: FluentIcon; size: number; color: string } = $props();
</script>

<svg class="icon" width={size} height={size} viewBox="0 0 {icon.size} {icon.size}" aria-hidden="true">
  <path d={icon.d} fill={color} fill-rule={icon.evenOdd ? 'evenodd' : undefined} />
</svg>

<style>
  /* Never squeezed by a long name beside it. */
  .icon {
    flex: none;
  }
</style>
