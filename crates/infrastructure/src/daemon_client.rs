//! The window's connection to the daemon: the adapter for
//! `uidm_application::daemon::DaemonClient` over `ipc` and `protocol`.
//!
//! It connects (starting or restarting the daemon as `ipc` does, with its
//! handshake), introduces itself, maps each `Command` to an `Incoming`
//! frame and each `Outgoing` frame to an `Event`, and says when the
//! connection goes.

use std::io::{self, ErrorKind};
use std::net::TcpStream;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use uidm_application::daemon::{Command, DaemonClient, DaemonError, Event, Lost};
use uidm_domain::download::CommandLine;

use crate::app::MAX_MESSAGE_BYTES;
use crate::ipc;
use crate::protocol::{read_message, write_frame, Incoming, Outgoing};

/// How long a command line waits for the daemon's answer.
const COMMAND_LINE_TIMEOUT: Duration = Duration::from_secs(10);

/// Opens a connection to the daemon, past its handshake; `true` restarts
/// the daemon first.
pub type Connector = dyn Fn(bool) -> io::Result<TcpStream> + Send + Sync;

/// A connection to the daemon over loopback TCP.
pub struct IpcDaemonClient {
    /// The `client` this connection names in its `hello`.
    client: String,
    /// The connection to write to, while there is one.
    writer: Arc<Mutex<Option<TcpStream>>>,
    connector: Arc<Connector>,
}

/// Lock `m`. The guarded value is a plain stream slot, so one a panicking
/// thread left behind is still usable.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl IpcDaemonClient {
    /// A connection that names itself `client` (the desktop window says
    /// `window`), starting the daemon when none runs.
    pub fn new(client: &str) -> Self {
        Self::with_connector(
            client,
            Arc::new(|restart| if restart { ipc::restart_daemon() } else { ipc::connect_or_spawn() }),
        )
    }

    /// A connection made by `connector` instead of `ipc`.
    pub fn with_connector(client: &str, connector: Arc<Connector>) -> Self {
        IpcDaemonClient { client: client.to_string(), writer: Arc::new(Mutex::new(None)), connector }
    }

    fn hello(client: &str) -> Incoming {
        Incoming::Hello { version: Some(env!("CARGO_PKG_VERSION").to_string()), client: Some(client.to_string()) }
    }
}

/// Write `msg` to the connection in `writer`, if there is one.
fn write(writer: &Mutex<Option<TcpStream>>, msg: &Incoming) -> bool {
    let mut guard = lock(writer);
    let Some(stream) = guard.as_mut() else {
        return false;
    };
    match serde_json::to_vec(msg) {
        Ok(frame) => write_frame(stream, &frame).is_ok(),
        Err(_) => false,
    }
}

impl DaemonClient for IpcDaemonClient {
    fn connect(&self, restart: bool, mut on_event: Box<dyn FnMut(Event) + Send>) {
        let (writer, connector, client) = (Arc::clone(&self.writer), Arc::clone(&self.connector), self.client.clone());
        thread::spawn(move || {
            let stream = match connector(restart) {
                Ok(s) => s,
                Err(e) => return on_event(Event::Lost(Lost::Failed(e.to_string()))),
            };
            let mut reader = match stream.try_clone() {
                Ok(r) => r,
                Err(e) => return on_event(Event::Lost(Lost::Failed(e.to_string()))),
            };
            *lock(&writer) = Some(stream);
            on_event(Event::Connected);
            for msg in [Self::hello(&client), Incoming::List, Incoming::GetSettings] {
                write(&writer, &msg);
            }
            while let Ok(Some(raw)) = read_message(&mut reader, MAX_MESSAGE_BYTES) {
                // A message this build does not know is skipped.
                if let Some(event) = serde_json::from_slice::<Outgoing>(&raw).ok().and_then(event) {
                    on_event(event);
                }
            }
            *lock(&writer) = None;
            on_event(Event::Lost(Lost::Closed));
        });
    }

    fn send(&self, command: Command) -> bool {
        write(&self.writer, &incoming(command))
    }

    fn run_command_line(&self, command: CommandLine) -> Result<bool, DaemonError> {
        let connect = |e: io::Error| DaemonError::Connect(e.to_string());
        let mut stream = (self.connector)(false).map_err(connect)?;
        stream.set_read_timeout(Some(COMMAND_LINE_TIMEOUT)).map_err(connect)?;
        for msg in [Self::hello("command-line"), Incoming::CommandLine(command)] {
            let frame = serde_json::to_vec(&msg).map_err(|e| DaemonError::Connect(e.to_string()))?;
            write_frame(&mut stream, &frame).map_err(connect)?;
        }
        loop {
            let raw = match read_message(&mut stream, MAX_MESSAGE_BYTES) {
                Ok(Some(raw)) => raw,
                Ok(None) => return Err(DaemonError::Closed),
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Err(DaemonError::Closed),
                Err(e) => return Err(connect(e)),
            };
            match serde_json::from_slice::<Outgoing>(&raw) {
                Ok(Outgoing::CommandLine { window_open }) => return Ok(window_open),
                // A daemon older than this window does not know the message.
                Ok(Outgoing::Error { message }) => return Err(DaemonError::Refused(message)),
                _ => {}
            }
        }
    }
}

