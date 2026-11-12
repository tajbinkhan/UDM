//! What the window does with a download's file beyond opening it: the zip
//! preview, and the offer to keep a download in the browser after its
//! save-as dialog is canceled twice. Plain functions, tested without Tauri.

use std::collections::HashSet;

use udm_application::daemon::{Event, Failure};
use udm_domain::bytes::ByteCount;
use udm_domain::download::{ZipEntry, MAX_ZIP_ENTRIES};
use udm_domain::id::DownloadId;
use udm_domain::settings::Settings;

use crate::contract::{ExceptionChoice, ExceptionView, ZipRow, ZipView};
use crate::i18n::{fill, Language};
use crate::projection::format;
use crate::state::AppState;

/// A zip download's preview: asked for, then its files or why not.
#[derive(Debug, Clone, PartialEq)]
pub struct ZipPreview {
    pub id: DownloadId,
    pub result: Option<Result<Vec<ZipEntry>, Failure>>,
}

/// What the window keeps of the file tools.
#[derive(Debug, Default)]
pub struct FileWork {
    pub zip: Option<ZipPreview>,
    /// The captured addresses whose save-as dialog was canceled once.
    pub canceled_once: HashSet<String>,
    /// The address to offer keeping in the browser.
    pub exception: Option<String>,
}

/// Take the daemon's zip listing.
pub fn take(state: &mut AppState, event: &Event) {
    if let Event::ZipPreview { id, entries, error } = event {
        if let Some(p) = state.files.zip.as_mut().filter(|p| p.id == *id) {
            p.result = Some(match error {
                Some(e) => Err(e.clone()),
                None => Ok(entries.clone()),
            });
        }
    }
}

/// A captured download's save-as dialog was canceled: the second time for
/// one address, offer to keep it in the browser, when Options allows.
pub fn canceled(state: &mut AppState, id: DownloadId) {
    let offer = state.settings.as_ref().is_some_and(|s| s.offer_exceptions);
    let Some(e) = state.table.get(id).filter(|e| e.from_browser && offer) else { return };
    let url = e.url.to_string();
    if !state.files.canceled_once.insert(url.clone()) {
        state.files.canceled_once.remove(&url);
        state.files.exception = Some(url);
    }
}

fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if host.starts_with('[') {
        host.split_once(']').map_or(host, |(h, _)| h).trim_start_matches('[')
    } else {
        host.split(':').next().unwrap_or_default()
    };
    host.to_ascii_lowercase()
}

/// The settings with the user's answer to the offer, or `None` when they
/// stay as they are.
pub fn exception_settings(
    current: &Settings,
    url: &str,
    choice: ExceptionChoice,
    stop_offering: bool,
) -> Option<Settings> {
    let mut s = current.clone();
    match choice {
        ExceptionChoice::Site => {
            let host = host_of(url);
            if !host.is_empty() && !s.capture_excluded_sites.contains(&host) {
                s.capture_excluded_sites.push(host);
            }
        }
        ExceptionChoice::Address if !s.capture_excluded_urls.iter().any(|u| u == url) => {
            s.capture_excluded_urls.push(url.to_string());
        }
        _ => {}
    }
    if stop_offering {
        s.offer_exceptions = false;
    }
    (s != *current).then_some(s)
}

pub fn exception_view(state: &AppState) -> Option<ExceptionView> {
    let url = state.files.exception.clone()?;
    Some(ExceptionView { host: host_of(&url), url, connected: state.connected() })
}

pub fn zip_view(lang: &'static Language, state: &AppState) -> Option<ZipView> {
    let p = state.files.zip.as_ref()?;
    let name = state.table.get(p.id).map(|e| e.display_name()).unwrap_or_default();
    let mut view = ZipView {
        title: fill(lang.tr("Inside {name}"), &[("name", &name)]),
        summary: String::new(),
        note: String::new(),
        rows: Vec::new(),
        error: None,
        reading: p.result.is_none(),
    };
    match &p.result {
        None => {}
        Some(Err(e)) => view.error = Some(crate::errors::failure(lang, e)),
        Some(Ok(entries)) => {
            let total = entries.iter().fold(ByteCount::ZERO, |sum, e| sum + e.size);
            view.summary = fill(
                lang.tr("{count} files, {size} unpacked"),
                &[("count", &entries.len().to_string()), ("size", &format::bytes(lang, total, 1))],
            );
            if entries.len() == MAX_ZIP_ENTRIES {
                view.note = fill(lang.tr("Showing the first {count}."), &[("count", &MAX_ZIP_ENTRIES.to_string())]);
            }
            view.rows =
                entries.iter().map(|e| ZipRow { name: e.name.clone(), size: format::bytes(lang, e.size, 1) }).collect();
        }
    }
    Some(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply, UiMessage};
    use std::time::Instant;

    #[test]
    fn hosts_come_from_addresses() {
        assert_eq!(host_of("https://User:pw@Files.Example.com:8443/a.zip?x"), "files.example.com");
        assert_eq!(host_of("http://[::1]:80/a"), "::1");
    }

    #[test]
    fn the_answer_changes_the_capture_rules() {
        let s = Settings::default();
        let url = "https://cdn.example.com/a.mp3";
        let out = exception_settings(&s, url, ExceptionChoice::Site, false).unwrap();
        assert!(out.capture_excluded_sites.contains(&"cdn.example.com".to_string()));
        let out = exception_settings(&s, url, ExceptionChoice::Address, true).unwrap();
        assert!(out.capture_excluded_urls.contains(&url.to_string()) && !out.offer_exceptions);
        assert_eq!(exception_settings(&s, url, ExceptionChoice::Keep, false), None);
    }

    #[test]
    fn the_zip_listing_reaches_the_preview() {
        let mut s = AppState::default();
        let id = DownloadId::new(4);
        s.files.zip = Some(ZipPreview { id, result: None });
        let entries = vec![ZipEntry { name: "a.txt".into(), size: ByteCount::new(2048) }];
        let event = Event::ZipPreview { id, entries, error: None };
        apply(&mut s, UiMessage::Daemon { event: Box::new(event), at: Instant::now(), unix: 0 });
        let v = zip_view(crate::i18n::resolve("en", &[]), &s).unwrap();
        assert!(!v.reading);
        assert_eq!(v.rows[0].name, "a.txt");
        assert_eq!(v.summary, "1 files, 2.0 KB unpacked");
    }
}
