<!-- Download File Info: the category and the file a download is saved as,
     before it starts now or waits in a queue. Closing it cancels the
     download, as Cancel does. -->
<script lang="ts">
  import { answerDownload, checkSaveFolder, closeWindow, fitHeight, messageOf, pickSaveFolder, previewZip } from '../../ipc';
  import type { DuplicateChoice, SaveAsView } from '../../ipc/types';
  import { projection } from '../../stores/projection.svelte';
  import { t } from '../../i18n';
  import Window from '../../components/Window.svelte';
  import FileGlyph from '../../components/FileGlyph.svelte';
  import Sheet from '../../components/Sheet.svelte';

  const view: SaveAsView | null = $derived(projection.view?.window === 'saveAs' ? projection.view.view : null);

  /** The dialog's drafts, taken from the view when it first comes. */
  let draft = $state({ category: '', saveAs: '', remember: false, description: '' });
  let loaded = $state(false);
  let error = $state('');
  let busy = $state(false);
  /** The Duplicate download link question: open, the choice, and whether to
   * remember it. */
  let asking = $state(false);
  let duplicate = $state<DuplicateChoice | null>(null);
  let rememberDuplicate = $state(false);
  /** UDM may not write to the folder Save As names, as the shell last
   * checked it, and whether to save there with administrator rights. */
  let denied = $state(false);
  let elevate = $state(false);
  const blocked = $derived(denied && !elevate);
  /** The window's size (the shell's `windows.rs`), and how much the
   * warning for such a folder adds. */
  const WIDTH = 576;
  const HEIGHT = 206;
  const WARNING = 40;

  // Check the folder a moment after the Save As box stops changing.
  $effect(() => {
    if (!loaded) return;
    const saveAs = draft.saveAs;
    const timer = setTimeout(() => {
      checkSaveFolder(saveAs)
        .then((access) => {
          if (draft.saveAs !== saveAs || access.denied === denied) return;
          denied = access.denied;
          void fitHeight(WIDTH, HEIGHT + (denied ? WARNING : 0)).catch(() => undefined);
        })
        .catch(() => undefined);
    }, 300);
    return () => clearTimeout(timer);
  });

  async function browse() {
    error = '';
    try {
      const picked = await pickSaveFolder(draft.saveAs);
      if (picked) draft.saveAs = picked;
    } catch (e) {
      error = messageOf(e);
    }
  }

  $effect(() => {
    if (view && !loaded) {
      draft = { category: view.category, saveAs: view.saveAs, remember: view.rememberFolder, description: view.description };
      loaded = true;
      if (view.duplicate) {
        duplicate = view.duplicate.otherRunning ? 'newName' : 'overwrite';
        asking = true;
      }
    }
  });

  async function answer(start: boolean) {
    if (busy || blocked) return;
    busy = true;
    error = '';
    try {
      await answerDownload({
        category: draft.category,
        saveAs: draft.saveAs,
        rememberFolder: draft.remember,
        description: draft.description,
        start,
        duplicate,
        rememberDuplicate,
        elevate: denied && elevate,
      });
    } catch (e) {
      error = messageOf(e);
    } finally {
      busy = false;
    }
  }
</script>

