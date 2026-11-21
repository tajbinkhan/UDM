<!-- The tools UDM did not find (yt-dlp, ffmpeg, gallery-dl): offered at the
     first start and from the strip that names them. Options > Tools has
     them too. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { answerToolsOffer } from '../../ipc';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import ToolProgress from '../../components/ToolProgress.svelte';
  import type { MissingTools, ToolKey } from '../../ipc/types';

  let { missing, onclose }: { missing: MissingTools; onclose: () => void } = $props();
  let chosen = $state<ToolKey[]>(untrack(() => missing.tools.filter((m) => m.installable).map((m) => m.tool)));

  function toggle(tool: ToolKey, on: boolean) {
    chosen = on ? [...chosen, tool] : chosen.filter((c) => c !== tool);
  }

  // Cancel is Not now: nothing is installed, and the first start stops asking.
  function answer(tools: ToolKey[]) {
    void answerToolsOffer(tools);
    onclose();
  }
</script>

<Sheet title={t('Tools')} width={440} oncancel={() => answer([])} onok={() => answer(chosen)} okDisabled={missing.busy}>
  <p class="lead">
    {t('Video and picture sites need these tools. UDM downloads their official builds and checks them against their published checksums. Options > Tools has them too.')}
  </p>
  <ul>
    {#each missing.tools as m (m.tool)}
      <li>
        {#if m.installable}
          <label>
            <input
              type="checkbox"
              checked={chosen.includes(m.tool)}
              disabled={missing.busy}
              onchange={(e) => toggle(m.tool, e.currentTarget.checked)}
            />
            <strong>{m.name}</strong>
          </label>
        {:else}
          <strong>{m.name}</strong>
        {/if}
        <div class="purpose">{m.purpose}</div>
        {#if m.hint}<div class="hint">{m.hint}</div>{/if}
        {#if m.progress}<div class="progress"><ToolProgress progress={m.progress} /></div>{/if}
      </li>
    {/each}
  </ul>
  {#if missing.busy}<p class="lead">{missing.busyText}</p>{/if}
</Sheet>

<style>
  .lead {
    margin-block: 0 8px;
    line-height: 1.4;
  }

  ul {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
  }

  li {
    padding-block: 6px;
    border-block-end: 1px solid var(--border);
  }

  .purpose,
  .hint,
  .progress {
    margin-inline-start: 22px;
    color: var(--fg-muted);
    line-height: 1.4;
  }

  .hint {
    user-select: text;
  }
</style>
