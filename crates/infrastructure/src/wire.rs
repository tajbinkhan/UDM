//! Mapping between the wire messages (`protocol::Incoming` and `Outgoing`)
//! and the application's `Command` and `Event`. Both sides of a
//! connection use it: the window's `daemon_client` sends commands and reads
//! events, and the daemon's `server` reads commands and sends events.

use pullbay_application::daemon::service::ServiceError;
use pullbay_application::daemon::{Command, Event, Failure};
use pullbay_application::downloads::{ConfirmError, StartOrWaitError};
use pullbay_domain::download::{DownloadError, Mismatch};
use pullbay_domain::grab::InvalidGrab;
use pullbay_domain::pairing::PairingError;
use pullbay_domain::ports::{ControlError, SettingsError};
use pullbay_domain::queue::QueueError;
use pullbay_domain::update::UpdateError;
use serde::{Deserialize, Serialize};

use crate::protocol::{Incoming, Outgoing};

/// The wire message for `command`.
pub fn incoming(command: Command) -> Incoming {
    match command {
        Command::Hello { version, client } => Incoming::Hello { version, client },
        Command::Ping => Incoming::Ping,
        Command::CommandLine(c) => Incoming::CommandLine(c),
        Command::ProbeMedia { request, download } => Incoming::ProbeMedia { request, download },
        Command::PickLinks { page, links, user_agent } => Incoming::PickLinks { page, links, user_agent },
        Command::Rendered { request, html, url, popups } => Incoming::Rendered { request, html, url, popups },
        Command::Cookies { request, cookies } => Incoming::Cookies { request, cookies },
        Command::Add(d) => Incoming::Add(d),
        Command::AddMany { items, queue } => Incoming::AddMany { items, queue },
        Command::Grab(r) => Incoming::Grab(r),
        Command::CancelGrab => Incoming::CancelGrab,
        Command::ProbeLinks { request, links } => Incoming::ProbeLinks { request, links },
        Command::StopProbeLinks => Incoming::StopProbeLinks,
        Command::Cancel { id } => Incoming::Cancel { id },
        Command::Retry { id } => Incoming::Retry { id },
        Command::Redownload { id } => Incoming::Redownload { id },
        Command::Remove { id, delete_files } => Incoming::Remove { id, delete_files },
        Command::List => Incoming::List,
        Command::PauseAll => Incoming::PauseAll,
        Command::ResumeAll => Incoming::ResumeAll,
        Command::Confirm(a) => Incoming::Confirm(a),
        Command::SaveQueue { queue } => Incoming::SaveQueue { queue },
        Command::DeleteQueue { id } => Incoming::DeleteQueue { id },
        Command::StartQueue { id } => Incoming::StartQueue { id },
        Command::StopQueue { id } => Incoming::StopQueue { id },
        Command::SetQueueOnce { id, when_done } => Incoming::SetQueueOnce { id, when_done },
        Command::MoveToQueue { id, queue, position } => Incoming::MoveToQueue { id, queue, position },
        Command::RemoveFromQueue { id } => Incoming::RemoveFromQueue { id },
        Command::CancelCountdown => Incoming::CancelCountdown,
        Command::RefreshLink { id, download, restart } => Incoming::RefreshLink { id, download, restart },
        Command::CancelRefresh => Incoming::CancelRefresh,
        Command::CheckUpdate => Incoming::CheckUpdate,
        Command::InstallUpdate => Incoming::InstallUpdate,
        Command::Convert { id, to, delete_original } => Incoming::Convert { id, to, delete_original },
        Command::PreviewZip { id } => Incoming::PreviewZip { id },
        Command::SetProperties { id, changes } => Incoming::SetProperties { id, changes },
        Command::SetSpeedLimit { id, bytes_per_sec, remember } => {
            Incoming::SetSpeedLimit { id, bytes_per_sec, remember }
        }
        Command::CountdownNow => Incoming::CountdownNow,
        Command::SetOnDone { id, on_done } => Incoming::SetOnDone { id, on_done },
        Command::GetSettings => Incoming::GetSettings,
        Command::SetSettings { settings } => Incoming::SetSettings { settings },
        Command::RemoteStatus => Incoming::RemoteStatus,
        Command::StartPairing => Incoming::StartPairing,
        Command::CancelPairing => Incoming::CancelPairing,
        Command::RemoveDevice { id } => Incoming::RemoveDevice { id },
    }
}

