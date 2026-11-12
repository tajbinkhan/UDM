//! What the dialogs and download windows show.

use std::collections::BTreeSet;
use std::path::Path;

use udm_domain::download::{DownloadEntry, SegmentState, Status};
use udm_domain::id::DownloadId;
use udm_domain::queue::WhenDone;
use udm_domain::settings::{Settings, GENERAL};

use super::format;
use super::main::{display_name, is_startable};
use super::Ctx;
use crate::contract::{
    AboutView, AddUrlView, DoneView, DownloadView, DuplicateView, Id, LimitView, OnDoneView, QueueItem, RemoveView,
    SaveAsView, SegmentView, Then,
};
use crate::i18n::fill;
use crate::state::AppState;

fn settings(state: &AppState) -> Settings {
    state.settings.clone().unwrap_or_default()
}

pub fn add_url(state: &AppState) -> AddUrlView {
    AddUrlView { connected: state.connected() }
}

/// `name` in `folder`, joined with the separator `folder` already uses, so
/// a path keeps one style whatever system the shell runs on.
fn join_like(folder: &str, name: &str) -> String {
    let sep = if folder.contains('\\') {
        '\\'
    } else if folder.contains('/') {
        '/'
    } else {
        std::path::MAIN_SEPARATOR
    };
    format!("{}{sep}{name}", folder.trim_end_matches(['/', '\\']))
}

/// Download File Info for a download waiting for its answer, or `None` once
/// it no longer waits.
pub fn save_as(ctx: &Ctx, state: &AppState, id: DownloadId) -> Option<SaveAsView> {
    let e = state.table.get(id).filter(|e| e.status == Status::Confirming)?;
    let settings = settings(state);
    let name = display_name(e);
    let category = settings.listed_under(e.category.as_deref(), &name, e.url.as_str()).to_string();
    let mut categories = vec![GENERAL.to_string()];
    categories.extend(settings.categories.iter().filter(|c| c.name != GENERAL).map(|c| c.name.clone()));
    let save_as = e.path.clone().unwrap_or(name.clone());
    let folder = Path::new(&save_as).parent().map(|p| p.display().to_string()).unwrap_or_default();
    let remembered = settings.category(&category).and_then(|c| c.last_folder.clone()).unwrap_or_else(|| folder.clone());
    // The folders recent downloads went to, newest first, for the Save As
    // box's list.
    let mut seen = BTreeSet::new();
    let mut folders = Vec::new();
    let mut entries: Vec<&DownloadEntry> = state.table.entries().collect();
    entries.reverse();
    for p in entries.iter().filter_map(|e| e.path.as_deref()) {
        if let Some(parent) = Path::new(p).parent().map(|p| p.display().to_string()) {
            if seen.insert(parent.clone()) {
                folders.push(join_like(&parent, &name));
            }
        }
        if folders.len() >= 10 {
            break;
        }
    }
    Some(SaveAsView {
        id: Id(id.get()),
        url: e.url.to_string(),
        categories,
        remember_folder: settings.remembers_folder(&category),
        category,
        save_as,
        folders,
        remembered,
        size: e.total.map(|t| format::bytes(ctx.lang, t, 2)).unwrap_or_default(),
        icon: format::file_icon(&name),
        description: e.description.clone().unwrap_or_default(),
        duplicate: e.duplicate_of.map(|other| DuplicateView {
            other_running: state.table.get(other).is_some_and(|o| !o.status.is_terminal()),
        }),
        queues: state
            .queues
            .iter()
            .map(|q| QueueItem { id: Id(q.id.get()), name: ctx.lang.tr(&q.name).to_string(), running: q.running })
            .collect(),
    })
}

fn then_of(w: WhenDone) -> Then {
    match w {
        WhenDone::Nothing => Then::Nothing,
        WhenDone::Exit => Then::Exit,
        WhenDone::Sleep => Then::Sleep,
        WhenDone::Hibernate => Then::Hibernate,
        WhenDone::ShutDown => Then::ShutDown,
    }
}

