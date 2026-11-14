<!-- Options > Proxy / Socks: how downloads reach servers. -->
<script lang="ts">
  import { messageOf, systemProxy } from '../../ipc';
  import type { ProxyDraft } from '../../ipc/types';
  import { t } from '../../i18n';
  import Sheet from '../../components/Sheet.svelte';
  import { editing } from './draft.svelte';
  import PageHead from './PageHead.svelte';

  // The pages are drawn only while the draft exists.
  const d = $derived(editing.draft!);
  const manual = $derived(d.proxyMode === 'manual');
  type Advanced = {
    proxies: ProxyDraft[];
    socks4: boolean;
    remoteDns: boolean;
    socksHttps: boolean;
    socksFtp: boolean;
    bypass: string;
  };
  let advanced = $state<Advanced | null>(null);
  const names = ['Use HTTP Proxy', 'Use HTTPS Proxy', 'Use FTP Proxy', 'Use socks'];
  let note = $state('');

  /** Fill the manual proxies from the system's, keeping the logins typed. */
  async function getSystem() {
    note = '';
    try {
      const found = await systemProxy();
      d.proxies = d.proxies.map((p, i) => ({ ...p, on: found.proxies[i].on, host: found.proxies[i].host, port: found.proxies[i].port }));
      d.extras.proxyBypass = found.bypass;
      d.proxyMode = 'manual';
    } catch (e) {
      note = messageOf(e);
    }
  }
</script>

<PageHead icon="proxy" title={t('Proxy / socks configuration')}>
  <button class="btn" title={t('Fill in the proxies from the Windows proxy setting')} onclick={() => void getSystem()}>{t('Get System')}</button>