/// The command wire message `msg` carries.
pub fn command(msg: Incoming) -> Command {
    match msg {
        Incoming::Hello { version, client } => Command::Hello { version, client },
        Incoming::Ping => Command::Ping,
        Incoming::CommandLine(c) => Command::CommandLine(c),
        Incoming::ProbeMedia { request, download } => Command::ProbeMedia { request, download },
        Incoming::PickLinks { page, links, user_agent } => Command::PickLinks { page, links, user_agent },
        Incoming::Rendered { request, html, url, popups } => Command::Rendered { request, html, url, popups },
        Incoming::Cookies { request, cookies } => Command::Cookies { request, cookies },
        Incoming::Add(d) => Command::Add(d),
        Incoming::AddMany { items, queue } => Command::AddMany { items, queue },
        Incoming::Grab(r) => Command::Grab(r),
        Incoming::CancelGrab => Command::CancelGrab,
        Incoming::ProbeLinks { request, links } => Command::ProbeLinks { request, links },
        Incoming::StopProbeLinks => Command::StopProbeLinks,
        Incoming::Cancel { id } => Command::Cancel { id },
        Incoming::Retry { id } => Command::Retry { id },
        Incoming::Redownload { id } => Command::Redownload { id },
        Incoming::Remove { id, delete_files } => Command::Remove { id, delete_files },
        Incoming::List => Command::List,
        Incoming::PauseAll => Command::PauseAll,
        Incoming::ResumeAll => Command::ResumeAll,
        Incoming::Confirm(a) => Command::Confirm(a),
        Incoming::SaveQueue { queue } => Command::SaveQueue { queue },
        Incoming::DeleteQueue { id } => Command::DeleteQueue { id },
        Incoming::StartQueue { id } => Command::StartQueue { id },
        Incoming::StopQueue { id } => Command::StopQueue { id },
        Incoming::SetQueueOnce { id, when_done } => Command::SetQueueOnce { id, when_done },
        Incoming::MoveToQueue { id, queue, position } => Command::MoveToQueue { id, queue, position },
        Incoming::RemoveFromQueue { id } => Command::RemoveFromQueue { id },
        Incoming::CancelCountdown => Command::CancelCountdown,
        Incoming::RefreshLink { id, download, restart } => Command::RefreshLink { id, download, restart },
        Incoming::CancelRefresh => Command::CancelRefresh,
        Incoming::CheckUpdate => Command::CheckUpdate,
        Incoming::InstallUpdate => Command::InstallUpdate,
        Incoming::Convert { id, to, delete_original } => Command::Convert { id, to, delete_original },
        Incoming::PreviewZip { id } => Command::PreviewZip { id },
        Incoming::SetProperties { id, changes } => Command::SetProperties { id, changes },
        Incoming::SetSpeedLimit { id, bytes_per_sec, remember } => {
            Command::SetSpeedLimit { id, bytes_per_sec, remember }
        }
        Incoming::CountdownNow => Command::CountdownNow,
        Incoming::SetOnDone { id, on_done } => Command::SetOnDone { id, on_done },
        Incoming::GetSettings => Command::GetSettings,
        Incoming::SetSettings { settings } => Command::SetSettings { settings },
        Incoming::RemoteStatus => Command::RemoteStatus,
        Incoming::StartPairing => Command::StartPairing,
        Incoming::CancelPairing => Command::CancelPairing,
        Incoming::RemoveDevice { id } => Command::RemoveDevice { id },
    }
}

