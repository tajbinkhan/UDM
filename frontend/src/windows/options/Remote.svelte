<!-- Options > Remote control: the page a phone opens to control UDM. -->
<script lang="ts">
  import { pairPhone, remoteStatus, removeDevice } from '../../ipc';
  import type { OptionsView } from '../../ipc/types';
  import { t } from '../../i18n';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  let { view }: { view: OptionsView } = $props();
  // The pages are drawn only while the draft exists.
  const e = $derived(editing.draft!.extras);
  const r = $derived(view.remote);

  // How it stands changes as phones pair and connect: ask while the page
  // is open.
  $effect(() => {
    const timer = setInterval(() => void remoteStatus().catch(() => undefined), 2000);
    return () => clearInterval(timer);
  });
</script>

<PageHead icon="remote" title={t('Remote control')} />
<div class="opt-col">
  <label class="check"><input type="checkbox" bind:checked={e.remoteEnabled} />{t('Let phones on this network control UDM')}</label>
  <div class="row opt-indent" class:disabled={!e.remoteEnabled}>
    <label for="port">{t('Port')}</label>
    <input id="port" class="text num" type="number" min="1" max="65535" bind:value={e.remotePort} disabled={!e.remoteEnabled} />
  </div>
  <fieldset class="group opt-col" class:disabled={!e.remoteEnabled}>
    <legend>{t('Listen on')}</legend>
    <label class="check"><input type="radio" name="remote-all" checked={e.remoteAll} onchange={() => (e.remoteAll = true)} disabled={!e.remoteEnabled} />{t('Every network interface')}</label>
    <label class="check"
      ><input type="radio" name="remote-all" checked={!e.remoteAll} onchange={() => (e.remoteAll = false)} disabled={!e.remoteEnabled} />{t('Only these addresses:')}</label
    >
    <input
      class="text"
      bind:value={e.remoteAddresses}
      disabled={!e.remoteEnabled || e.remoteAll}
      aria-label={t('Only these addresses:')}
      placeholder="192.168.1.20, 192.168.1.21"
      spellcheck="false"
    />
  </fieldset>
  {#if r}
    <p class:error-text={r.error} role="status">{r.status}</p>
    {#if r.fingerprint}<p class="opt-note fingerprint">{t('Certificate fingerprint (SHA-256):')} {r.fingerprint}</p>{/if}
    <fieldset class="group opt-col">
      <legend>{t('Paired devices')}</legend>
      {#each r.devices as device (device.id)}
        <div class="row">
          <span class="grow"><strong>{device.name}</strong> <span class="opt-note">{device.detail}</span></span>
          <button class="btn" title={t('It stops working at once')} onclick={() => void removeDevice(device.id)}>{t('Remove')}</button>
        </div>
      {:else}
        <p class="opt-note">{t('None yet.')}</p>
      {/each}
    </fieldset>
    {#if r.pairing}
      <div class="pairing">
        {#if r.pairing.expired}
          <p>{t('The code was used or ran out.')}</p>
          <button class="btn" onclick={() => void pairPhone(true)}>{t('Show a new code')}</button>
        {:else}
          <svg
            class="qr"
            viewBox="-2 -2 {r.pairing.side + 4} {r.pairing.side + 4}"
            role="img"
            aria-label={t('QR code for pairing a phone')}
            shape-rendering="crispEdges"
          >
            <rect x="-2" y="-2" width={r.pairing.side + 4} height={r.pairing.side + 4} fill="var(--qr-light)" />
            {#each r.pairing.modules as dark, i (i)}
              {#if dark}<rect x={i % r.pairing.side} y={Math.floor(i / r.pairing.side)} width="1" height="1" fill="var(--qr-dark)" />{/if}
            {/each}
          </svg>
          <p>{t("Scan the QR code with the phone's camera and open the link.")}</p>
          <p class="opt-note">{r.pairing.typeNote}</p>
          <p class="code">{r.pairing.code}</p>
        {/if}
        <button class="btn" onclick={() => void pairPhone(false)}>{t('Done')}</button>
      </div>
    {:else}
      <button
        class="btn opt-right"
        disabled={!r.canPair}
        title={r.canPair ? '' : t('Turn remote control on and save first')}
        onclick={() => void pairPhone(true)}>{t('Pair a phone…')}</button
      >
    {/if}
  {/if}
</div>

<style>
  .num {
    width: 90px;
  }

  p {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .fingerprint {
    font-family: monospace;
    font-size: 11px;
  }

  .pairing {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    text-align: center;
  }

  /* A QR code reads only dark on light, whatever the theme. */
  .qr {
    width: 180px;
    height: 180px;
  }

  .code {
    font-family: monospace;
    font-size: 16px;
    letter-spacing: 2px;
  }
</style>
