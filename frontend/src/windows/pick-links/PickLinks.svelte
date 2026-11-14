<!-- The browser's Download all links: a page's links, filtered by the
     shell, to tick and add to a queue, renamed by a pattern and kept together
     as a package if the user wants. -->
<script lang="ts">
  import { addPicks, closeWindow, linkNames, messageOf, setLinkFilter } from '../../ipc';
  import type { LinkFilter, PickLinksView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import ListBox from '../../components/ListBox.svelte';
  import FileGlyph from '../../components/FileGlyph.svelte';

  const view: PickLinksView | null = $derived(projection.view?.window === 'pickLinks' ? projection.view.view : null);
  /** The rows the user ticked; none start ticked. */
  let ticked = $state<number[]>([]);
  let pickedQueue = $state<number | null>(null);
  let start = $state(true);
  let keep = $state(false);
  let packageName = $state<string | null>(null);
  let picked = $state(-1);
  let error = $state('');
  let busy = $state(false);

  const queue = $derived(pickedQueue ?? view?.queues[0]?.id ?? 1);
  const name = $derived(packageName ?? view?.suggestedPackage ?? '');
  /** The ticked links the filters leave in view; only they are added. */
  const tickedShown = $derived(view ? view.rows.filter((r) => ticked.includes(r.index)).map((r) => r.index) : []);

  /** The Types filter as typed, sent to the shell a moment after typing
   * stops. */
  let types = $state<string | null>(null);
  const typesShown = $derived(types ?? view?.filter.types ?? '');
  let rename = $state('');
  /** The names the rename pattern gives the first ticked links, or why it
   * gives none. */
  let renamed = $state<string[]>([]);
  let renameError = $state('');

  function filter(change: Partial<LinkFilter>) {
    if (!view) return;
    void setLinkFilter({ ...view.filter, ...change }).catch((e: unknown) => (error = messageOf(e)));
  }

  $effect(() => {
    if (types === null) return;
    const typed = types;
    const timer = setTimeout(() => filter({ types: typed }), 300);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    const [indices, pattern] = [tickedShown, rename];
    if (!pattern.trim()) {
      renamed = [];
      renameError = '';
      return;
    }
    const timer = setTimeout(() => {
      linkNames(indices, pattern)
        .then((names) => {
          renamed = names;
          renameError = '';
        })
        .catch((e: unknown) => {
          renamed = [];
          renameError = messageOf(e);
        });
    }, 200);
    return () => clearTimeout(timer);
  });

  function tick(index: number, on: boolean) {
    ticked = on ? [...ticked, index] : ticked.filter((x) => x !== index);
  }

  /** Tick every link of one file type, as the list shows its type. */
  function tickType(type: string) {
    if (!view || !type) return;
    const more = view.rows.filter((r) => r.kind.toLowerCase() === type).map((r) => r.index);
    ticked = [...new Set([...ticked, ...more])];
  }

  async function ok() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      await addPicks({ indices: tickedShown, queue, start, package: keep ? name : null, rename, quality: 0, folder: '' });
    } catch (e) {
      error = messageOf(e);
    } finally {
      busy = false;
    }
  }
</script>