/// The event wire message `msg` carries.
pub fn event(msg: Outgoing) -> Event {
    match msg {
        Outgoing::Hello { name, version, api } => Event::Hello { name, version, api },
        Outgoing::Pong => Event::Pong,
        Outgoing::Ack { id, url, status } => Event::Ack { id, url, status },
        Outgoing::MediaInfo { request, info } => Event::MediaInfo { request, info },
        Outgoing::CookiesWanted { request, url } => Event::CookiesWanted { request, url },
        Outgoing::RenderPage { request, url } => Event::RenderPage { request, url },
        Outgoing::CommandLine { window_open } => Event::CommandLine { window_open },
        Outgoing::Added { id, url } => Event::Added { id, url },
        Outgoing::Started { id, url, path, total, resumable, connections, partial, validator } => {
            Event::Started { id, url, path, total, resumable, connections, partial, validator }
        }
        Outgoing::Progress { id, url, done, total, parts, segments } => {
            Event::Progress { id, url, done, total, parts, segments }
        }
        Outgoing::Done { id, url, path, note } => Event::Done { id, url, path, note },
        Outgoing::Failed { id, url, error } => Event::Failed { id, url, error },
        Outgoing::Canceled { id, url } => Event::Canceled { id, url },
        Outgoing::Removed { id } => Event::Removed { id },
        Outgoing::Updated { item } => Event::Updated { item },
        Outgoing::Downloads { items } => Event::Downloads { items },
        Outgoing::Settings { settings } => Event::Settings { settings },
        Outgoing::Show => Event::Show,
        Outgoing::ShowDone { id } => Event::ShowDone { id },
        Outgoing::Queues { items } => Event::Queues { items },
        Outgoing::Countdown { action, seconds, queue, file } => Event::Countdown { action, seconds, queue, file },
        Outgoing::CountdownCanceled => Event::CountdownCanceled,
        Outgoing::Quit => Event::Quit,
        Outgoing::RefreshWaiting { id } => Event::RefreshWaiting { id },
        Outgoing::NameTaken { id, url, path, total } => Event::NameTaken { id, url, path, total },
        Outgoing::RefreshMismatch { id, url, reason, error_kind } => {
            Event::RefreshMismatch { id, url, reason: failure(reason, error_kind) }
        }
        Outgoing::AddedMany { count, skipped } => Event::AddedMany { count, skipped },
        Outgoing::LinkInfo { request, index, total, content_type, error, error_kind } => {
            Event::LinkInfo { request, index, total, content_type, error: error.map(|e| failure(e, error_kind)) }
        }
        Outgoing::PickLinks { page, links, user_agent } => Event::PickLinks { page, links, user_agent },
        Outgoing::Quota { used, limit, paused, resumes_at } => Event::Quota { used, limit, paused, resumes_at },
        Outgoing::LimitLifted { lifted } => Event::LimitLifted { lifted },
        Outgoing::BrowserLink { connected, last_seen } => Event::BrowserLink { connected, last_seen },
        Outgoing::Update { newer, error, error_kind } => {
            Event::Update { newer, error: error.map(|e| failure(e, error_kind)) }
        }
        Outgoing::UpdateFailed { error, error_kind } => Event::UpdateFailed { error: failure(error, error_kind) },
        Outgoing::ZipPreview { id, entries, error, error_kind } => {
            Event::ZipPreview { id, entries, error: error.map(|e| failure(e, error_kind)) }
        }
        Outgoing::GrabProgress { pages, bytes, found, explored, added } => {
            Event::GrabProgress { pages, bytes, found, explored, added }
        }
        Outgoing::GrabDone { files, pages, explored, stopped, login_cookies } => {
            Event::GrabDone { files, pages, explored, stopped, login_cookies }
        }
        Outgoing::Error { message, error_kind } => Event::Error { error: failure(message, error_kind) },
        Outgoing::Rejected { url, message, error_kind } => Event::Rejected { url, error: failure(message, error_kind) },
        Outgoing::Remote { state } => Event::Remote { state },
    }
}

/// The wire message for `event`. `None` for `Connected` and `Lost`, which
/// a client's own connection reports and nothing sends.
pub fn outgoing(event: Event) -> Option<Outgoing> {
    Some(match event {
        Event::Connected | Event::Lost(_) => return None,
        Event::Hello { name, version, api } => Outgoing::Hello { name, version, api },
        Event::Pong => Outgoing::Pong,
        Event::Ack { id, url, status } => Outgoing::Ack { id, url, status },
        Event::MediaInfo { request, info } => Outgoing::MediaInfo { request, info },
        Event::CookiesWanted { request, url } => Outgoing::CookiesWanted { request, url },
        Event::RenderPage { request, url } => Outgoing::RenderPage { request, url },
        Event::CommandLine { window_open } => Outgoing::CommandLine { window_open },
        Event::Added { id, url } => Outgoing::Added { id, url },
        Event::Started { id, url, path, total, resumable, connections, partial, validator } => {
            Outgoing::Started { id, url, path, total, resumable, connections, partial, validator }
        }
        Event::Progress { id, url, done, total, parts, segments } => {
            Outgoing::Progress { id, url, done, total, parts, segments }
        }
        Event::Done { id, url, path, note } => Outgoing::Done { id, url, path, note },
        Event::Failed { id, url, error } => Outgoing::Failed { id, url, error },
        Event::Canceled { id, url } => Outgoing::Canceled { id, url },
        Event::Removed { id } => Outgoing::Removed { id },
        Event::Updated { item } => Outgoing::Updated { item },
        Event::Downloads { items } => Outgoing::Downloads { items },
        Event::Settings { settings } => Outgoing::Settings { settings },
        Event::Show => Outgoing::Show,
        Event::ShowDone { id } => Outgoing::ShowDone { id },
        Event::Queues { items } => Outgoing::Queues { items },
        Event::Countdown { action, seconds, queue, file } => Outgoing::Countdown { action, seconds, queue, file },
        Event::CountdownCanceled => Outgoing::CountdownCanceled,
        Event::Quit => Outgoing::Quit,
        Event::RefreshWaiting { id } => Outgoing::RefreshWaiting { id },
        Event::NameTaken { id, url, path, total } => Outgoing::NameTaken { id, url, path, total },
        Event::RefreshMismatch { id, url, reason } => {
            let (reason, error_kind) = text_and_kind(reason);
            Outgoing::RefreshMismatch { id, url, reason, error_kind }
        }
        Event::AddedMany { count, skipped } => Outgoing::AddedMany { count, skipped },
        Event::LinkInfo { request, index, total, content_type, error } => {
            let (error, error_kind) = optional(error);
            Outgoing::LinkInfo { request, index, total, content_type, error, error_kind }
        }
        Event::PickLinks { page, links, user_agent } => Outgoing::PickLinks { page, links, user_agent },
        Event::Quota { used, limit, paused, resumes_at } => Outgoing::Quota { used, limit, paused, resumes_at },
        Event::LimitLifted { lifted } => Outgoing::LimitLifted { lifted },
        Event::BrowserLink { connected, last_seen } => Outgoing::BrowserLink { connected, last_seen },
        Event::Update { newer, error } => {
            let (error, error_kind) = optional(error);
            Outgoing::Update { newer, error, error_kind }
        }
        Event::UpdateFailed { error } => {
            let (error, error_kind) = text_and_kind(error);
            Outgoing::UpdateFailed { error, error_kind }
        }
        Event::ZipPreview { id, entries, error } => {
            let (error, error_kind) = optional(error);
            Outgoing::ZipPreview { id, entries, error, error_kind }
        }
        Event::GrabProgress { pages, bytes, found, explored, added } => {
            Outgoing::GrabProgress { pages, bytes, found, explored, added }
        }
        Event::GrabDone { files, pages, explored, stopped, login_cookies } => {
            Outgoing::GrabDone { files, pages, explored, stopped, login_cookies }
        }
        Event::Error { error } => {
            let (message, error_kind) = text_and_kind(error);
            Outgoing::Error { message, error_kind }
        }
        Event::Rejected { url, error } => {
            let (message, error_kind) = text_and_kind(error);
            Outgoing::Rejected { url, message, error_kind }
        }
        Event::Remote { state } => Outgoing::Remote { state },
    })
}

