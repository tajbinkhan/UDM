//! Errors as the user reads them (AGENTS.md section 9): commands fail with
//! an [`InvokeError`] whose message the shell has already translated.
//!
//! The egui window's `errors.rs` translates every domain error kind; it
//! moves to a crate both windows share when more windows come over (see
//! AGENTS.md section 16). Until then the daemon's failures read in the
//! English the daemon wrote them in.

use udm_application::daemon::Failure;
use udm_domain::address::AddressError;

use crate::contract::{ErrorKind, InvokeError};
use crate::i18n::{fill, Language};

/// The daemon's failure as text for the banner.
pub fn failure(e: &Failure) -> String {
    e.to_string()
}

fn error(kind: ErrorKind, message: impl Into<String>) -> InvokeError {
    InvokeError { kind, message: message.into() }
}

pub fn offline(lang: &'static Language) -> InvokeError {
    error(ErrorKind::Offline, lang.tr("UDM's background service is not connected. Try again in a moment."))
}

pub fn gone(lang: &'static Language) -> InvokeError {
    error(ErrorKind::Gone, lang.tr("That download is no longer in the list."))
}

pub fn address(lang: &'static Language, e: &AddressError) -> InvokeError {
    let text = match e {
        AddressError::Bad(address, _) | AddressError::Scheme(address) | AddressError::NotDownload(address) => {
            fill(lang.tr("{address} is not an http, https or ftp address."), &[("address", address)])
        }
    };
    error(ErrorKind::Invalid, text)
}

/// Why Options could not save. The settings' own checks read in English
/// until the egui window's translations of them are shared.
pub fn options(lang: &'static Language, e: &crate::options::OptionsError) -> InvokeError {
    use crate::options::OptionsError as E;
    let message = match e {
        E::Invalid(invalid) => invalid.to_string(),
        E::CategoryName => lang.tr("Type a name for each category.").to_string(),
        E::CategoryTwice(name) => fill(lang.tr("There are two categories named \"{name}\"."), &[("name", name)]),
        E::Theme(name) => fill(lang.tr("\"{name}\" is not a theme."), &[("name", name)]),
    };
    error(ErrorKind::Invalid, message)
}

/// Why a queue cannot be saved. The domain's text, in English until the
/// egui window's translations of it are shared.
pub fn queue(e: &udm_domain::queue::InvalidQueue) -> InvokeError {
    error(ErrorKind::Invalid, e.to_string())
}

/// Why settings cannot be saved, likewise.
pub fn settings(e: &udm_domain::settings::InvalidSettings) -> InvokeError {
    error(ErrorKind::Invalid, e.to_string())
}

/// Why the crawl cannot start, in the domain's English for now.
pub fn grab(e: &udm_domain::grab::InvalidGrab) -> InvokeError {
    error(ErrorKind::Invalid, e.to_string())
}

/// A file the shell could not read or write, such as a project.
pub fn files(e: &udm_domain::ports::FilesError) -> InvokeError {
    error(ErrorKind::Gone, e.to_string())
}

pub fn no_projects(lang: &'static Language) -> InvokeError {
    error(ErrorKind::Gone, lang.tr("There is no projects folder."))
}

pub fn category(lang: &'static Language, e: &crate::tools::CategoryError) -> InvokeError {
    use crate::tools::CategoryError as E;
    let message = match e {
        E::NoName => lang.tr("Type a name for the category.").to_string(),
        E::Taken(name) => fill(lang.tr("There is a category named \"{name}\" already."), &[("name", name)]),
    };
    error(ErrorKind::Invalid, message)
}

pub fn batch(e: &udm_domain::batch::BatchError) -> InvokeError {
    error(ErrorKind::Invalid, e.to_string())
}

pub fn list(e: &udm_application::window::ListError) -> InvokeError {
    error(ErrorKind::Gone, e.0.clone())
}

pub fn invalid(lang: &'static Language, english: &'static str) -> InvokeError {
    error(ErrorKind::Invalid, lang.tr(english))
}

/// The shell itself failed (a window did not open).
#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("the window did not start: {0}")]
    Tauri(#[from] tauri::Error),
    #[error("the TypeScript bindings were not written: {0}")]
    Bindings(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::resolve;

    #[test]
    fn a_bad_address_names_itself() {
        let e = address(resolve("en", &[]), &AddressError::NotDownload("mailto:x".into()));
        assert_eq!(e.kind, ErrorKind::Invalid);
        assert_eq!(e.message, "mailto:x is not an http, https or ftp address.");
    }
}
