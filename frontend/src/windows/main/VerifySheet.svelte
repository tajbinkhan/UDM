<!-- Verify checksum: compare a finished download's file with the checksum
     its site published. The shell reads the file on a worker. -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { messageOf, verifyChecksum } from '../../ipc';
  import type { Row, VerifyResult } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';

  let { row, start, onclose }: { row: Row; start: string; onclose: () => void } = $props();
  let text = $state(untrack(() => start));
  let busy = $state(false);
  let result = $state<VerifyResult | null>(null);
  let error = $state('');

  async function check() {
    if (!text.trim() || busy) return;
    busy = true;
    error = '';
    result = null;
    try {
      result = await verifyChecksum(row.id, text);
    } catch (e) {
      error = messageOf(e);
    } finally {
      busy = false;
    }
  }
</script>

<Sheet title={t('Verify checksum')} width={460} oncancel={onclose} buttons={false}>
  <p class="name">{row.name}</p>
  <label for="checksum">{t('Paste the MD5, SHA-1, SHA-256, SHA-512 or CRC32 checksum the site gives for this file.')}</label>
  <input
    id="checksum"
    class="text sum"
    bind:value={text}
    oninput={() => (result = null)}
    spellcheck="false"
    autocomplete="off"
  />
  {#if busy}
    <p class="muted" role="status">{t('Reading the file...')}</p>
  {:else if result}
    <p class="muted">{t('Algorithm: {name}', { name: result.algorithm })}</p>
    {#if result.matches}
      <p class="ok" role="status">{t('The file matches the checksum.')}</p>
    {:else}
      <p class="error-text" role="alert">{t('The file does not match the checksum.')}</p>
      <p class="digest">{t('Expected: {digest}', { digest: result.expected })}</p>
      <p class="digest">{t('File: {digest}', { digest: result.actual })}</p>
    {/if}
  {/if}
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
  <div class="row buttons">
    <button class="btn default" disabled={!text.trim() || busy} onclick={() => void check()}>{t('Verify')}</button>
    <button class="btn" onclick={onclose}>{t('Close')}</button>
  </div>
</Sheet>

<style>
  p {
    margin: 4px 0 0;
    overflow-wrap: anywhere;
  }

  .name {
    font-weight: 600;
    margin: 0 0 6px;
  }

  .sum {
    width: 100%;
    margin-top: 4px;
    font-family: monospace;
  }

  .ok {
    color: var(--tint-green);
  }

  .digest {
    font-family: monospace;
    font-size: 11px;
  }

  .buttons {
    justify-content: flex-end;
    margin-top: 10px;
  }
</style>
