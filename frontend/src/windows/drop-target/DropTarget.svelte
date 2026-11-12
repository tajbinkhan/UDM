<!-- The drop target: a small box over other windows. A link dropped on it
     is added; dragging it moves it; a double click shows the main window;
     the menu closes it. -->
<script lang="ts">
  import { addDownload, dragWindow, openWindow, showDropTarget } from '../../ipc';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import PopupMenu from '../../components/PopupMenu.svelte';
  import { item } from '../../components/menu';
  import logo from '../../../../assets/icon-256.png';

  let over = $state(false);
  let menu = $state<{ x: number; y: number } | null>(null);
  const connected = $derived(projection.view?.window === 'dropTarget' && projection.view.view.connected);

  function ondrop(e: DragEvent) {
    e.preventDefault();
    over = false;
    const text = e.dataTransfer?.getData('text/uri-list') || e.dataTransfer?.getData('text/plain') || '';
    const link = text.split(/\r?\n/).find((l) => l.trim() && !l.startsWith('#'));
    if (link) void addDownload(link.trim(), null).catch(() => undefined);
  }
</script>

<Window>
  <div
    class="target"
    class:over
    class:off={!connected}
    role="button"
    tabindex="0"
    aria-label={t('Drop a link here to download it')}
    title={t('Drag it to move it, double-click it to open UDM, right-click it to close it')}
    ondragover={(e) => {
      e.preventDefault();
      over = true;
    }}
    ondragleave={() => (over = false)}
    {ondrop}
    onmousedown={(e) => e.button === 0 && e.detail === 1 && void dragWindow()}
    ondblclick={() => void openWindow('main')}
    oncontextmenu={(e) => {
      e.preventDefault();
      menu = { x: e.clientX, y: e.clientY };
    }}
    onkeydown={(e) => e.key === 'Enter' && void openWindow('main')}
  >
    <img src={logo} width="36" height="36" alt="" draggable="false" />
  </div>
  {#if menu}
    <PopupMenu
      items={[item('Open UDM', () => void openWindow('main')), item('Hide', () => void showDropTarget(false))]}
      x={menu.x}
      y={menu.y}
      onclose={() => (menu = null)}
    />
  {/if}
</Window>

<style>
  .target {
    display: grid;
    place-items: center;
    width: 100vw;
    height: 100vh;
    background: var(--face);
    border: 1px solid var(--frame);
  }

  .target.over {
    background: var(--selected);
    border-color: var(--focus);
  }

  .target.off img {
    opacity: 0.5;
  }
</style>