<Window onescape={() => void closeWindow()}>
  {#if view}
    <div class="dialog">
      <p class="page">{t('Download links from the page')} <span class="muted">{view.page}</span></p>
      <div class="row">
        <label class="check"
          ><input type="checkbox" checked={view.filter.hidePages} onchange={(e) => filter({ hidePages: e.currentTarget.checked })} />{t(
            'Hide web pages',
          )}</label
        >
        <label class="check"
          ><input type="checkbox" checked={view.filter.hideImages} onchange={(e) => filter({ hideImages: e.currentTarget.checked })} />{t(
            'Hide images on the page',
          )}</label
        >
        <label class="check"
          ><input
            type="checkbox"
            checked={view.filter.hideRepeated}
            onchange={(e) => filter({ hideRepeated: e.currentTarget.checked })}
          />{t('Hide repeated links')}</label
        >
        <label for="only-types">{t('Types')}</label>
        <input
          id="only-types"
          class="text grow"
          value={typesShown}
          oninput={(e) => (types = e.currentTarget.value)}
          placeholder={t('all, or zip, pdf')}
          spellcheck="false"
        />
      </div>
      <div class="list">
        <ListBox
          columns={[{ title: t('Name'), width: 190 }, { title: t('Type'), width: 50 }, { title: t('Size'), width: 80 }, { title: t('Link'), width: 280 }, { title: t('Title') }]}
          count={view.rows.length}
          bind:selected={picked}
          fill
          label={t('Download links from the page')}
        >
          {#snippet cell(i, j)}
            {@const r = view.rows[i]}
            {#if j === 0}
              <input type="checkbox" checked={ticked.includes(r.index)} onchange={(e) => tick(r.index, e.currentTarget.checked)} aria-label={r.name} /><FileGlyph
                kind={r.icon}
              /><span class="name">{r.name}</span>
            {:else if j === 1}{r.kind}{:else if j === 2}{r.size}{:else if j === 3}{r.url}{:else}{r.text}{/if}
          {/snippet}
        </ListBox>
      </div>
      <div class="row">
        <button class="btn" onclick={() => (ticked = [...new Set([...ticked, ...(view?.rows.map((r) => r.index) ?? [])])])}>{t('Check all')}</button>
        <button class="btn" onclick={() => (ticked = [])}>{t('Uncheck all')}</button>
        <select class="select" value="" onchange={(e) => tickType(e.currentTarget.value)} aria-label={t('Check all')} title={t('Check all')}>
          <option value=""></option>
          {#each view.types as type (type)}<option value={type}>{type}</option>{/each}
        </select>
        <span class="grow"></span>
        <span class="muted">{t('{shown} of {total} links shown', { shown: view.rows.length, total: view.total })}</span>
        <span class="muted">{tickedShown.length} / {view.rows.length}</span>
      </div>
      <div class="row">
        <label for="rename">{t('Rename')}</label>
        <input
          id="rename"
          class="text grow"
          bind:value={rename}
          placeholder={t('Their own names, or a pattern such as photo_*')}
          spellcheck="false"
        />
      </div>
      {#if renameError}
        <p class="error-text" role="alert">{renameError}</p>
      {:else if renamed.length > 0}
        <p class="muted names">{renamed.slice(0, 3).join(', ')}{renamed.length > 3 ? ', ...' : ''}</p>
      {/if}
      <div class="row">
        <label class="check"><input type="checkbox" bind:checked={keep} />{t('Keep them together as a package')}</label>
        <input
          class="text grow"
          value={name}
          oninput={(e) => (packageName = e.currentTarget.value)}
          disabled={!keep}
          aria-label={t('Package name')}
          spellcheck="false"
        />
      </div>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <div class="row">
        <label for="queue">{t('Queue')}</label>
        <select id="queue" class="select queue" value={queue} onchange={(e) => (pickedQueue = Number(e.currentTarget.value))}>
          {#each view.queues as q (q.id)}<option value={q.id}>{q.name}</option>{/each}
        </select>
        <label class="check"><input type="checkbox" bind:checked={start} />{t('Start the queue now')}</label>
        <span class="grow"></span>
        <button class="btn default" disabled={tickedShown.length === 0 || !view.connected || busy} onclick={() => void ok()}>{t('OK')}</button>
        <button class="btn" onclick={() => void closeWindow()}>{t('Cancel')}</button>
      </div>
    </div>
  {/if}
</Window>

<style>
  .dialog {
    gap: 6px;
  }

  p {
    margin: 0;
  }

  .page {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .list {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    padding-inline-start: 2px;
  }

  .queue {
    width: 150px;
  }

  .names {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
</style>
