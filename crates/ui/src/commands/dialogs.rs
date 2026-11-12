//! The dialogs' commands: Add URL, Download File Info, Confirm deletion and
//! Download complete.

use std::path::Path;
use std::sync::Arc;

use tauri::State;
use udm_application::daemon::Command;
use udm_domain::address::Address;
use udm_domain::download::{Answer, Download, DownloadLogin};
use udm_domain::filename::FileName;
use udm_domain::ports::DownloadFiles;

use super::options::Pickers;
use super::{ask, label_of, lang, post};
use crate::actor::{CurrentLanguage, Inbox, Outbox};
use crate::contract::{AnswerInput, DuplicateChoice, FolderAccess, InvokeError, LoginInput};
use crate::errors;
use crate::projection::dialogs::{save_folder_denied, save_in};
use crate::windows::Label;
use udm_domain::settings::Duplicate;

/// Add the address typed in Add URL, with its login, and close the dialog.
/// The password goes to the daemon only; no view carries it back.
#[tauri::command]
#[specta::specta]
pub async fn add_download(
    window: tauri::WebviewWindow,
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    address: String,
    login: Option<LoginInput>,
) -> Result<(), InvokeError> {
    let address = Address::parse(address.trim()).map_err(|e| errors::address(lang(&current), &e))?;
    let login = login
        .filter(|l| !l.user.trim().is_empty())
        .map(|l| DownloadLogin { username: l.user.trim().to_string(), password: Some(l.password) });
    let download = Download { url: address.into(), login, ..Download::default() };
    ask(&outbox, &current, |reply| Inbox::Send { command: Box::new(Command::Add(download)), reply: Some(reply) })
        .await?;
    let _ = window.destroy();
    Ok(())
}

/// The download files' port, for the write check of the folder a download
/// is saved in (a managed type).
pub type Files = Arc<dyn DownloadFiles>;

/// Whether UDM may save in the folder Download File Info's Save As names,
/// checked on a worker since it writes a probe file there.
#[tauri::command]
#[specta::specta]
pub async fn check_save_folder(
    files: State<'_, Files>,
    current: State<'_, CurrentLanguage>,
    save_as: String,
) -> Result<FolderAccess, InvokeError> {
    let files = Arc::clone(&files);
    let denied = tauri::async_runtime::spawn_blocking(move || save_folder_denied(&save_as, |f| files.can_write(f)))
        .await
        .map_err(|_| errors::offline(lang(&current)))?;
    Ok(FolderAccess { denied })
}

/// Download File Info's Browse: another folder for the file, keeping its
/// name. `None` when the user cancels.
#[tauri::command]
#[specta::specta]
pub async fn pick_save_folder(
    pickers: State<'_, Pickers>,
    current: State<'_, CurrentLanguage>,
    save_as: String,
) -> Result<Option<String>, InvokeError> {
    let lang = lang(&current);
    let pickers = Arc::clone(&pickers);
    let title = lang.tr("Save to").to_string();
    let start = save_as.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let start = Path::new(start.trim()).parent().filter(|p| p.is_absolute());
        pickers.pick_folder(&title, start)
    })
    .await
    .map_err(|_| errors::offline(lang))?;
    Ok(picked.map(|folder| save_in(&save_as, &folder.display().to_string())))
}

/// Download File Info's Start Download or Download Later.
#[tauri::command]
#[specta::specta]
pub async fn answer_download(
    window: tauri::WebviewWindow,
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    files: State<'_, Files>,
    answer: AnswerInput,
) -> Result<(), InvokeError> {
    let Label::SaveAs(id) = label_of(&window, &current)? else {
        return Err(errors::gone(lang(&current)));
    };
    // Administrator rights only for a folder that needs them, checked
    // again here on a worker: the webview's word is not taken for it.
    let elevate = answer.elevate && {
        let (files, save_as) = (Arc::clone(&files), answer.save_as.clone());
        tauri::async_runtime::spawn_blocking(move || save_folder_denied(&save_as, |f| files.can_write(f)))
            .await
            .map_err(|_| errors::offline(lang(&current)))?
    };
    let path = Path::new(answer.save_as.trim());
    let name = path
        .file_name()
        .and_then(|n| FileName::parse(&n.to_string_lossy()).ok())
        .ok_or_else(|| errors::invalid(lang(&current), "Type a name for the file."))?;
    let folder = path.parent().map(|p| p.display().to_string()).filter(|f| !f.is_empty());
    let answer = Answer {
        filename: Some(name.into_string()),
        folder,
        start: answer.start,
        category: Some(answer.category.chars().take(200).collect()),
        description: Some(answer.description.chars().take(2000).collect()),
        remember_folder: Some(answer.remember_folder),
        duplicate: answer.duplicate.map(|d| match d {
            DuplicateChoice::Ask => Duplicate::Ask,
            DuplicateChoice::NewName => Duplicate::NewName,
            DuplicateChoice::Overwrite => Duplicate::Overwrite,
            DuplicateChoice::Existing => Duplicate::Existing,
        }),
        remember_duplicate: answer.remember_duplicate,
        elevate,
        ..Answer::start(id)
    };
    ask(&outbox, &current, |reply| Inbox::Answer { answer: Box::new(answer), reply }).await
}

/// Confirm deletion's Yes.
#[tauri::command]
#[specta::specta]
pub async fn remove_downloads(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    delete_files: bool,
    dont_ask: bool,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Remove { delete_files, dont_ask })
}

/// Download complete's "Don't show this dialog again".
#[tauri::command]
#[specta::specta]
pub async fn hide_done_dialogs(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::NoDoneDialogs)
}
