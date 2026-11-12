//! The dialogs' commands: Add URL, Download File Info, Confirm deletion and
//! Download complete.

use std::path::Path;

use tauri::State;
use udm_application::daemon::Command;
use udm_domain::address::Address;
use udm_domain::download::{Answer, Download, DownloadLogin};
use udm_domain::filename::FileName;

use super::{ask, label_of, lang, post};
use crate::actor::{CurrentLanguage, Inbox, Outbox};
use crate::contract::{AnswerInput, DuplicateChoice, InvokeError, LoginInput};
use crate::errors;
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

/// Download File Info's Start Download or Download Later.
#[tauri::command]
#[specta::specta]
pub async fn answer_download(
    window: tauri::WebviewWindow,
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    answer: AnswerInput,
) -> Result<(), InvokeError> {
    let Label::SaveAs(id) = label_of(&window, &current)? else {
        return Err(errors::gone(lang(&current)));
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
