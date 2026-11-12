//! Snapshots of every window, made by the real projections from a sample
//! list, for the frontend's tests and its preview page
//! (`frontend/test/fixtures/`). `UDM_WRITE_FIXTURES=1 cargo test -p
//! udm-ui-tauri fixtures` writes them; otherwise the test checks they are
//! current.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::json;
use udm_application::daemon::Event;
use udm_domain::bytes::ByteCount;
use udm_domain::download::{SegmentInfo, SegmentState, Status};
use udm_domain::id::{DownloadId, QueueId};
use udm_domain::queue::Queue;
use udm_domain::settings::{Notify, Settings, Theme};

use crate::i18n::resolve;
use crate::projection::{self, Ctx};
use crate::state::{apply, AppState, UiMessage};
use crate::testing::entry;
use crate::windows::Label;

const DAY: u64 = 86_400;
/// 2026-10-21, the first download's day.
const START: u64 = 1_792_540_800;

fn sample() -> AppState {
    let mut s = AppState::default();
    let at = Instant::now();
    let send = |s: &mut AppState, event: Event, at: Instant| {
        apply(s, UiMessage::Daemon { event: Box::new(event), at, unix: 0 })
    };
    let files: [(&str, u64, Status); 17] = [
        ("interview-notes.pdf", 70_686, Status::Done),
        ("HandBrake-1.11.2-x86_64-Win_GUI.exe", 25_323_520, Status::Done),
        ("Screen-Capture (3).mp3", 88_749_000, Status::Done),
        ("udm-windows.zip", 4_445_962, Status::Done),
        ("sf-pro-display.zip", 904_396, Status::Done),
        ("Open.Movie.2026.1080p.mkv", 8_740_000_000, Status::Done),
        ("UDM-Setup-0.1.0.exe", 5_725_224, Status::Done),
        ("UltraViewer_setup_6.6.exe", 3_638_558, Status::Done),
        ("launcher-0.6.0.exe", 44_774_195, Status::Done),
        ("project-files-v2.zip", 353_802_000, Status::Done),
        ("DTLite1260-2472i.exe", 46_153_400, Status::Done),
        ("office-suite-2016-x64.iso", 4_080_000_000, Status::Downloading),
        ("office-suite-2016-x86.iso", 4_080_000_000, Status::Done),
        ("kubuntu-26.04.1-desktop-amd64.iso", 5_068_000_000, Status::Done),
        ("rufus-4.15.exe", 1_981_808, Status::Done),
        ("UDM-Setup-0.3.3.exe", 7_927_000, Status::Done),
        ("udm-idm-test-40MB.bin", 41_943_040, Status::Confirming),
    ];
    let items = files
        .iter()
        .enumerate()
        .map(|(i, (name, total, status))| {
            let done = if *status == Status::Downloading { total * 1628 / 10_000 } else { *total };
            let mut e = entry(i as u64 + 1, name, *status, *total, done);
            e.last_try = Some(START + DAY * (i as u64 / 2) + 3600 * 14 + 60 * i as u64);
            if i == 2 {
                e.description = Some("MP3: 64kbs, 44100Hz, Length: 3:05:42".into());
            }
            e
        })
        .collect();
    send(&mut s, Event::Downloads { items }, at);
    send(&mut s, Event::Connected, at);
    // No tip at startup, so the window's tests see the window itself.
    let settings =
        Settings { notify: Notify::Dialog, theme: Theme::Dark, tips_on_startup: false, ..Settings::default() };
    send(&mut s, Event::Settings { settings }, at);
    let queue: Queue = serde_json::from_value(json!({
        "id": 1, "name": "Main queue", "max_concurrent": 1, "running": true, "order": [12]
    }))
    .unwrap();
    let sync: Queue =
        serde_json::from_value(json!({"id": 2, "name": "Synchronization queue", "max_concurrent": 1})).unwrap();
    send(&mut s, Event::Queues { items: vec![queue, sync] }, at);
    // A running download, 16.28% done at about 15 MB/s, on eight
    // connections.
    let id = DownloadId::new(12);
    let total = 4_080_000_000u64;
    for (k, done) in [(0u64, total * 1600 / 10_000), (1, total * 1628 / 10_000)] {
        let part = total / 8;
        let segments = (0..8)
            .map(|n| SegmentInfo {
                start: ByteCount::new(n * part),
                end: ByteCount::new((n + 1) * part - 1),
                done: ByteCount::new(part * (130 + n * 7) / 1000),
                state: SegmentState::Receiving,
            })
            .collect();
        let event = Event::Progress {
            id,
            url: "http://127.0.0.1:8765/office-suite-2016-x64.iso".into(),
            done: ByteCount::new(done),
            total: Some(ByteCount::new(total)),
            parts: None,
            segments: Some(segments),
        };
        send(&mut s, event, at + Duration::from_secs(k * 4));
    }
    apply(&mut s, UiMessage::Select(vec![DownloadId::new(2)]));
    apply(&mut s, UiMessage::Sort(crate::contract::ColumnKey::LastTry));
    apply(&mut s, UiMessage::ShowProgress(id));
    s.done.insert(DownloadId::new(17));
    let _ = QueueId::MAIN;
    s
}