/// A failure as the wire carries it: its English text, which clients that
/// do not translate show (the browser extension, `pullbay-host send`), and its
/// kind and values as `error_kind`. A failure known only by its text has no
/// kind.
pub fn text_and_kind(failure: Failure) -> (String, Option<serde_json::Value>) {
    let text = failure.to_string();
    let kind = WireFailure::of(failure).and_then(|w| serde_json::to_value(w).ok());
    (text, kind)
}

fn optional(failure: Option<Failure>) -> (Option<String>, Option<serde_json::Value>) {
    match failure.map(text_and_kind) {
        Some((text, kind)) => (Some(text), kind),
        None => (None, None),
    }
}

/// The failure that `text` and `kind` describe. Without a kind this
/// client can read (a daemon from before kinds, or a kind newer than this
/// client), it is the text alone; an older daemon's "invalid message: ..."
/// is still known for what it is.
pub fn failure(text: String, kind: Option<serde_json::Value>) -> Failure {
    if let Some(w) = kind.and_then(|k| serde_json::from_value::<WireFailure>(k).ok()) {
        return w.into();
    }
    match text.strip_prefix(INVALID_MESSAGE) {
        Some(why) => Failure::InvalidMessage(why.to_string()),
        None => Failure::Other(text),
    }
}

/// How `Failure::InvalidMessage` reads, which daemons sent as text alone.
const INVALID_MESSAGE: &str = "invalid message: ";

/// `Failure`'s shape on the wire: the application's enums, which do not know
/// the wire, mirrored here around the domain's, which serialize themselves.
/// These variant names, and the domain enums', are the wire format: renaming
/// one changes what clients read (the golden tests below pin them).
#[derive(Serialize, Deserialize)]
enum WireFailure {
    Service(WireService),
    Settings(SettingsError),
    Mismatch(Mismatch),
    InvalidMessage(String),
    Probe(String),
}

#[derive(Serialize, Deserialize)]
enum WireService {
    Queue(QueueError),
    Download(DownloadError),
    Confirm(WireConfirm),
    Update(UpdateError),
    Grab(InvalidGrab),
    CrawlRunning,
    BadBody(String),
    NotDownloadAddress(String),
    Pairing(PairingError),
}

#[derive(Serialize, Deserialize)]
enum WireConfirm {
    Download(DownloadError),
    StartControl(ControlError),
    StartQueue(QueueError),
}

impl WireFailure {
    /// `None` for `Failure::Other`, which is its text alone.
    fn of(f: Failure) -> Option<WireFailure> {
        Some(match f {
            Failure::Service(e) => WireFailure::Service(WireService::of(e)),
            Failure::Settings(e) => WireFailure::Settings(e),
            Failure::Mismatch(e) => WireFailure::Mismatch(e),
            Failure::InvalidMessage(why) => WireFailure::InvalidMessage(why),
            Failure::Probe(why) => WireFailure::Probe(why),
            Failure::Other(_) => return None,
        })
    }
}

