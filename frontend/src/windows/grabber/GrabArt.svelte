<!-- The wizard's picture: a globe and links falling into a folder, UDM's own
     drawing in the theme's colors. -->
<script lang="ts">
  const rows = [20, 45, 70, 95, 120, 145, 170, 195, 220, 245];
  const columns = [10, 35, 60, 85, 110, 135];
</script>

<svg class="art" viewBox="0 0 140 260" preserveAspectRatio="xMidYMid slice" aria-hidden="true">
  <rect width="140" height="260" fill="var(--bar)" />
  <g fill="none" stroke="var(--grid)" stroke-width="0.8">
    {#each rows as y (y)}
      <path d="M0 {y} Q70 {y - 18} 140 {y}" />
    {/each}
    {#each columns as x (x)}
      <path d="M{x} 0 Q{x + 14} 130 {x} 260" />
    {/each}
  </g>
  <circle cx="82" cy="78" r="46" fill="var(--field)" stroke="var(--tint-blue)" stroke-width="2.5" />
  <path d="M36 78h92M82 32c-20 14-20 78 0 92M82 32c20 14 20 78 0 92M44 56h76M44 100h76" fill="none" stroke="var(--tint-blue)" stroke-width="1.5" />
  <g stroke="var(--tint-green)" stroke-width="3" stroke-linecap="round" fill="none">
    <path d="M42 140v26M34 158l8 8 8-8" />
    <path d="M78 150v30M70 172l8 8 8-8" />
    <path d="M110 136v22M103 152l7 7 7-7" />
  </g>
  <path d="M22 196h34l8 8h56v44H22z" fill="var(--tint-folder)" stroke="var(--fg-muted)" stroke-linejoin="round" />
  <path d="M22 248l10-36h98l-10 36z" fill="var(--tint-folder)" stroke="var(--fg-muted)" stroke-linejoin="round" />
</svg>

<style>
  .art {
    width: 140px;
    height: 100%;
    border: 1px solid var(--frame);
    display: block;
  }
</style>