/// The wire message for `command`.
pub fn incoming(command: Command) -> Incoming {
    match command {
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

/// The event `msg` is for a window, if it is one. Replies only the host or
/// the browser extension uses, and the command line's answer, are not.
pub fn event(msg: Outgoing) -> Option<Event> {
    Some(match msg {
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
        Outgoing::Hello { .. }
        | Outgoing::Pong
        | Outgoing::Ack { .. }
        | Outgoing::MediaInfo { .. }
        | Outgoing::CookiesWanted { .. }
        | Outgoing::RenderPage { .. }
        | Outgoing::CommandLine { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc::{channel, Receiver};
    use uidm_domain::download::Download;

    /// A pretend daemon on a local port: it hands each accepted
    /// connection to `serve`.
    fn fake_daemon(serve: impl Fn(TcpStream) + Send + Sync + 'static) -> Arc<Connector> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = Arc::new(serve);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let serve = Arc::clone(&serve);
                thread::spawn(move || serve(stream));
            }
        });
        Arc::new(move |_restart| TcpStream::connect(addr))
    }

    fn read(stream: &mut TcpStream) -> Option<Incoming> {
        let raw = read_message(stream, MAX_MESSAGE_BYTES).ok()??;
        serde_json::from_slice(&raw).ok()
    }

    fn send(stream: &mut TcpStream, msg: &Outgoing) {
        write_frame(stream, &serde_json::to_vec(msg).unwrap()).unwrap();
    }

    fn events(client: &IpcDaemonClient) -> Receiver<Event> {
        let (tx, rx) = channel();
        client.connect(
            false,
            Box::new(move |e| {
                let _ = tx.send(e);
            }),
        );
        rx
    }

    fn next(rx: &Receiver<Event>) -> Event {
        rx.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn it_introduces_itself_then_relays_both_ways() {
        let (got_tx, got) = channel();
        let got_tx = Mutex::new(got_tx);
        let connector = fake_daemon(move |mut stream| {
            // Hello, list and settings come first.
            for _ in 0..3 {
                let _ = lock(&got_tx).send(read(&mut stream));
            }
            send(&mut stream, &Outgoing::Added { id: 4, url: "http://x/f".into() });
            send(&mut stream, &Outgoing::Pong);
            send(&mut stream, &Outgoing::Removed { id: 4 });
            let _ = lock(&got_tx).send(read(&mut stream));
        });
        let client = IpcDaemonClient::with_connector("window", connector);
        let rx = events(&client);
        assert_eq!(next(&rx), Event::Connected);
        let hello = got.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(hello, Some(Incoming::Hello { client: Some(ref c), .. }) if c == "window"));
        assert!(matches!(got.recv().unwrap(), Some(Incoming::List)));
        assert!(matches!(got.recv().unwrap(), Some(Incoming::GetSettings)));
        assert_eq!(next(&rx), Event::Added { id: 4, url: "http://x/f".into() });
        // `pong` is not for a window.
        assert_eq!(next(&rx), Event::Removed { id: 4 });
        assert!(client.send(Command::Cancel { id: 4 }));
        assert!(matches!(got.recv_timeout(Duration::from_secs(5)).unwrap(), Some(Incoming::Cancel { id: 4 })));
        // The pretend daemon hangs up after that.
        assert_eq!(next(&rx), Event::Lost(Lost::Closed));
        assert!(!client.send(Command::List));
    }

    #[test]
    fn a_failed_connection_says_why() {
        let connector: Arc<Connector> = Arc::new(|_| Err(io::Error::other("no daemon here")));
        let client = IpcDaemonClient::with_connector("window", connector);
        let rx = events(&client);
        assert_eq!(next(&rx), Event::Lost(Lost::Failed("no daemon here".into())));
        assert!(!client.send(Command::List));
    }

    #[test]
    fn a_command_line_waits_for_its_answer() {
        let connector = fake_daemon(|mut stream| {
            assert!(matches!(read(&mut stream), Some(Incoming::Hello { client: Some(c), .. }) if c == "command-line"));
            match read(&mut stream) {
                Some(Incoming::CommandLine(cmd)) if cmd.start_queue => {
                    send(&mut stream, &Outgoing::Pong);
                    send(&mut stream, &Outgoing::CommandLine { window_open: true });
                }
                _ => send(&mut stream, &Outgoing::Error { message: "unexpected".into() }),
            }
        });
        let client = IpcDaemonClient::with_connector("window", connector);
        let cmd = CommandLine { start_queue: true, ..CommandLine::default() };
        assert_eq!(client.run_command_line(cmd), Ok(true));
        let refused = client.run_command_line(CommandLine::default());
        assert_eq!(refused, Err(DaemonError::Refused("unexpected".into())));
    }

    #[test]
    fn a_daemon_that_hangs_up_on_a_command_line_closed_it() {
        let connector = fake_daemon(|mut stream| {
            let _ = read(&mut stream);
            let _ = read(&mut stream);
        });
        let client = IpcDaemonClient::with_connector("window", connector);
        assert_eq!(client.run_command_line(CommandLine::default()), Err(DaemonError::Closed));
    }

    #[test]
    fn commands_keep_their_fields_on_the_wire() {
        let d = Download { url: "http://x/f".into(), ..Download::default() };
        let json = serde_json::to_value(incoming(Command::AddMany { items: vec![d], queue: Some(3) })).unwrap();
        assert_eq!(json["op"], "add_many");
        assert_eq!(json["queue"], 3);
        assert_eq!(json["items"][0]["url"], "http://x/f");
        let json = serde_json::to_value(incoming(Command::Remove { id: 2, delete_files: true })).unwrap();
        assert_eq!(json, serde_json::json!({"op": "remove", "id": 2, "delete_files": true}));
    }
}