impl From<WireFailure> for Failure {
    fn from(w: WireFailure) -> Self {
        match w {
            WireFailure::Service(e) => Failure::Service(e.into()),
            WireFailure::Settings(e) => Failure::Settings(e),
            WireFailure::Mismatch(e) => Failure::Mismatch(e),
            WireFailure::InvalidMessage(why) => Failure::InvalidMessage(why),
            WireFailure::Probe(why) => Failure::Probe(why),
        }
    }
}

impl WireService {
    fn of(e: ServiceError) -> WireService {
        match e {
            ServiceError::Queue(e) => WireService::Queue(e),
            ServiceError::Download(e) => WireService::Download(e),
            ServiceError::Confirm(ConfirmError::Download(e)) => WireService::Confirm(WireConfirm::Download(e)),
            ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Start(e))) => {
                WireService::Confirm(WireConfirm::StartControl(e))
            }
            ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Queue(e))) => {
                WireService::Confirm(WireConfirm::StartQueue(e))
            }
            ServiceError::Update(e) => WireService::Update(e),
            ServiceError::Grab(e) => WireService::Grab(e),
            ServiceError::CrawlRunning => WireService::CrawlRunning,
            ServiceError::BadBody(url) => WireService::BadBody(url),
            ServiceError::NotDownloadAddress(url) => WireService::NotDownloadAddress(url),
            ServiceError::Pairing(e) => WireService::Pairing(e),
        }
    }
}

