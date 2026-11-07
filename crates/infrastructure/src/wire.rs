//! Mapping between the wire messages (`protocol::Incoming` and `Outgoing`)
//! and the application's `Command` and `Event`. Both sides of a
//! connection use it: the window's `daemon_client` sends commands and reads
//! events, and the daemon's `server` reads commands and sends events.

use uidm_application::daemon::{Command, Event};

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
    }
}

/// The event wire message `msg` carries.
pub fn event(msg: Outgoing) -> Event {
    match msg {
        Outgoing::Hello { name, version } => Event::Hello { name, version },
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
        Outgoing::RefreshMismatch { id, url, reason } => Event::RefreshMismatch { id, url, reason },
        Outgoing::AddedMany { count, skipped } => Event::AddedMany { count, skipped },
        Outgoing::LinkInfo { request, index, total, content_type, error } => {
            Event::LinkInfo { request, index, total, content_type, error }
        }
        Outgoing::PickLinks { page, links, user_agent } => Event::PickLinks { page, links, user_agent },
        Outgoing::Quota { used, limit, paused, resumes_at } => Event::Quota { used, limit, paused, resumes_at },
        Outgoing::BrowserLink { connected, last_seen } => Event::BrowserLink { connected, last_seen },
        Outgoing::Update { newer, error } => Event::Update { newer, error },
        Outgoing::UpdateFailed { error } => Event::UpdateFailed { error },
        Outgoing::ZipPreview { id, entries, error } => Event::ZipPreview { id, entries, error },
        Outgoing::GrabProgress { pages, bytes, found, explored, added } => {
            Event::GrabProgress { pages, bytes, found, explored, added }
        }
        Outgoing::GrabDone { files, pages, explored, stopped, login_cookies } => {
            Event::GrabDone { files, pages, explored, stopped, login_cookies }
        }
        Outgoing::Error { message } => Event::Error { message },
        Outgoing::Rejected { url, message } => Event::Rejected { url, message },
    }
}

/// The wire message for `event`. `None` for `Connected` and `Lost`, which
/// a client's own connection reports and nothing sends.
pub fn outgoing(event: Event) -> Option<Outgoing> {
    Some(match event {
        Event::Connected | Event::Lost(_) => return None,
        Event::Hello { name, version } => Outgoing::Hello { name, version },
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
        Event::RefreshMismatch { id, url, reason } => Outgoing::RefreshMismatch { id, url, reason },
        Event::AddedMany { count, skipped } => Outgoing::AddedMany { count, skipped },
        Event::LinkInfo { request, index, total, content_type, error } => {
            Outgoing::LinkInfo { request, index, total, content_type, error }
        }
        Event::PickLinks { page, links, user_agent } => Outgoing::PickLinks { page, links, user_agent },
        Event::Quota { used, limit, paused, resumes_at } => Outgoing::Quota { used, limit, paused, resumes_at },
        Event::BrowserLink { connected, last_seen } => Outgoing::BrowserLink { connected, last_seen },
        Event::Update { newer, error } => Outgoing::Update { newer, error },
        Event::UpdateFailed { error } => Outgoing::UpdateFailed { error },
        Event::ZipPreview { id, entries, error } => Outgoing::ZipPreview { id, entries, error },
        Event::GrabProgress { pages, bytes, found, explored, added } => {
            Outgoing::GrabProgress { pages, bytes, found, explored, added }
        }
        Event::GrabDone { files, pages, explored, stopped, login_cookies } => {
            Outgoing::GrabDone { files, pages, explored, stopped, login_cookies }
        }
        Event::Error { message } => Outgoing::Error { message },
        Event::Rejected { url, message } => Outgoing::Rejected { url, message },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uidm_domain::download::Download;

    /// `msg` as JSON, after a trip through `f` and back.
    fn json<T: serde::Serialize>(msg: &T) -> serde_json::Value {
        serde_json::to_value(msg).unwrap()
    }

    #[test]
    fn commands_keep_their_fields_on_the_wire() {
        let d = Download { url: "http://x/f".into(), ..Download::default() };
        let msg = incoming(Command::AddMany { items: vec![d], queue: Some(3) });
        let value = json(&msg);
        assert_eq!(value["op"], "add_many");
        assert_eq!(value["queue"], 3);
        assert_eq!(value["items"][0]["url"], "http://x/f");
        assert_eq!(json(&incoming(command(msg))), value, "back and forth is the same message");
        let msg = incoming(Command::Remove { id: 2, delete_files: true });
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
            Outgoing::Ack { id: 1, url: "u".into(), status: "accepted".into() },
            Outgoing::CommandLine { window_open: true },
            Outgoing::RenderPage { request: 2, url: "u".into() },
            Outgoing::Error { message: "no".into() },
        ] {
            let value = json(&msg);
            assert_eq!(json(&outgoing(event(msg)).unwrap()), value);
        }
        assert!(outgoing(Event::Connected).is_none());
    }
}
