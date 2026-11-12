<!-- The main window: the menu bar, the toolbar, the categories and the
     download list, as the download manager UDM follows lays them out. -->
<script lang="ts">
  import {
    browseCategory,
    clipboardText,
    importList,
    messageOf,
    moveDownload,
    openClipboardLinks,
    openDownload,
    openWindow,
    queueSelection,
    saveCategory,
    setLimiter,
    setListFont,
    setPanelWidth,
    setToolbar,
    showDropTarget,
    showProperties,
    quitApp,
    resizeColumn,
    runListAction,
    selectNode,
    findDownload,
    selectRows,
    showDownload,
    sortList,
    toggleCategories,
    toggleNode,
  } from '../../ipc';
  import type { CategoryInput, ColumnKey, FindInput, ListAction, MainView, Row, TreeNode } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import MenuBar from '../../components/MenuBar.svelte';
  import Toolbar, { type Tool } from '../../components/Toolbar.svelte';
  import CategoryTree from '../../components/CategoryTree.svelte';
  import DownloadTable from '../../components/DownloadTable.svelte';
  import PopupMenu from '../../components/PopupMenu.svelte';
  import { item, sep, sub, type MenuEntry } from '../../components/menu';
  import FindSheet from './FindSheet.svelte';
  import ExportSheet from './ExportSheet.svelte';
  import LimiterSheet from './LimiterSheet.svelte';
  import ColumnsSheet from './ColumnsSheet.svelte';
  import CategorySheet from './CategorySheet.svelte';
  import FontSheet from './FontSheet.svelte';
  import ToolbarSheet from './ToolbarSheet.svelte';
  import TipSheet from './TipSheet.svelte';
  import Sheet from '../../components/Sheet.svelte';

  type SheetOpen =
    | { kind: 'find' | 'export' | 'limiter' | 'columns' | 'font' | 'toolbar' | 'tip' }
    | { kind: 'category'; original: string | null; start: CategoryInput }
    | { kind: 'note'; text: string };
  let sheet = $state<SheetOpen | null>(null);
  let query = $state<FindInput>({ text: '', name: true, description: false, sites: false, matchCase: false, whole: false });

  async function tell(p: Promise<unknown>) {
    try {
      await p;
    } catch (e) {
      sheet = { kind: 'note', text: messageOf(e) };
    }
  }

  async function fromClipboard() {
    let text: string;
    try {
      text = await clipboardText();
    } catch {
      sheet = { kind: 'note', text: t('UDM could not read the clipboard.') };
      return;
    }
    await tell(openClipboardLinks(text));
  }

  function findNext() {
    if (query.text.trim()) void tell(findDownload($state.snapshot(query)));
    else sheet = { kind: 'find' };
  }

  const blankCategory: CategoryInput = { name: '', extensions: '', sitesOn: false, sites: '', folder: '', rememberFolder: false };

  const view: MainView | null = $derived(projection.view?.window === 'main' ? projection.view.view : null);
  const can = $derived(view?.chrome.can);
  const one: Row | undefined = $derived(
    view && view.chrome.selected.length === 1 ? view.rows.find((r) => r.id === view.chrome.selected[0]) : undefined,
  );

  let listMenu: { x: number; y: number } | null = $state(null);
  let treeMenu: { x: number; y: number; node: TreeNode } | null = $state(null);
  /** The panel's width while it is dragged; otherwise the shell's. */
  let dragWidth = $state<number | null>(null);
  const panelWidth = $derived(dragWidth ?? view?.chrome.panelWidth ?? 143);
  // The tip of the day opens by itself once, when the window starts.
  let tipOffered = false;
  $effect(() => {
    if (view?.chrome.showTip && !tipOffered) {
      tipOffered = true;
      sheet = { kind: 'tip' };
    }
  });

  const act = (action: ListAction) => () => void runListAction(action);
  const when = (ok: boolean | undefined, run: () => void) => (ok ? run : undefined);

  function open(row: Row) {
    if (row.finished) void openDownload(row.id, 'file');
    else void showDownload(row.id);
  }

  const queueItems = (start: boolean): MenuEntry[] =>
    (view?.chrome.queues ?? []).map((q) =>
      item(q.name, when(can?.connected && (start ? !q.running : q.running), act(start ? { action: 'startQueue', id: q.id } : { action: 'stopQueue', id: q.id }))),
    );

  const arrange = (key: ColumnKey, label: string): MenuEntry => {
    const head = view?.chrome.columns.find((c) => c.key === key);
    return item(label, () => void sortList(key), { checked: !!head?.sort });
  };

  const menus = $derived([
    {
      label: '&Tasks',
      items: [
        item('&Add new download', () => void openWindow('addUrl')),
        item('Add &batch download', () => void openWindow('batch')),
        item('Add batch download from &clipboard', when(can?.connected, () => void fromClipboard())),
        sep,
        item('Run site &grabber', when(can?.connected, () => void openWindow('grabber'))),
        item('Show &drop target', () => view && void showDropTarget(!view.chrome.dropTarget), { checked: !!view?.chrome.dropTarget }),
        sep,
        item('E&xport...', () => (sheet = { kind: 'export' })),
        item('&Import...', () => void tell(importList())),
        sep,
        item('&Exit', () => void quitApp()),
      ],
    },
    {
      label: '&File',
      items: [
        item('&Download Now', when(can?.resume, act({ action: 'resume' }))),
        item('&Stop Download', when(can?.stop, act({ action: 'stop' }))),
        item('R&edownload', when(can?.redownload, act({ action: 'redownload' }))),
        sep,
        item('&Remove', when(can?.delete, act({ action: 'delete' })), { shortcut: 'Del' }),
      ],
    },
    {
      label: '&Downloads',
      items: [
        item('&Pause All', when(can?.stopAll, act({ action: 'stopAll' }))),
        item('&Resume All', when(can?.resumeAll, act({ action: 'resumeAll' }))),
        item('&Delete All Completed', when(can?.deleteCompleted, act({ action: 'deleteCompleted' }))),
        sep,
        item('Find (Ctrl-F)', () => (sheet = { kind: 'find' })),
        item('Find Next (F3)', findNext),
        sep,
        item('S&cheduler', when(can?.connected, () => void openWindow('scheduler'))),
        sub('Start &queue', queueItems(true)),
        sub('S&top queue', queueItems(false)),
        sep,
        sub('Speed &Limiter', [
          item('Turn o&n', view && !view.chrome.limiter.on ? () => view && void tell(setLimiter({ ...view.chrome.limiter, on: true })) : undefined),
          item('Turn of&f', view?.chrome.limiter.on ? () => view && void tell(setLimiter({ ...view.chrome.limiter, on: false })) : undefined),
          sep,
          item('S&ettings...', () => (sheet = { kind: 'limiter' })),
        ]),
        sep,
        item('&Options', when(can?.connected, () => void openWindow('options'))),
      ],
    },
    {
      label: '&View',
      items: [
        item('Hide &categories', () => void toggleCategories(), { checked: !view?.chrome.categoriesPanel }),
        sub('&Arrange files', [
          arrange('name', 'By &File Name'),
          arrange('size', 'By &Size'),
          arrange('status', 'By St&atus'),
          arrange('timeLeft', 'By &Time Left'),
          arrange('speed', 'By Transfer &Rate'),
          arrange('lastTry', 'By &Last Try Date'),
          arrange('description', 'By &Description'),
          arrange('saveTo', 'By Save &Path'),
          arrange('referrer', 'By R&eferrer'),
          arrange('page', 'By Parent &Web Page'),
        ]),
        sep,
        sub('&Toolbar', [
          item('&Large Buttons', () => view && void setToolbar(false, view.chrome.toolbar), { checked: !view?.chrome.toolbarSmall }),
          item('&Small Buttons', () => view && void setToolbar(true, view.chrome.toolbar), { checked: !!view?.chrome.toolbarSmall }),
          sep,
          item('Customize...', () => (sheet = { kind: 'toolbar' })),
        ]),
        sub('&Font', [item('&Choose Font...', () => (sheet = { kind: 'font' })), item('&Reset to default', () => void setListFont(null))]),
        sub('Tray &icon', [], true),
        item('C&ustomize URL List...', () => (sheet = { kind: 'columns' })),
        sub('&Language', [], true),
      ],
    },
    {
      label: '&Help',
      items: [
        item('H&elp contents', undefined),
        item('&Keyboard shortcuts', undefined),
        item('Tip of the &Day...', () => (sheet = { kind: 'tip' })),
        sep,
        item('&About UDM', () => void openWindow('about')),
      ],
    },
  ]);

  const queueMenu = (start: boolean) => queueItems(start);
  const mainQueue = $derived(view?.chrome.queues[0]);

  const allTools: Tool[] = $derived([
    { icon: 'add', label: 'Add URL', hint: 'Add a download by its address', enabled: true, run: () => void openWindow('addUrl') },
    { icon: 'resume', label: 'Resume', hint: 'Start or resume the selected downloads', enabled: !!can?.resume, run: act({ action: 'resume' }) },
    { icon: 'stop', label: 'Stop', hint: 'Stop the selected downloads', enabled: !!can?.stop, run: act({ action: 'stop' }) },
    { icon: 'stopAll', label: 'Stop All', hint: 'Stop every download', enabled: !!can?.stopAll, run: act({ action: 'stopAll' }) },
    { icon: 'delete', label: 'Delete', hint: 'Remove the selected downloads from the list', enabled: !!can?.delete, run: act({ action: 'delete' }) },
    {
      icon: 'deleteCompleted',
      label: 'Delete Completed',
      hint: 'Remove every finished download from the list',
      enabled: !!can?.deleteCompleted,
      run: act({ action: 'deleteCompleted' }),
    },
    {
      icon: 'options',
      label: 'Options',
      hint: 'Browser capture, file types, connection, proxy, logins and more',
      enabled: !!can?.connected,
      run: () => void openWindow('options'),
    },
    {
      icon: 'scheduler',
      label: 'Scheduler',
      hint: 'Queues and their schedules',
      enabled: !!can?.connected,
      run: () => void openWindow('scheduler'),
    },
    {
      icon: 'startQueue',
      label: 'Start Queue',
      hint: 'Start the main queue, or pick a queue from the arrow',
      enabled: !!can?.connected && !!mainQueue && !mainQueue.running,
      run: () => mainQueue && void runListAction({ action: 'startQueue', id: mainQueue.id }),
      more: queueMenu(true),
    },
    {
      icon: 'stopQueue',
      label: 'Stop Queue',
      hint: 'Stop the main queue, or pick a queue from the arrow',
      enabled: !!can?.connected && !!mainQueue?.running,
      run: () => mainQueue && void runListAction({ action: 'stopQueue', id: mainQueue.id }),
      more: queueMenu(false),
    },
    {
      icon: 'grabber',
      label: 'Grabber',
      hint: 'Find and download the files a site links to',
      enabled: !!can?.connected,
      run: () => void openWindow('grabber'),
    },
  ]);

  /** The toolbar's buttons in the order the user set, with separators. */
  const tools: (Tool | 'separator')[] = $derived(
    (view?.chrome.toolbar ?? []).flatMap((id): (Tool | 'separator')[] => {
      if (id === 'separator') return ['separator'];
      const tool = allTools.find((x) => x.icon === id);
      return tool ? [tool] : [];
    }),
  );
  const toolLabels = $derived(Object.fromEntries(allTools.map((x) => [x.icon, x.label])));

  const listItems: MenuEntry[] = $derived([
    item('Open', when(can?.open && !!one, () => one && void openDownload(one.id, 'file'))),
    item('Open with...', undefined),
    item('Open folder', when(!!one?.saveTo, () => one && void openDownload(one.id, 'folder'))),
    sep,
    item('Move/Rename (Ctrl-M)', when(!!one, () => one && void tell(moveDownload(one.id, one.name)))),
    sep,
    item('Redownload', when(can?.redownload, act({ action: 'redownload' }))),
    sep,
    item('Resume Download', when(can?.resume, act({ action: 'resume' }))),
    item('Stop Download', when(can?.stop, act({ action: 'stop' }))),
    sep,
    item('Refresh download address', undefined),
    sep,
    item('Remove', when(can?.delete, act({ action: 'delete' }))),
    sep,
    sub(
      'Add to queue',
      (view?.chrome.queues ?? []).map((q) => item(q.name, when(can?.delete, () => void queueSelection(q.id)))),
    ),
    item('Delete from queue', when(can?.delete, () => void queueSelection(null))),
    sep,
    sub('On Double click', [], true),
    sep,
    item('Properties', when(!!one, () => one && void showProperties(one.id))),
  ]);

  /** The category a tree node names, `General` for the branches. */
  const categoryOf = (node: TreeNode) => (typeof node.icon === 'object' ? (node.key.split('/')[1] ?? null) : null);
  const treeItems: MenuEntry[] = $derived.by(() => {
    const node = treeMenu?.node;
    const name = node ? categoryOf(node) : null;
    const current = view?.chrome.categories.find((c) => c.name === name);
    return [
      item('Browse', node ? () => void browseCategory(name ?? 'General') : undefined),
      item('Properties', current ? () => (sheet = { kind: 'category', original: current.name, start: current }) : undefined),
      sep,
      item('Add category', () => (sheet = { kind: 'category', original: null, start: blankCategory })),
      item('Delete', current ? () => void tell(saveCategory(current.name, null)) : undefined),
    ];
  });

  function dragPanel(e: PointerEvent) {
    const start = e.clientX;
    const base = panelWidth;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => (dragWidth = Math.max(100, Math.min(400, base + ev.clientX - start)));
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      if (dragWidth !== null) void setPanelWidth(dragWidth).finally(() => (dragWidth = null));
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (sheet) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      sheet = { kind: 'find' };
    } else if (e.key === 'F3') {
      e.preventDefault();
      findNext();
    }
  }}