fn fixtures() -> serde_json::Value {
    let mut s = sample();
    let ctx = Ctx {
        lang: resolve("en", &[]),
        offset: 0,
        today: "2026-11-12".into(),
        now: std::time::Instant::now(),
        downloads: std::path::PathBuf::from("/home/u/Downloads"),
    };
    let dark = projection::look(&s);
    if let Some(settings) = s.settings.as_mut() {
        settings.theme = Theme::Light;
    }
    let light = projection::look(&s);
    apply(&mut s, UiMessage::Select(vec![DownloadId::new(2), DownloadId::new(4)]));
    apply(&mut s, UiMessage::AskRemove);
    let view = |label: Label| serde_json::to_value(projection::view(&ctx, &s, &label)).unwrap();
    // The Download complete dialog is drawn from a finished download.
    let mut finished = s.table.get(DownloadId::new(17)).cloned().unwrap();
    finished.status = Status::Done;
    finished.done = finished.total.unwrap_or_default();
    let mut done_state = sample();
    apply(
        &mut done_state,
        UiMessage::Daemon { event: Box::new(Event::Updated { item: finished }), at: Instant::now(), unix: 0 },
    );
    let done = serde_json::to_value(projection::view(&ctx, &done_state, &Label::Done(DownloadId::new(17)))).unwrap();
    // The grabber after a crawl of a small site.
    let mut grab_state = sample();
    let page = "http://127.0.0.1:8765/index.html";
    let file = |name: &str, size: u64, text: &str| udm_domain::grab::GrabFile {
        url: format!("http://127.0.0.1:8765/files/{name}"),
        name: name.into(),
        page: page.into(),
        size: Some(ByteCount::new(size)),
        text: Some(text.into()),
    };
    grab_state.grab.library = Some(crate::grabber::Library { recent: vec!["docs-site".into()], ..Default::default() });
    grab_state.grab.started(udm_domain::grab::GrabRequest { start: page.into(), ..Default::default() });
    grab_state.grab.done(
        vec![
            file("manual.pdf", 2_400_000, "User manual"),
            file("setup.exe", 41_943_040, "Download"),
            file("notes.txt", 3_100, ""),
        ],
        4,
        9,
        None,
        None,
    );
    let grab_results = serde_json::to_value(projection::view(&ctx, &grab_state, &Label::Grabber)).unwrap();
    // Links from the clipboard, to tick.
    let mut links_state = sample();
    let text = "http://127.0.0.1:8765/files/report.pdf http://127.0.0.1:8765/files/photos.zip http://127.0.0.1:8765/files/talk.mp3";
    apply(
        &mut links_state,
        UiMessage::Links(Some(Box::new(crate::tools::LinkList {
            file: None,
            items: crate::tools::links_of(text),
            rejected: 0,
        }))),
    );
    let links = serde_json::to_value(projection::view(&ctx, &links_state, &Label::Links)).unwrap();
    // What the daemon asks of the window: a login, a countdown, a new
    // link, a package's password, and links, videos and a gallery to pick.
    let mut asks = sample();
    let failed = |id: u64| {
        let mut e = asks.table.get(DownloadId::new(id)).cloned().unwrap();
        e.status = Status::Failed;
        e.error = Some(udm_domain::download::needs_login_error("127.0.0.1", Some("Private files")));
        e
    };
    let (f3, f4) = (failed(3), failed(4));
    let at = Instant::now();
    let send = |s: &mut AppState, event: Event| apply(s, UiMessage::Daemon { event: Box::new(event), at, unix: 0 });
    send(&mut asks, Event::Updated { item: f3.clone() });
    send(&mut asks, Event::Updated { item: f4.clone() });
    send(&mut asks, Event::Failed { id: f3.id, url: f3.url.clone(), error: f3.error.clone().unwrap() });
    send(&mut asks, Event::Failed { id: f4.id, url: f4.url.clone(), error: f4.error.clone().unwrap() });
    send(
        &mut asks,
        Event::Countdown {
            action: udm_domain::queue::WhenDone::ShutDown,
            seconds: 60,
            queue: "Main queue".into(),
            file: None,
        },
    );
    apply(&mut asks, UiMessage::ShowRefresh(f3.id));
    let link = |url: &str, text: &str| udm_domain::download::PageLink {
        url: url.into(),
        text: text.into(),
        image: false,
        cookies: None,
    };
    send(
        &mut asks,
        Event::PickLinks {
            page: "http://127.0.0.1:8765/index.html".into(),
            links: vec![
                link("http://127.0.0.1:8765/files/manual.pdf", "User manual"),
                link("http://127.0.0.1:8765/files/setup.exe", "Download"),
                link("http://127.0.0.1:8765/files/photos.zip", "Photos"),
            ],
            user_agent: None,
        },
    );
    let request = asks.asks.links.as_ref().map_or(0, |l| l.request);
    send(
        &mut asks,
        Event::LinkInfo { request, index: 1, total: Some(ByteCount::new(41_943_040)), content_type: None, error: None },
    );
    let entry = |n: u32| udm_domain::media::PlaylistEntry {
        url: format!("https://videos.example.com/watch?v={n}"),
        title: format!("Lesson {n}: getting started"),
        duration: Some(f64::from(300 + n * 61)),
    };
    let playlist = udm_domain::media::Playlist { title: "A course".into(), entries: (1..=4).map(entry).collect() };
    send(&mut asks, Event::PickVideos { page: "https://videos.example.com/list".into(), playlist, user_agent: None });
    send(&mut asks, Event::PickGallery { page: "https://pics.example.com/album".into(), user_agent: None });
    let request = asks.asks.gallery.as_ref().map_or(0, |g| g.request);
    let picture = |n: u32| udm_domain::gallery::Picture {
        url: format!("https://pics.example.com/{n}.jpg"),
        name: format!("{n}.jpg"),
        extension: "jpg".into(),
        width: Some(1920),
        height: Some(1080),
    };
    let gallery = udm_domain::gallery::Gallery { title: "Album".into(), pictures: (1..=3).map(picture).collect() };
    send(
        &mut asks,
        Event::Gallery { request, page: "https://pics.example.com/album".into(), gallery: Some(gallery), error: None },
    );
    let package: udm_domain::package::Package = serde_json::from_value(json!({
        "id": 1, "name": "photos", "downloads": [5], "unpack": {"state": "needs_password", "wrong": false}
    }))
    .unwrap();
    send(&mut asks, Event::Packages { items: vec![package] });
    apply(&mut asks, UiMessage::AskPassword(udm_domain::id::PackageId::new(1)));
    let asked = |label: Label| serde_json::to_value(projection::view(&ctx, &asks, &label)).unwrap();
    let countdown = asked(Label::Countdown);
    let (login, refresh, password) = (asked(Label::Login), asked(Label::Refresh), asked(Label::Password));
    let (pick_links, pick_videos, gallery) = (asked(Label::PickLinks), asked(Label::PickVideos), asked(Label::Gallery));
    json!({
        "look": { "light": light, "dark": dark },
        "views": {
            "main": view(Label::Main),
            "add-url": view(Label::AddUrl),
            "save-as": view(Label::SaveAs(DownloadId::new(17))),
            "download": view(Label::Download(DownloadId::new(12))),
            "done": done,
            "remove": view(Label::Remove),
            "about": view(Label::About),
            "options": view(Label::Options),
            "scheduler": view(Label::Scheduler),
            "grabber": view(Label::Grabber),
            "grabber-results": grab_results,
            "batch": view(Label::Batch),
            "links": links,
            "properties": view(Label::Properties(DownloadId::new(2))),
            "drop-target": view(Label::DropTarget),
            "login": login,
            "countdown": countdown,
            "refresh": refresh,
            "password": password,
            "pick-links": pick_links,
            "pick-videos": pick_videos,
            "gallery": gallery,
        }
    })
}

#[test]
fn fixtures_are_current() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../frontend/test/fixtures/views.json");
    let fresh = serde_json::to_string_pretty(&fixtures()).unwrap() + "\n";
    if std::env::var_os("UDM_WRITE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, fresh).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(committed == fresh, "frontend/test/fixtures/views.json is out of date: run UDM_WRITE_FIXTURES=1 cargo test -p udm-ui-tauri fixtures");
}
