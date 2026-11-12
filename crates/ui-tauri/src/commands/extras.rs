//! The commands of the windows the daemon asks for, the packages, and the
//! update check.

use std::sync::mpsc;
use std::time::Duration;

use tauri::State;
use udm_application::daemon::Command;
use udm_domain::download::Download;
use udm_domain::id::{DownloadId, PackageId};
use udm_domain::package::Password;

use super::{label_of, lang, post};
use crate::actor::{CurrentLanguage, Inbox, Outbox};
use crate::contract::{Id, InvokeError, PickInput};
use crate::errors;
use crate::state::UiMessage;

async fn wait(
    current: &State<'_, CurrentLanguage>,
    rx: mpsc::Receiver<Result<(), InvokeError>>,
) -> Result<(), InvokeError> {
    let offline = errors::offline(lang(current));
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(10)))
        .await
        .map_err(|_| offline.clone())?
        .map_err(|_| offline)?
}

fn send(outbox: &State<'_, Outbox>, current: &State<'_, CurrentLanguage>, command: Command) -> Result<(), InvokeError> {
    post(outbox, current, Inbox::Send { command: Box::new(command), reply: None })
}

/// The login prompt's OK. With `save`, the login is kept for the site.
#[tauri::command]
#[specta::specta]
pub async fn answer_login(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    user: String,
    password: String,
    save: bool,
) -> Result<(), InvokeError> {
    if user.trim().is_empty() {
        return Err(errors::invalid(lang(&current), "Type the user name."));
    }
    let (tx, rx) = mpsc::channel();
    post(&outbox, &current, Inbox::Login { user, password, save, reply: tx })?;
    wait(&current, rx).await
}

/// The countdown's Do it now (`now`) or Cancel.
#[tauri::command]
#[specta::specta]
pub async fn answer_countdown(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    now: bool,
) -> Result<(), InvokeError> {
    send(&outbox, &current, if now { Command::CountdownNow } else { Command::CancelCountdown })
}

/// The list menu's Refresh download address.
#[tauri::command]
#[specta::specta]
pub async fn show_refresh(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::ShowRefresh(DownloadId::new(id.0))))
}

/// The New link window: use `url`, or with none the next download the
/// browser starts. `restart` starts over when the link is for another file.
#[tauri::command]
#[specta::specta]
pub async fn refresh_link(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
    url: Option<String>,
    restart: bool,
) -> Result<(), InvokeError> {
    let download = match url.as_deref().map(str::trim) {
        Some(u) => {
            let address = udm_domain::address::Address::parse(u).map_err(|e| errors::address(lang(&current), &e))?;
            Some(Download { url: address.into(), ..Download::default() })
        }
        None => None,
    };
    send(&outbox, &current, Command::RefreshLink { id: DownloadId::new(id.0), download, restart })
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_refresh(outbox: State<'_, Outbox>, current: State<'_, CurrentLanguage>) -> Result<(), InvokeError> {
    send(&outbox, &current, Command::CancelRefresh)
}

/// Unpack a package's archives, with a password for encrypted ones.
#[tauri::command]
#[specta::specta]
pub async fn unpack_package(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
    password: Option<String>,
) -> Result<(), InvokeError> {
    let password = password.and_then(Password::new);
    send(&outbox, &current, Command::UnpackPackage { id: PackageId::new(id.0), password })
}

/// Ask a package's password, from its menu.
#[tauri::command]
#[specta::specta]
pub async fn ask_package_password(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::AskPassword(PackageId::new(id.0))))
}

/// Stop keeping a package; its downloads stay in the list.
#[tauri::command]
#[specta::specta]
pub async fn ungroup_package(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
) -> Result<(), InvokeError> {
    send(&outbox, &current, Command::UngroupPackage { id: PackageId::new(id.0) })
}

#[tauri::command]
#[specta::specta]
pub async fn open_package(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    id: Id,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::OpenPackage(PackageId::new(id.0)))
}

/// The link, video and gallery pickers' Add.
#[tauri::command]
#[specta::specta]
pub async fn add_picks(
    window: tauri::WebviewWindow,
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
    input: PickInput,
) -> Result<(), InvokeError> {
    let label = label_of(&window, &current)?;
    let (tx, rx) = mpsc::channel();
    post(&outbox, &current, Inbox::AddPicks { label, input: Box::new(input), reply: tx })?;
    wait(&current, rx).await
}

/// About's Check for Update.
#[tauri::command]
#[specta::specta]
pub async fn check_update(outbox: State<'_, Outbox>, current: State<'_, CurrentLanguage>) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::UpdateChecking))?;
    send(&outbox, &current, Command::CheckUpdate)
}

/// Install now: download the newer release's setup and run it.
#[tauri::command]
#[specta::specta]
pub async fn install_update(outbox: State<'_, Outbox>, current: State<'_, CurrentLanguage>) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::Ui(UiMessage::Installing))?;
    send(&outbox, &current, Command::InstallUpdate)
}

/// Close the strip offering a newer release, until a newer one comes.
#[tauri::command]
#[specta::specta]
pub async fn dismiss_release(
    outbox: State<'_, Outbox>,
    current: State<'_, CurrentLanguage>,
) -> Result<(), InvokeError> {
    post(&outbox, &current, Inbox::DismissRelease)
}
