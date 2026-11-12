<!-- About UDM: the version, and the daemon's. -->
<script lang="ts">
  import { closeWindow } from '../../ipc';
  import type { AboutView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import logo from '../../../../assets/icon-256.png';

  const view: AboutView | null = $derived(projection.view?.window === 'about' ? projection.view.view : null);
</script>

<Window onescape={() => void closeWindow()}>
  {#if view}
    <div class="dialog">
      <div class="head">
        <img src={logo} width="40" height="40" alt="" />
        <div class="titles">
          <h1>{t('Unified Download Manager')}</h1>
          <p class="version">{t('Version: {version}', { version: view.version })}</p>
        </div>
      </div>
      <button class="btn check" disabled>{t('Check for Update')}</button>
      <div class="box">
        <p>{t('UDM is free software under the MIT license.')}</p>
        {#if view.daemon}<p>{t('Background service: version {version}', { version: view.daemon })}</p>{/if}
      </div>
    </div>
  {/if}
</Window>

<style>
  .dialog {
    padding: 6px 14px 14px;
    gap: 4px;
  }

  .head {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }

  img {
    margin-top: 4px;
  }

  h1 {
    margin: 0;
    font-size: 19px;
    font-weight: 700;
  }

  .version {
    margin: 2px 0 0 52px;
    color: var(--link);
  }

  .check {
    align-self: center;
    width: 147px;
  }

  .box {
    margin-top: 8px;
    height: 74px;
    padding: 6px 8px;
    border: 1px solid var(--border);
  }

  .box p {
    margin: 0 0 4px;
  }
</style>
