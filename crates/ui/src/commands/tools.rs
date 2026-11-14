//! The smaller tools' commands: batch downloads, the links window, import
//! and export, find, properties and moving files, the speed limiter, the
//! columns, categories, queues and the drop target.

use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use tauri::State;
use udm_application::window::{FileKind, ListFiles};
use udm_domain::download::{DownloadLogin, Properties};
use udm_domain::id::{DownloadId, QueueId};

use super::options::Pickers;
use super::{label_of, lang, post};
use crate::actor::{CurrentLanguage, Inbox, Outbox};
use crate::contract::{
    BatchInput, BatchPreview, CategoryInput, ColumnChoice, ExportFormat, ExportScope, FindInput, Id, InvokeError,
    LimiterDraft, ListFontInput, LoginInput, PropertiesInput,
};
use crate::errors;
use crate::i18n::fill;
use crate::state::UiMessage;
use crate::windows::Label;

/// Reading and writing download lists (a managed type).
pub type Lists = Arc<dyn ListFiles>;

/// Wait off the main thread for the actor's reply.
async fn wait<T: Send + 'static>(
    current: &State<'_, CurrentLanguage>,
    rx: mpsc::Receiver<T>,
) -> Result<T, InvokeError> {
    let offline = errors::offline(lang(current));
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(10)))
        .await
        .map_err(|_| offline.clone())?
        .map_err(|_| offline)
}

fn login_of(login: Option<LoginInput>) -> Option<DownloadLogin> {
    login
        .filter(|l| !l.user.trim().is_empty())
        .map(|l| DownloadLogin { username: l.user.trim().to_string(), password: Some(l.password) })
}

/// Batch download's first, second and last addresses.
#[tauri::command]
#[specta::specta]
pub async fn preview_batch(
    current: State<'_, CurrentLanguage>,
    input: BatchInput,
) -> Result<BatchPreview, InvokeError> {
    crate::tools::batch_preview(&input).map_err(|e| errors::batch(lang(&current), &e))
}

/// Batch download's OK: every address, waiting in the main queue.
#[tauri::command]
#[specta::specta]
pub async fn add_batch(
    window: tauri::WebviewWindow,
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    input: BatchInput,
    login: Option<LoginInput>,
) -> Result<(), InvokeError> {
    let items =
        crate::tools::batch_downloads(&input, login_of(login)).map_err(|e| errors::batch(lang(&current), &e))?;
    let (tx, rx) = mpsc::channel();
    post(&outbox, &current, Inbox::AddItems { items, queue: QueueId::MAIN, start: false, reply: tx })?;
    wait(&current, rx).await??;
    let _ = window.destroy();
    Ok(())
}

/// Tasks > Add batch download from clipboard, with the clipboard's text.
#[tauri::command]
#[specta::specta]
pub async fn open_clipboard_links(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    text: String,
) -> Result<(), InvokeError> {
    let text: String = text.chars().take(1_000_000).collect();
    let list = crate::tools::LinkList { file: None, items: crate::tools::links_of(&text), rejected: 0 };
    if list.items.is_empty() {
        return Err(errors::invalid(lang(&current), "The clipboard holds no http, https or ftp link."));
    }
    post(&outbox, &current, Inbox::Ui(UiMessage::Links(Some(Box::new(list)))))
}

/// The links window's OK: the ticked links, waiting in `queue`.
#[tauri::command]
#[specta::specta]
pub async fn add_links(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    indices: Vec<u32>,
    queue: Id,
    start: bool,
) -> Result<(), InvokeError> {
    let (tx, rx) = mpsc::channel();
    post(&outbox, &current, Inbox::AddLinks { indices, queue: QueueId::new(queue.0), start, reply: tx })?;
    wait(&current, rx).await?
}