/>

<Window>
  {#if view}
    <div class="main">
      <MenuBar {menus} />
      <Toolbar {tools} small={view.chrome.toolbarSmall} />
      {#if view.chrome.banner}
        <div class="banner" class:error={view.chrome.banner.error} role="status">{view.chrome.banner.text}</div>
      {/if}
      <div class="body">
        {#if view.chrome.categoriesPanel}
          <div class="panel" style:width="{panelWidth}px">
            <CategoryTree
              nodes={view.chrome.tree}
              onselect={(key) => void selectNode(key)}
              ontoggle={(key) => void toggleNode(key)}
              onclose={() => void toggleCategories()}
              onmenu={(node, x, y) => (treeMenu = { node, x, y })}
            />
          </div>
          <div class="splitter" role="separator" aria-orientation="vertical" onpointerdown={dragPanel}></div>
        {/if}
        <div class="list">
          <DownloadTable
            rows={view.rows}
            columns={view.chrome.columns}
            selected={view.chrome.selected}
            onselect={(ids) => void selectRows(ids)}
            onsort={(key) => void sortList(key)}
            onresize={(key, width) => void resizeColumn(key, width)}
            onopen={open}
            ondelete={() => can?.delete && void runListAction({ action: 'delete' })}
            onselectall={() => void runListAction({ action: 'selectAll' })}
            onmenu={(x, y) => (listMenu = { x, y })}
            mono={view.chrome.listMono}
            size={view.chrome.listSize}
          />
        </div>
      </div>
    </div>
    {#if listMenu}
      <PopupMenu items={listItems} x={listMenu.x} y={listMenu.y} label={t('Downloads')} onclose={() => (listMenu = null)} />
    {/if}
    {#if treeMenu}
      <PopupMenu items={treeItems} x={treeMenu.x} y={treeMenu.y} label={treeMenu.node.label} onclose={() => (treeMenu = null)} />
    {/if}
    {#if sheet?.kind === 'find'}
      <FindSheet bind:query onclose={() => (sheet = null)} />
    {:else if sheet?.kind === 'export'}
      <ExportSheet selected={view.chrome.selected.length} onclose={() => (sheet = null)} />
    {:else if sheet?.kind === 'limiter'}
      <LimiterSheet start={view.chrome.limiter} onclose={() => (sheet = null)} />
    {:else if sheet?.kind === 'columns'}
      <ColumnsSheet start={view.chrome.allColumns} defaults={view.chrome.defaultColumns} onclose={() => (sheet = null)} />
    {:else if sheet?.kind === 'category'}
      <CategorySheet original={sheet.original} start={sheet.start} onclose={() => (sheet = null)} />
    {:else if sheet?.kind === 'font'}
      <FontSheet mono={view.chrome.listMono} size={view.chrome.listSize} onclose={() => (sheet = null)} />
    {:else if sheet?.kind === 'toolbar'}
      <ToolbarSheet current={view.chrome.toolbar} small={view.chrome.toolbarSmall} labels={toolLabels} onclose={() => (sheet = null)} />
    {:else if sheet?.kind === 'tip'}
      <TipSheet tips={view.chrome.tips} next={view.chrome.nextTip} onStartup={view.chrome.tipsOnStartup} onclose={() => (sheet = null)} />
    {:else if sheet?.kind === 'note'}
      <Sheet title={t('UDM')} width={320} oncancel={() => (sheet = null)}>
        <p class="note">{sheet.text}</p>
      </Sheet>
    {/if}
  {/if}
</Window>

<style>
  .main {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--face);
  }

  .banner {
    padding: 3px 10px;
    background: var(--hover);
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  .banner.error {
    color: var(--error);
  }

  .body {
    flex: 1;
    display: flex;
    min-height: 0;
    padding: 4px 8px 8px 14px;
  }

  .panel {
    flex: none;
    min-width: 0;
  }

  .splitter {
    width: 5px;
    flex: none;
    cursor: col-resize;
  }

  .list {
    flex: 1;
    min-width: 0;
  }

  .note {
    margin: 0;
  }
</style>