</PageHead>
<div class="opt-col">
  <label class="check"
    ><input type="checkbox" bind:checked={d.browserProxy} />{t(
      "Use the proxy of the browser's request for downloads taken over from the browser",
    )}</label
  >
  <label class="check opt-indent"
    ><input type="checkbox" bind:checked={d.extras.browserProxyAfterErrors} disabled={!d.browserProxy} />{t(
      'Only after a download fails: it is tried once more through the browser\'s proxy',
    )}</label
  >
  <div class="opt-rule"></div>
  <label class="check"><input type="radio" name="mode" value="none" bind:group={d.proxyMode} />{t('No proxy/socks')}</label>
  <div class="opt-rule"></div>
  <label class="check"><input type="radio" name="mode" value="system" bind:group={d.proxyMode} />{t('Use system settings')}</label>
  <div class="opt-rule"></div>
  <label class="check"><input type="radio" name="mode" value="script" bind:group={d.proxyMode} />{t('Use automatic configuration script')}</label>
  <div class="row opt-indent">
    <label for="pac" class:disabled={d.proxyMode !== 'script'}>{t('Address')}</label>
    <input id="pac" class="text grow" bind:value={d.pacUrl} disabled={d.proxyMode !== 'script'} spellcheck="false" />
  </div>
  <div class="opt-rule"></div>
  <label class="check"><input type="radio" name="mode" value="manual" bind:group={d.proxyMode} />{t('Manual proxy/socks configuration')}</label>
  <div class="manual opt-indent" class:disabled={!manual}>
    <label for="host">{t('Proxy server address')}</label>
    <label for="port">{t('Port')}</label>
    <label for="user">{t('UserName')}</label>
    <label for="pass">{t('Password')}</label>
    <input id="host" class="text" bind:value={d.proxies[0].host} disabled={!manual} spellcheck="false" />
    <input id="port" class="text" bind:value={d.proxies[0].port} disabled={!manual} inputmode="numeric" />
    <input id="user" class="text" bind:value={d.proxies[0].user} disabled={!manual} autocomplete="off" />
    <input id="pass" class="text" type="password" bind:value={d.proxies[0].password} disabled={!manual} autocomplete="off" />
  </div>
  <span class="opt-indent" class:disabled={!manual}>{t('Use this proxy for the following protocols:')}</span>
  <div class="row opt-indent">
    <label class="check"><input type="checkbox" bind:checked={d.proxies[0].on} disabled={!manual} />http</label>
    <label class="check"><input type="checkbox" bind:checked={d.proxies[1].on} disabled={!manual} />https</label>
    <label class="check"><input type="checkbox" bind:checked={d.proxies[2].on} disabled={!manual} />ftp</label>
    <span class="grow"></span>
    <button
      class="btn"
      disabled={!manual}
      onclick={() =>
        (advanced = {
          proxies: d.proxies.map((p) => ({ ...p })),
          socks4: d.socks4,
          remoteDns: d.extras.remoteDns,
          socksHttps: d.extras.socksHttps,
          socksFtp: d.extras.socksFtp,
          bypass: d.extras.proxyBypass,
        })}>{t('Advanced / Socks...')}</button
    >
  </div>
  <div class="opt-rule"></div>
  <label class="check"><input type="checkbox" bind:checked={d.passiveFtp} />{t('Use FTP in PASV mode')}</label>
  {#if note}<p class="opt-note" role="status">{note}</p>{/if}
</div>

{#if advanced}
  <Sheet
    title={t('Advanced proxy settings')}
    width={400}
    oncancel={() => (advanced = null)}
    onok={() => {
      if (!advanced) return;
      d.proxies = advanced.proxies;
      d.socks4 = advanced.socks4;
      d.extras.remoteDns = advanced.remoteDns;
      d.extras.socksHttps = advanced.socksHttps;
      d.extras.socksFtp = advanced.socksFtp;
      d.extras.proxyBypass = advanced.bypass;
      advanced = null;
    }}
  >
    {#each advanced.proxies as p, i (i)}
      <fieldset class="group">
        <legend><label class="check"><input type="checkbox" bind:checked={p.on} />{t(names[i])}</label></legend>
        <div class="manual" class:disabled={!p.on}>
          <span>{i === 3 ? t('Socks server address') : t('Proxy')}</span>
          <span>{t('Port')}</span>
          <span>{t('UserName')}</span>
          <span>{t('Password')}</span>
          <input class="text" bind:value={p.host} disabled={!p.on} aria-label={t('Proxy')} spellcheck="false" />
          <input class="text" bind:value={p.port} disabled={!p.on} aria-label={t('Port')} inputmode="numeric" />
          <input class="text" bind:value={p.user} disabled={!p.on} aria-label={t('UserName')} autocomplete="off" />
          <input class="text" type="password" bind:value={p.password} disabled={!p.on} aria-label={t('Password')} autocomplete="off" />
        </div>
        {#if i === 3}
          <label class="check socks4"><input type="checkbox" bind:checked={advanced.socks4} disabled={!p.on} />{t('SOCKS 4 (SOCKS 5 when unticked)')}</label>
          <label class="check"
            ><input type="checkbox" bind:checked={advanced.remoteDns} disabled={!p.on || !advanced.socks4} />{t(
              'The proxy looks up host names (SOCKS4a)',
            )}</label
          >
          <span class:disabled={!p.on}>{t('SOCKS also for')}</span>
          <label class="check"
            ><input type="checkbox" bind:checked={advanced.socksHttps} disabled={!p.on} />{t(
              'https addresses, even with an HTTP or HTTPS proxy set',
            )}</label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={advanced.socksFtp} disabled={!p.on} />{t('ftp:// downloads, when no FTP proxy is set')}</label
          >
        {/if}
      </fieldset>
    {/each}
    <label for="bypass">{t('Not for')}</label>
    <input id="bypass" class="text bypass" bind:value={advanced.bypass} spellcheck="false" />
    <p class="opt-note">{t('Hosts reached directly, separated by spaces or semicolons. * matches anything; <local> is any name without a dot.')}</p>
  </Sheet>
{/if}

<style>
  .manual {
    display: grid;
    grid-template-columns: 2fr 0.8fr 1.2fr 1.2fr;
    gap: 2px 6px;
  }

  input[type='radio'] {
    margin-block: 0;
    margin-inline: 0 5px;
    accent-color: var(--focus);
  }

  .socks4 {
    margin-top: 6px;
  }

  .bypass {
    width: 100%;
  }
</style>