<Window onescape={() => void closeWindow()}>
  {#if view}
    <form
      class="dialog"
      onsubmit={(e) => {
        e.preventDefault();
        void answer(true);
      }}
    >
      <div class="grid">
        <label for="url">{t('URL')}</label>
        <input id="url" class="text" value={view.url} readonly spellcheck="false" />

        <label for="category">{t('Category')}</label>
        <div class="row">
          <select id="category" class="select category" bind:value={draft.category}>
            {#each view.categories as c (c)}<option value={c}>{t(c)}</option>{/each}
          </select>
          <button class="btn small" type="button" disabled aria-label={t('Add category')} title={t('Add category')}>+</button>
        </div>
        <div class="side big">
          <FileGlyph kind={view.icon} size={40} />
          <span>{view.size}</span>
          {#if view.canPreview}
            <button
              class="btn small"
              type="button"
              title={t('List the files inside, reading only the end of the archive')}
              onclick={() => view && void previewZip(view.id)}>{t('Preview')}</button
            >
          {/if}
        </div>

        <label for="save-as">{t('Save As')}</label>
        <div class="row">
          <input id="save-as" class="text grow" bind:value={draft.saveAs} list="folders" spellcheck="false" />
          <datalist id="folders">
            {#each view.folders as f (f)}<option value={f}></option>{/each}
          </datalist>
          <button class="btn small" type="button" aria-label={t('Browse...')} title={t('Browse...')} onclick={() => void browse()}>...</button>
        </div>

        <fieldset class="group">
          <legend>
            <label class="check">
              <input type="checkbox" bind:checked={draft.remember} />
              {t('Remember this path for "{category}" category', { category: t(draft.category) })}
            </label>
          </legend>
          <input class="text wide" value={view.remembered} readonly aria-label={t('Remembered folder')} />
        </fieldset>

        <label for="description">{t('Description')}</label>
        <input id="description" class="text" bind:value={draft.description} />
      </div>
      {#if denied}
        <div class="denied">
          <p class="error-text" role="status">{t('UDM may not save files in this folder. Saving there needs administrator rights.')}</p>
          {#if view.canElevate}
            <label class="check" title={t('The download runs as you; Windows asks once it is done, before the file is moved there')}
              ><input type="checkbox" bind:checked={elevate} />{t('Save there with administrator rights')}</label
            >
          {/if}
        </div>
      {/if}
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <div class="buttons">
        <button class="btn" type="button" disabled={busy || blocked} onclick={() => void answer(false)}>{t('Download Later')}</button>
        <button class="btn default" type="submit" disabled={busy || blocked}>{t('Start Download')}</button>
        <button class="btn" type="button" onclick={() => void closeWindow()}>{t('Cancel')}</button>
      </div>
    </form>
    {#if asking && view.duplicate}
      <Sheet
        title={t('Duplicate download link')}
        width={420}
        oncancel={() => void closeWindow()}
        onok={() => (asking = false)}
      >
        <input class="text" readonly value={view.url} aria-label={t('Address')} />
        <p>{t('This file already exists in your download list. You may choose one of the following options, or press the Cancel button to skip the download of this file.')}</p>
        <label class="check"><input type="radio" name="dup" value="newName" bind:group={duplicate} />{t('Add the duplicate with a numbered file name')}</label>
        <label class="check"
          ><input type="radio" name="dup" value="overwrite" bind:group={duplicate} disabled={view.duplicate.otherRunning} />{t(
            'Add the duplicate and overwrite the existing file',
          )}</label
        >
        <label class="check"
          ><input type="radio" name="dup" value="existing" bind:group={duplicate} />{t(
            'If existing file is complete, show download complete dialog; otherwise resume it.',
          )}</label
        >
        <label class="check"
          ><input type="checkbox" bind:checked={rememberDuplicate} />{t(
            "Remember my selection and don't show this dialog again. You may change it in UDM Options > Downloads at a later time.",
          )}</label
        >
      </Sheet>
    {/if}
  {/if}
</Window>

<style>
  .dialog {
    gap: 6px;
    padding: 8px 10px 8px 12px;
  }

  .grid {
    display: grid;
    grid-template-columns: 78px 364px 1fr;
    align-items: center;
    gap: 6px 8px;
  }

  .grid > label {
    grid-column: 1;
    text-align: right;
  }

  .grid > :not(label):not(.big) {
    grid-column: 2;
  }

  .category {
    width: 172px;
  }

  .big {
    grid-column: 3;
    grid-row: 1 / span 4;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding-top: 6px;
  }

  fieldset {
    padding: 2px 8px 8px;
  }

  .wide {
    width: 100%;
  }

  .denied {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-left: 86px;
  }

  .buttons {
    justify-content: flex-start;
    padding-left: 86px;
    gap: 22px;
  }

  .buttons .btn {
    min-width: 108px;
  }

  p {
    margin: 0;
  }

  input[type='radio'] {
    margin: 0 5px 0 0;
    accent-color: var(--focus);
  }
</style>