pub fn when_done(t: Then) -> WhenDone {
    match t {
        Then::Nothing => WhenDone::Nothing,
        Then::Exit => WhenDone::Exit,
        Then::Sleep => WhenDone::Sleep,
        Then::Hibernate => WhenDone::Hibernate,
        Then::ShutDown => WhenDone::ShutDown,
    }
}

/// A download's progress window, while the download is in the list.
pub fn download(ctx: &Ctx, state: &AppState, id: DownloadId) -> Option<DownloadView> {
    let lang = ctx.lang;
    let e = state.table.get(id)?;
    let settings = settings(state);
    let speed = state.speed(id).filter(|_| e.status == Status::Downloading);
    let fraction = format::fraction(e).unwrap_or(0.0);
    let name = display_name(e);
    let percent = lang.number(f64::from(fraction) * 100.0, 2);
    let unknown = lang.tr("(unknown)").to_string();
    let downloaded = match format::fraction(e) {
        Some(_) => format!("{} ( {} % )", format::bytes(lang, e.done, 3), percent),
        None => format::bytes(lang, e.done, 3),
    };
    let total = e.total.map(|t| t.as_f64()).filter(|t| *t > 0.0);
    let segments = state
        .segments
        .get(&id)
        .map(|segs| {
            segs.iter()
                .map(|s| {
                    let share = |b: f64| total.map_or(0.0, |t| (b / t).clamp(0.0, 1.0) as f32);
                    SegmentView {
                        start: share(s.start.as_f64()),
                        reached: share(s.start.as_f64() + s.done.as_f64()),
                        downloaded: format::bytes(lang, s.done, 3),
                        info: lang
                            .tr(match s.state {
                                SegmentState::Waiting => "Waiting",
                                SegmentState::Connecting => "Connecting...",
                                SegmentState::Receiving => "Receiving data...",
                                _ => "Retrying...",
                            })
                            .to_string(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let on_done = e.on_done.unwrap_or_default();
    Some(DownloadView {
        id: Id(id.get()),
        title: format!("{}% {}", lang.number(f64::from(fraction) * 100.0, 0), name),
        address: e.url.to_string(),
        status: format::status_line(lang, e).to_string(),
        size: e.total.map(|t| format::bytes(lang, t, 3)).unwrap_or_else(|| unknown.clone()),
        downloaded,
        speed: speed.map(|s| format::rate(lang, s, 3)).unwrap_or_default(),
        time_left: format::time_left(e, speed).map(|s| format::duration(lang, s)).unwrap_or_default(),
        resume: lang
            .tr(match (e.last_try, e.resumable) {
                (None, _) => "(unknown)",
                (Some(_), true) => "Yes",
                (Some(_), false) => "No",
            })
            .to_string(),
        fraction,
        running: matches!(e.status, Status::Downloading | Status::Queued),
        can_resume: is_startable(e.status),
        save_to: e.path.clone().unwrap_or_default(),
        segments,
        limit: LimitView {
            on: e.speed_limit.is_some(),
            kb_per_sec: e.speed_limit.map_or(10, |b| (b / 1024).max(1) as u32),
            remember: e.limit_remember,
        },
        on_done: OnDoneView { dialog: on_done.dialog, then: then_of(on_done.then), force_close: on_done.force_close },
        speed_tab: settings.progress_speed_tab,
        done_tab: settings.progress_done_tab,
        details: settings.progress_start_details,
    })
}

/// Download complete, for a finished download.
pub fn done(ctx: &Ctx, state: &AppState, id: DownloadId) -> Option<DoneView> {
    let lang = ctx.lang;
    let e = state.table.get(id)?;
    let failed = e.status != Status::Done;
    let size = e.total.unwrap_or(e.done);
    Some(DoneView {
        id: Id(id.get()),
        failed,
        headline: lang.tr(if failed { "Download failed" } else { "Download complete" }).to_string(),
        size_line: fill(
            lang.tr("Downloaded {size} ({bytes} Bytes)"),
            &[("size", &format::bytes(lang, size, 2)), ("bytes", &size.get().to_string())],
        ),
        address: e.url.to_string(),
        saved_as: e.path.clone().unwrap_or_default(),
        can_open: !failed && e.path.is_some(),
    })
}

pub fn remove(state: &AppState) -> Option<RemoveView> {
    if state.to_remove.is_empty() {
        return None;
    }
    let has_files = state.to_remove.iter().filter_map(|id| state.table.get(*id)).any(|e| e.path.is_some());
    Some(RemoveView { count: state.to_remove.len() as u32, has_files })
}

pub fn about(ctx: &Ctx, state: &AppState) -> AboutView {
    let lang = ctx.lang;
    let a = &state.asks;
    let update = match (&a.installing, &a.update, a.checking) {
        (Some(None), _, _) => lang.tr("Downloading the update...").to_string(),
        (Some(Some(why)), _, _) => fill(lang.tr("The update did not install: {error}"), &[("error", why)]),
        (_, _, true) => lang.tr("Checking for updates...").to_string(),
        (_, Some(Ok(Some(r))), _) => fill(lang.tr("UDM {version} is available."), &[("version", &r.version)]),
        (_, Some(Ok(None)), _) => lang.tr("This is the latest version.").to_string(),
        (_, Some(Err(why)), _) => fill(lang.tr("Could not check for updates: {error}"), &[("error", why)]),
        (None, None, false) => String::new(),
    };
    AboutView {
        version: env!("CARGO_PKG_VERSION").to_string(),
        daemon: state.daemon_version.clone().unwrap_or_default(),
        update,
        can_install: matches!(&a.update, Some(Ok(Some(r))) if r.setup.is_some()) && a.installing.is_none(),
        checking: a.checking,
        connected: state.connected(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::resolve;
    use crate::state::{apply, UiMessage};
    use std::time::Instant;
    use udm_application::daemon::Event;

    fn ctx() -> Ctx {
        Ctx {
            lang: resolve("en", &[]),
            offset: 0,
            today: "2026-11-12".into(),
            now: std::time::Instant::now(),
            unix: 1_700_000_000,
            downloads: std::path::PathBuf::from("/home/u/Downloads"),
        }
    }

    fn with(entry: DownloadEntry) -> AppState {
        let mut s = AppState::default();
        let event = Event::Downloads { items: vec![entry] };
        apply(&mut s, UiMessage::Daemon { event: Box::new(event), at: Instant::now(), unix: 0 });
        s
    }

    fn entry(status: Status) -> DownloadEntry {
        crate::testing::entry(4, "big.bin", status, 41_943_040, 13_183_549)
    }

    #[test]
    fn download_file_info_opens_only_while_the_download_waits_for_it() {
        let s = with(entry(Status::Confirming));
        let v = save_as(&ctx(), &s, DownloadId::new(4)).unwrap();
        assert_eq!(v.save_as, "/home/u/Downloads/big.bin");
        assert_eq!(v.url, "http://127.0.0.1:8765/big.bin");
        assert_eq!(v.size, "40.00 MB");
        assert_eq!(v.categories[0], GENERAL);
        let s = with(entry(Status::Downloading));
        assert!(save_as(&ctx(), &s, DownloadId::new(4)).is_none());
    }

    #[test]
    fn folders_keep_their_own_separator() {
        assert_eq!(join_like("/home/u/Downloads", "a.zip"), "/home/u/Downloads/a.zip");
        assert_eq!(join_like(r"C:\Users\u\Downloads\", "a.zip"), r"C:\Users\u\Downloads\a.zip");
    }

    #[test]
    fn the_progress_window_reads_like_the_list() {
        let s = with(entry(Status::Downloading));
        let v = download(&ctx(), &s, DownloadId::new(4)).unwrap();
        assert_eq!(v.size, "40.000 MB");
        assert_eq!(v.downloaded, "12.573 MB ( 31.43 % )");
        assert_eq!(v.title, "31% big.bin");
        assert!(v.running && !v.can_resume);
    }

    #[test]
    fn the_completion_dialog_names_the_size_in_bytes() {
        let s = with(entry(Status::Done));
        let v = done(&ctx(), &s, DownloadId::new(4)).unwrap();
        assert_eq!(v.size_line, "Downloaded 40.00 MB (41943040 Bytes)");
        assert!(v.can_open);
    }
}