/// Tasks > Import: a list file, shown in the links window to tick.
#[tauri::command]
#[specta::specta]
pub async fn import_list(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    pickers: State<'_, Pickers>,
    lists: State<'_, Lists>,
) -> Result<(), InvokeError> {
    let lang = lang(&current);
    let (pickers, lists) = (Arc::clone(&pickers), Arc::clone(&lists));
    let kinds = [FileKind::new(lang.tr("Lists"), &["txt", "json", "ef2"]), FileKind::new(lang.tr("All files"), &["*"])];
    let title = lang.tr("Import a download list").to_string();
    let read = tauri::async_runtime::spawn_blocking(move || {
        let path = pickers.pick_file(&title, &kinds, None)?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Some(lists.read(&path).and_then(|text| lists.import(&text)).map(|(items, rejected)| (name, items, rejected)))
    })
    .await
    .map_err(|_| errors::offline(lang))?;
    let Some(read) = read else { return Ok(()) };
    let (name, items, rejected) = read.map_err(|e| errors::list(&e))?;
    post(
        &outbox,
        &current,
        Inbox::Ui(UiMessage::Links(Some(Box::new(crate::tools::LinkList { file: Some(name), items, rejected })))),
    )
}

/// Tasks > Export: the downloads `scope` names, saved where the user picks.
/// Says what it did, or nothing when the user cancelled.
#[tauri::command]
#[specta::specta]
pub async fn export_list(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    pickers: State<'_, Pickers>,
    lists: State<'_, Lists>,
    scope: ExportScope,
    format: ExportFormat,
) -> Result<Option<String>, InvokeError> {
    let lang = lang(&current);
    let (tx, rx) = mpsc::channel();
    post(&outbox, &current, Inbox::ExportText { scope, format, reply: tx })?;
    let (text, count) = wait(&current, rx).await?;
    let (ext, name) = match format {
        ExportFormat::Ef2 => ("ef2", "downloads.ef2"),
        ExportFormat::Text => ("txt", "downloads.txt"),
        ExportFormat::Json => ("json", "downloads.json"),
    };
    let (pickers, lists) = (Arc::clone(&pickers), Arc::clone(&lists));
    let title = lang.tr("Export download list").to_string();
    let written = tauri::async_runtime::spawn_blocking(move || {
        let path = pickers.save_file(&title, name, &[FileKind::new(ext, &[ext])])?;
        Some(lists.write(&path, &text).map(|()| path))
    })
    .await
    .map_err(|_| errors::offline(lang))?;
    match written {
        None => Ok(None),
        Some(Err(e)) => Err(errors::list(&e)),
        Some(Ok(path)) => Ok(Some(fill(
            lang.tr("Saved {count} downloads to {path}."),
            &[("count", &count.to_string()), ("path", &path.display().to_string())],
        ))),
    }
}

/// Find file: select the next match. False when nothing matches.
#[tauri::command]
#[specta::specta]
pub async fn find_download(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    input: FindInput,
) -> Result<bool, InvokeError> {
    let (tx, rx) = mpsc::channel();
    post(&outbox, &current, Inbox::Find { input: Box::new(input), reply: tx })?;
    wait(&current, rx).await
}

/// Show a download's properties.
#[tauri::command]
#[specta::specta]
pub async fn show_properties(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::ShowProperties(DownloadId::new(id.0))))
}

/// The properties window's OK.
#[tauri::command]
#[specta::specta]
pub async fn save_properties(
    window: tauri::WebviewWindow,
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    input: PropertiesInput,
) -> Result<(), InvokeError> {
    let Label::Properties(id) = label_of(&window, &current)? else {
        return Err(errors::gone(lang(&current)));
    };
    let (tx, rx) = mpsc::channel();
    post(&outbox, &current, Inbox::Properties { id, input: Box::new(input), reply: tx })?;
    wait(&current, rx).await??;
    let _ = window.destroy();
    Ok(())
}