impl From<WireService> for ServiceError {
    fn from(w: WireService) -> Self {
        match w {
            WireService::Queue(e) => ServiceError::Queue(e),
            WireService::Download(e) => ServiceError::Download(e),
            WireService::Confirm(WireConfirm::Download(e)) => ServiceError::Confirm(ConfirmError::Download(e)),
            WireService::Confirm(WireConfirm::StartControl(e)) => {
                ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Start(e)))
            }
            WireService::Confirm(WireConfirm::StartQueue(e)) => {
                ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Queue(e)))
            }
            WireService::Update(e) => ServiceError::Update(e),
            WireService::Grab(e) => ServiceError::Grab(e),
            WireService::CrawlRunning => ServiceError::CrawlRunning,
            WireService::BadBody(url) => ServiceError::BadBody(url),
            WireService::NotDownloadAddress(url) => ServiceError::NotDownloadAddress(url),
            WireService::Pairing(e) => ServiceError::Pairing(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pullbay_domain::bytes::ByteCount;
    use pullbay_domain::download::Download;
    use pullbay_domain::id::{DownloadId, QueueId};

    /// `msg` as JSON, after a trip through `f` and back.
    fn json<T: serde::Serialize>(msg: &T) -> serde_json::Value {
        serde_json::to_value(msg).unwrap()
    }

    #[test]
    fn commands_keep_their_fields_on_the_wire() {
        let d = Download { url: "http://x/f".into(), ..Download::default() };
        let msg = incoming(Command::AddMany { items: vec![d], queue: Some(QueueId::new(3)) });
        let value = json(&msg);
        assert_eq!(value["op"], "add_many");
        assert_eq!(value["queue"], 3);
        assert_eq!(value["items"][0]["url"], "http://x/f");
        assert_eq!(json(&incoming(command(msg))), value, "back and forth is the same message");
        let msg = incoming(Command::Remove { id: DownloadId::new(2), delete_files: true });
        assert_eq!(json(&msg), serde_json::json!({"op": "remove", "id": 2, "delete_files": true}));
    }

    #[test]
    fn every_request_the_host_sends_reads_as_a_command() {
        for raw in [
            r#"{"op":"hello","version":"1"}"#,
            r#"{"op":"ping"}"#,
            r#"{"op":"cookies","request":4,"cookies":"a=b"}"#,
            r#"{"op":"rendered","request":5}"#,
            r#"{"op":"pick_links","page":"https://p/","links":[]}"#,
        ] {
            let msg: Incoming = serde_json::from_str(raw).unwrap();
            let value = json(&msg);
            assert_eq!(json(&incoming(command(msg))), value, "{raw}");
        }
    }

    #[test]
    fn events_go_out_as_they_came_in() {
        for msg in [
            Outgoing::Pong,
            Outgoing::Ack { id: DownloadId::new(1), url: "u".into(), status: "accepted".into() },
            Outgoing::CommandLine { window_open: true },
            Outgoing::RenderPage { request: 2, url: "u".into() },
            Outgoing::Error { message: "no".into(), error_kind: None },
        ] {
            let value = json(&msg);
            assert_eq!(json(&outgoing(event(msg)).unwrap()), value);
        }
        assert!(outgoing(Event::Connected).is_none());
    }

    /// `event` as the JSON a client reads, and that JSON read back.
    fn round_trip(event: Event) -> (serde_json::Value, Event) {
        let value = json(&outgoing(event).unwrap());
        let back: Outgoing = serde_json::from_value(value.clone()).unwrap();
        (value, super::event(back))
    }

    #[test]
    fn typed_failures_keep_their_kind_and_values() {
        let failures: Vec<Failure> = vec![
            ServiceError::NotDownloadAddress("javascript:x".into()).into(),
            ServiceError::CrawlRunning.into(),
            QueueError::NameTaken("Night".into()).into(),
            QueueError::Invalid(pullbay_domain::queue::InvalidQueue::StartTime("25:00".into())).into(),
            DownloadError::BadSpeedLimit { kbps: 1, lo: 2, hi: 3 }.into(),
            ServiceError::Confirm(ConfirmError::Download(DownloadError::AnswerNoFolder)).into(),
            ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Start(ControlError("busy".into())))).into(),
            ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Queue(QueueError::NoQueue(QueueId::new(4)))))
                .into(),
            UpdateError::Save { path: "/t/s".into(), reason: "denied".into() }.into(),
            ServiceError::Grab(InvalidGrab::Depth).into(),
            SettingsError::Invalid(pullbay_domain::settings::InvalidSettings::FolderNotFull(
                pullbay_domain::settings::SettingsField::CategoryFolder("Music".into()),
                "rel".into(),
            ))
            .into(),
            SettingsError::Refused("read-only".into()).into(),
            Failure::InvalidMessage("expected value".into()),
            Failure::Probe("HTTP 404".into()),
            ServiceError::Pairing(PairingError::NotListening).into(),
            ServiceError::Pairing(PairingError::NoDevice(pullbay_domain::id::DeviceId::new(3))).into(),
        ];
        for f in failures {
            let text = f.to_string();
            let (value, back) = round_trip(Event::Error { error: f.clone() });
            assert_eq!(value["message"], text.as_str(), "text-only clients read the message");
            assert!(value.get("error_kind").is_some(), "{f:?}");
            assert_eq!(back, Event::Error { error: f.clone() });

            let (value, back) = round_trip(Event::Rejected { url: "u".into(), error: f.clone() });
            assert_eq!(value["message"], text.as_str());
            assert_eq!(back, Event::Rejected { url: "u".into(), error: f.clone() });

            let (value, back) = round_trip(Event::UpdateFailed { error: f.clone() });
            assert_eq!(value["error"], text.as_str());
            assert_eq!(back, Event::UpdateFailed { error: f.clone() });

            let (value, back) = round_trip(Event::Update { newer: None, error: Some(f.clone()) });
            assert_eq!(value["error"], text.as_str());
            assert_eq!(back, Event::Update { newer: None, error: Some(f.clone()) });

            let zip = Event::ZipPreview { id: DownloadId::new(1), entries: Vec::new(), error: Some(f.clone()) };
            let (value, back) = round_trip(zip.clone());
            assert_eq!(value["error"], text.as_str());
            assert_eq!(back, zip);

            let link = Event::LinkInfo { request: 1, index: 0, total: None, content_type: None, error: Some(f) };
            let (value, back) = round_trip(link.clone());
            assert_eq!(value["error"], text.as_str());
            assert_eq!(back, link);
        }
    }

    #[test]
    fn remote_control_goes_as_its_ops() {
        use pullbay_domain::id::DeviceId;
        use pullbay_domain::pairing::{
            DeviceInfo, DeviceName, ListenerState, PairingCode, PairingOffer, RemoteFault, RemoteState,
        };
        for (command, op) in [
            (Command::RemoteStatus, serde_json::json!({"op": "remote_status"})),
            (Command::StartPairing, serde_json::json!({"op": "start_pairing"})),
            (Command::CancelPairing, serde_json::json!({"op": "cancel_pairing"})),
            (Command::RemoveDevice { id: DeviceId::new(2) }, serde_json::json!({"op": "remove_device", "id": 2})),
        ] {
            let msg = incoming(command);
            assert_eq!(json(&msg), op);
            assert_eq!(json(&incoming(super::command(serde_json::from_value(op).unwrap()))), json(&msg));
        }
        let code = PairingCode::from_random(&[7; 32]);
        let state = RemoteState {
            listener: ListenerState {
                listening: vec!["0.0.0.0:18443".into()],
                page: Some("https://192.168.1.5:18443/".into()),
                fault: None,
                fingerprint: Some("AB:CD".into()),
            },
            devices: vec![DeviceInfo {
                id: DeviceId::new(1),
                name: DeviceName::clean("Pixel"),
                paired_at: 5,
                last_seen: None,
            }],
            pairing: Some(PairingOffer {
                link: format!("https://192.168.1.5:18443/#pair={}", code.as_str()),
                code: code.clone(),
                expires_at: 300,
            }),
        };
        let (value, back) = round_trip(Event::Remote { state: state.clone() });
        assert_eq!(
            value,
            serde_json::json!({"op": "remote", "state": {
                "listening": ["0.0.0.0:18443"],
                "page": "https://192.168.1.5:18443/",
                "fingerprint": "AB:CD",
                "devices": [{"id": 1, "name": "Pixel", "paired_at": 5}],
                "pairing": {"link": format!("https://192.168.1.5:18443/#pair={}", code.as_str()), "code": code.as_str(), "expires_at": 300},
            }})
        );
        assert_eq!(back, Event::Remote { state });
        let fault = RemoteState {
            listener: ListenerState {
                fault: Some(RemoteFault::PortInUse { address: "0.0.0.0:18443".into() }),
                ..ListenerState::default()
            },
            ..RemoteState::default()
        };
        let (value, back) = round_trip(Event::Remote { state: fault.clone() });
        assert_eq!(value["state"]["fault"], serde_json::json!({"PortInUse": {"address": "0.0.0.0:18443"}}));
        assert_eq!(back, Event::Remote { state: fault });
    }

    #[test]
    fn a_lifted_speed_limit_goes_as_its_op() {
        let (value, back) = round_trip(Event::LimitLifted { lifted: true });
        assert_eq!(value, serde_json::json!({"op": "limit_lifted", "lifted": true}));
        assert_eq!(back, Event::LimitLifted { lifted: true });
    }

    #[test]
    fn a_failure_known_only_by_its_text_goes_as_text() {
        let (value, back) = round_trip(Event::Error { error: Failure::Other("as said".into()) });
        assert_eq!(value, serde_json::json!({"op": "error", "message": "as said"}));
        assert_eq!(back, Event::Error { error: Failure::Other("as said".into()) });
        let (value, _) = round_trip(Event::Update { newer: None, error: None });
        assert_eq!(value, serde_json::json!({"op": "update"}), "no error, no kind");
    }

    /// Messages from a daemon that sent only text, or a kind this client
    /// does not know, still read.
    #[test]
    fn text_only_messages_read_as_their_text() {
        let read = |raw: &str| super::event(serde_json::from_str::<Outgoing>(raw).unwrap());
        assert_eq!(
            read(r#"{"op":"error","message":"no queue with id 7"}"#),
            Event::Error { error: Failure::Other("no queue with id 7".into()) }
        );
        assert_eq!(
            read(r#"{"op":"error","message":"invalid message: unknown variant `x`"}"#),
            Event::Error { error: Failure::InvalidMessage("unknown variant `x`".into()) }
        );
        assert_eq!(
            read(r#"{"op":"rejected","url":"u","message":"m","error_kind":{"FromTheFuture":1}}"#),
            Event::Rejected { url: "u".into(), error: Failure::Other("m".into()) }
        );
        assert_eq!(
            read(r#"{"op":"update_failed","error":"e"}"#),
            Event::UpdateFailed { error: Failure::Other("e".into()) }
        );
        assert_eq!(
            read(r#"{"op":"link_info","request":1,"index":2,"error":"refused"}"#),
            Event::LinkInfo {
                request: 1,
                index: 2,
                total: None,
                content_type: None,
                error: Some(Failure::Other("refused".into()))
            }
        );
        assert_eq!(
            read(r#"{"op":"zip_preview","id":3,"error":"not a zip"}"#),
            Event::ZipPreview {
                id: DownloadId::new(3),
                entries: Vec::new(),
                error: Some(Failure::Other("not a zip".into()))
            }
        );
    }

    /// What a typed failure looks like on the wire. The variant names of
    /// the wire mirrors and of the domain enums are the wire format, so a
    /// rename shows up here rather than in the clients.
    #[test]
    fn the_kind_names_the_error_and_its_values() {
        use pullbay_domain::command::CommandError;
        use pullbay_domain::queue::InvalidQueue;
        use pullbay_domain::settings::{InvalidSettings, ProxyEntry, SettingsField};
        use serde_json::json;
        let golden: Vec<(Failure, serde_json::Value)> = vec![
            (ServiceError::CrawlRunning.into(), json!({"Service": "CrawlRunning"})),
            (ServiceError::BadBody("http://a/".into()).into(), json!({"Service": {"BadBody": "http://a/"}})),
            (ServiceError::NotDownloadAddress("x:y".into()).into(), json!({"Service": {"NotDownloadAddress": "x:y"}})),
            (QueueError::NoQueue(QueueId::new(7)).into(), json!({"Service": {"Queue": {"NoQueue": 7}}})),
            (QueueError::MainQueue.into(), json!({"Service": {"Queue": "MainQueue"}})),
            (QueueError::NameTaken("N".into()).into(), json!({"Service": {"Queue": {"NameTaken": "N"}}})),
            (
                QueueError::Invalid(InvalidQueue::StartTime("25:00".into())).into(),
                json!({"Service": {"Queue": {"Invalid": {"StartTime": "25:00"}}}}),
            ),
            (DownloadError::NoDownload(DownloadId::new(3)).into(), json!({"Service": {"Download": {"NoDownload": 3}}})),
            (
                DownloadError::BadSpeedLimit { kbps: 1, lo: 2, hi: 3 }.into(),
                json!({"Service": {"Download": {"BadSpeedLimit": {"kbps": 1, "lo": 2, "hi": 3}}}}),
            ),
            (
                DownloadError::FilesStay { url: "u".into(), reason: "r".into() }.into(),
                json!({"Service": {"Download": {"FilesStay": {"url": "u", "reason": "r"}}}}),
            ),
            (UpdateError::NewestAlready.into(), json!({"Service": {"Update": "NewestAlready"}})),
            (
                UpdateError::Save { path: "/t/s".into(), reason: "denied".into() }.into(),
                json!({"Service": {"Update": {"Save": {"path": "/t/s", "reason": "denied"}}}}),
            ),
            (ServiceError::Grab(InvalidGrab::Depth).into(), json!({"Service": {"Grab": "Depth"}})),
            (
                ServiceError::Confirm(ConfirmError::Download(DownloadError::AnswerNoFolder)).into(),
                json!({"Service": {"Confirm": {"Download": "AnswerNoFolder"}}}),
            ),
            (
                ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Start(ControlError("busy".into())))).into(),
                json!({"Service": {"Confirm": {"StartControl": "busy"}}}),
            ),
            (
                ServiceError::Confirm(ConfirmError::Start(StartOrWaitError::Queue(QueueError::NoQueue(QueueId::new(
                    4,
                )))))
                .into(),
                json!({"Service": {"Confirm": {"StartQueue": {"NoQueue": 4}}}}),
            ),
            (SettingsError::Refused("read-only".into()).into(), json!({"Settings": {"Refused": "read-only"}})),
            (SettingsError::Invalid(InvalidSettings::QuotaZero).into(), json!({"Settings": {"Invalid": "QuotaZero"}})),
            (
                SettingsError::Invalid(InvalidSettings::FolderNotFull(
                    SettingsField::CategoryFolder("Music".into()),
                    "rel".into(),
                ))
                .into(),
                json!({"Settings": {"Invalid": {"FolderNotFull": [{"CategoryFolder": "Music"}, "rel"]}}}),
            ),
            (
                SettingsError::Invalid(InvalidSettings::ProxyNotHostPort(ProxyEntry::Socks)).into(),
                json!({"Settings": {"Invalid": {"ProxyNotHostPort": "Socks"}}}),
            ),
            (
                SettingsError::Invalid(InvalidSettings::Scanner(CommandError::NoPath)).into(),
                json!({"Settings": {"Invalid": {"Scanner": "NoPath"}}}),
            ),
            (Mismatch::Changed.into(), json!({"Mismatch": "Changed"})),
            (
                Mismatch::Size { new: ByteCount::new(1), partial: ByteCount::new(2) }.into(),
                json!({"Mismatch": {"Size": {"new": 1, "partial": 2}}}),
            ),
            (Failure::InvalidMessage("eof".into()), json!({"InvalidMessage": "eof"})),
            (Failure::Probe("HTTP 404".into()), json!({"Probe": "HTTP 404"})),
        ];
        for (failure, kind) in golden {
            let (value, back) = round_trip(Event::Error { error: failure.clone() });
            assert_eq!(value["error_kind"], kind, "{failure:?}");
            assert_eq!(back, Event::Error { error: failure });
        }
    }

    #[test]
    fn a_new_links_mismatch_keeps_its_reason_and_kind() {
        let reason = Failure::from(Mismatch::Size { new: ByteCount::new(2 << 20), partial: ByteCount::new(1 << 20) });
        let (value, back) =
            round_trip(Event::RefreshMismatch { id: DownloadId::new(1), url: "u".into(), reason: reason.clone() });
        assert_eq!(value["reason"], "The new link is for a 2.0 MB file; the partial download is of a 1.0 MB file.");
        assert_eq!(
            value["error_kind"],
            serde_json::json!({"Mismatch": {"Size": {"new": 2 << 20, "partial": 1 << 20}}})
        );
        assert_eq!(back, Event::RefreshMismatch { id: DownloadId::new(1), url: "u".into(), reason });
        // From a daemon that sent the reason as text alone.
        let old: Outgoing =
            serde_json::from_str(r#"{"op":"refresh_mismatch","id":1,"url":"u","reason":"other file"}"#).unwrap();
        assert_eq!(
            super::event(old),
            Event::RefreshMismatch {
                id: DownloadId::new(1),
                url: "u".into(),
                reason: Failure::Other("other file".into())
            }
        );
    }
}