/// Move/Rename: where to move download `id`'s file, picked in the system's
/// save dialog, starting from `name`.
#[tauri::command]
#[specta::specta]
pub async fn move_download(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    pickers: State<'_, Pickers>,
    id: Id,
    name: String,
) -> Result<(), InvokeError> {
    let lang = lang(&current);
    let pickers = Arc::clone(&pickers);
    let title = lang.tr("Move or rename the file").to_string();
    let name = Path::new(&name).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let picked = tauri::async_runtime::spawn_blocking(move || pickers.save_file(&title, &name, &[]))
        .await
        .map_err(|_| errors::offline(lang))?;
    let Some(path) = picked else { return Ok(()) };
    let changes: Properties = crate::tools::moved_to(&path);
    let command = udm_application::daemon::Command::SetProperties { id: DownloadId::new(id.0), changes };
    post(&outbox, &current, Inbox::Send { command: Box::new(command), reply: None })
}

/// Downloads > Speed Limiter: on or off, and its speed.
#[tauri::command]
#[specta::specta]
pub async fn set_limiter(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    draft: LimiterDraft,
) -> Result<(), InvokeError> {
    let (tx, rx) = mpsc::channel();
    post(&outbox, &current, Inbox::Limiter { draft, reply: tx })?;
    wait(&current, rx).await?
}

/// View > Customize URL List: the columns shown, in order, and widths.
#[tauri::command]
#[specta::specta]
pub async fn set_columns(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    columns: Vec<ColumnChoice>,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::Columns(crate::tools::columns_of(&columns))))
}

/// A category's properties sheet: save `input` as category `original` (a
/// new one when `None`), or delete `original` when `input` is `None`.
#[tauri::command]
#[specta::specta]
pub async fn save_category(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    original: Option<String>,
    input: Option<CategoryInput>,
) -> Result<(), InvokeError> {
    let (tx, rx) = mpsc::channel();
    post(&outbox, &current, Inbox::Category { original, input: input.map(Box::new), reply: tx })?;
    wait(&current, rx).await?
}

/// The category menu's Browse: open its folder.
#[tauri::command]
#[specta::specta]
pub async fn browse_category(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    name: String,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::BrowseCategory(name))
}

/// Add to queue, or Delete from queue when `queue` is `None`, for the
/// selected downloads.
#[tauri::command]
#[specta::specta]
pub async fn queue_selection(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    queue: Option<Id>,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::QueueSelection(queue.map(|q| QueueId::new(q.0))))
}

/// Tasks > Show drop target, and the drop target's own close.
#[tauri::command]
#[specta::specta]
pub async fn show_drop_target(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    on: bool,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::DropTarget(on))
}

/// The categories panel's width, once dragged.
#[tauri::command]
#[specta::specta]
pub async fn set_panel_width(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    width: u32,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::PanelWidth(width.min(1000) as f32)))
}

/// View > Font: the list's font, or the window's own with `None`.
#[tauri::command]
#[specta::specta]
pub async fn set_list_font(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    font: Option<ListFontInput>,
) -> Result<(), InvokeError> {
    let font = font.map(|f| crate::state::ListFont { mono: f.mono, size: f.size.min(100) as f32 });
    post(&outbox, &current, Inbox::Ui(UiMessage::ListFont(font)))
}

/// View > Toolbar: large or small buttons, and which ones in what order.
#[tauri::command]
#[specta::specta]
pub async fn set_toolbar(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    small: bool,
    buttons: Vec<String>,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::Toolbar { small, buttons }))
}

/// View > Language, or the first-start picker's choice: a language's code,
/// or empty for the system's.
#[tauri::command]
#[specta::specta]
pub async fn set_language(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    code: String,
) -> Result<(), InvokeError> {
    let known = code.is_empty() || crate::i18n::LANGUAGES.iter().any(|l| l.code == code);
    if !known {
        return Err(errors::invalid(lang(&current), "That is not a language."));
    }
    post(&outbox, &current, Inbox::SetLanguage(code))
}

/// Tip of the Day closed.
#[tauri::command]
#[specta::specta]
pub async fn close_tip(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    next: u32,
    on_startup: bool,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::TipClosed { next, on_startup })
}
