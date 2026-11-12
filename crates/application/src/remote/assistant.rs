//! What an AI assistant can ask of UDM through `udm-mcp`: each
//! request is one of the remote use cases, run on a connection to the
//! daemon that stays open between requests.

use std::time::Duration;

use udm_domain::id::DownloadId;
use udm_domain::queue::Queue;

use super::{
    AddLink, AddRequest, Added, ControlQueue, ForgetDownload, ListDownloads, Listing, PauseDownload, RemoteError,
    ResumeDownload, Resumed, Session,
};
use crate::daemon::DaemonClient;

/// One download, or every one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    One(DownloadId),
    All,
}

/// A request from an assistant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Every download and queue.
    List,
    /// Add a link, starting it or leaving it in a queue.
    Add(AddRequest),
    /// Stop a running download, keeping its partial file, or everything.
    Pause(Which),
    /// Start a stopped download again, or everything a pause stopped.
    Resume(Which),
    /// Take a download out of the list; with `delete_files`, its files go
    /// to the Recycle Bin or trash too.
    Remove { id: DownloadId, delete_files: bool },
    /// Start (`start`) or stop the queue named by its name or id, or the
    /// main queue.
    Queue { name: Option<String>, start: bool },
}

/// What a request did.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Listing(Listing),
    Added(Added),
    Paused(Which),
    /// `None` when everything was resumed.
    Resumed(Option<Resumed>),
    Removed {
        id: DownloadId,
        delete_files: bool,
    },
    /// The queue as it was before.
    Queue {
        queue: Queue,
        started: bool,
    },
}

/// The port an assistant's server (`udm-mcp`'s) hands each request to.
pub trait Tools {
    fn run(&mut self, request: Request) -> Result<Reply, RemoteError>;
}

/// The requests run on one connection to the daemon, opened at the first
/// request and kept for the next. When it goes between requests, the next
/// one opens another (starting the daemon again if it stopped); when it goes
/// during one, that request fails, since it may have arrived, and the next
/// opens another.
pub struct Assistant<'c, C: DaemonClient + ?Sized> {
    client: &'c C,
    wait: Duration,
    session: Option<Session<'c, C>>,
}

impl<'c, C: DaemonClient + ?Sized> Assistant<'c, C> {
    /// Requests through `client`, each answer waited for up to `wait`.
    pub fn new(client: &'c C, wait: Duration) -> Self {
        Assistant { client, wait, session: None }
    }

    /// The open connection, caught up, or a new one.
    fn session(&mut self) -> Result<&Session<'c, C>, RemoteError> {
        if let Some(session) = &self.session {
            if let Err(e) = session.catch_up() {
                tracing::info!(error = %e, "the daemon connection went; connecting again");
                self.session = None;
            }
        }
        if self.session.is_none() {
            self.session = Some(Session::open(self.client, self.wait)?);
        }
        self.session.as_ref().ok_or(RemoteError::Lost("the connection ended".into()))
    }
}

impl<C: DaemonClient + ?Sized> Tools for Assistant<'_, C> {
    fn run(&mut self, request: Request) -> Result<Reply, RemoteError> {
        let result = answer(self.session()?, request);
        if result.as_ref().is_err_and(RemoteError::unreachable) {
            self.session = None;
        }
        result
    }
}

/// Run `request` on `session`.
fn answer<C: DaemonClient + ?Sized>(session: &Session<'_, C>, request: Request) -> Result<Reply, RemoteError> {
    match request {
        Request::List => ListDownloads::new(session).run().map(Reply::Listing),
        Request::Add(add) => AddLink::new(session).run(&add).map(Reply::Added),
        Request::Pause(Which::One(id)) => PauseDownload::new(session).run(id).map(|()| Reply::Paused(Which::One(id))),
        Request::Pause(Which::All) => PauseDownload::new(session).all().map(|()| Reply::Paused(Which::All)),
        Request::Resume(Which::One(id)) => ResumeDownload::new(session).run(id).map(|r| Reply::Resumed(Some(r))),
        Request::Resume(Which::All) => ResumeDownload::new(session).all().map(|()| Reply::Resumed(None)),
        Request::Remove { id, delete_files } => {
            ForgetDownload::new(session).run(id, delete_files).map(|()| Reply::Removed { id, delete_files })
        }
        Request::Queue { name, start } => {
            ControlQueue::new(session).run(name.as_deref(), start).map(|queue| Reply::Queue { queue, started: start })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::{Command, Event, Failure, Lost};
    use crate::memory::{row, FakeDaemon};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use udm_domain::address::Address;
    use udm_domain::download::Status;
    use udm_domain::id::QueueId;

    const WAIT: Duration = Duration::from_secs(5);

    fn id(n: u64) -> DownloadId {
        DownloadId::new(n)
    }

    /// A daemon with a running download (1), a finished one (2) and two
    /// queues, which answers each request as the real one does.
    fn daemon() -> FakeDaemon {
        let rows = vec![row(1, Status::Downloading), row(2, Status::Done)];
        let queues = vec![Queue::main(), Queue::new(QueueId::new(2), "Night")];
        FakeDaemon::listing(rows, queues, |c| match c {
            Command::Add(d) => vec![Event::Ack { id: id(9), url: d.url.clone(), status: "accepted".into() }],
            Command::Cancel { id } => vec![Event::Canceled { id: *id, url: "https://dl.example/1.zip".into() }],
            Command::Retry { id } => {
                vec![Event::Ack { id: DownloadId::new(id.get() + 1), url: "u".into(), status: "accepted".into() }]
            }
            Command::Remove { id, .. } if id.get() == 1 => {
                vec![Event::Error { error: Failure::Other("it is running".into()) }]
            }
            _ => Vec::new(),
        })
    }

    #[test]
    fn each_tool_runs_its_use_case_on_one_connection() {
        let daemon = daemon();
        let mut assistant = Assistant::new(&daemon, WAIT);
        let Ok(Reply::Listing(listing)) = assistant.run(Request::List) else {
            panic!("no list");
        };
        assert_eq!(listing.downloads.len(), 2);

        let add = AddRequest {
            queue: Some("Night".into()),
            ..AddRequest::new(Address::parse("https://dl.example/new.iso").unwrap())
        };
        let Ok(Reply::Added(added)) = assistant.run(Request::Add(add)) else {
            panic!("not added");
        };
        assert_eq!((added.id, added.queue.map(|q| q.id)), (id(9), Some(QueueId::new(2))));

        assert_eq!(assistant.run(Request::Pause(Which::One(id(1)))), Ok(Reply::Paused(Which::One(id(1)))));
        assert_eq!(assistant.run(Request::Pause(Which::All)), Ok(Reply::Paused(Which::All)));
        assert_eq!(
            assistant.run(Request::Resume(Which::One(id(2)))),
            Ok(Reply::Resumed(Some(Resumed::Retried { old: id(2), new: id(3) })))
        );
        assert_eq!(assistant.run(Request::Resume(Which::All)), Ok(Reply::Resumed(None)));
        assert_eq!(
            assistant.run(Request::Remove { id: id(2), delete_files: false }),
            Ok(Reply::Removed { id: id(2), delete_files: false })
        );
        let started = assistant.run(Request::Queue { name: None, start: true });
        assert!(matches!(started, Ok(Reply::Queue { ref queue, started: true }) if queue.id.get() == 1));
        assert!(matches!(
            assistant.run(Request::Queue { name: Some("night".into()), start: false }),
            Ok(Reply::Queue { started: false, .. })
        ));
        assert_eq!(daemon.connections(), 1);

        let sent = daemon.sent();
        assert!(sent.iter().any(|c| matches!(c, Command::Cancel { id } if id.get() == 1)));
        assert!(sent.iter().any(|c| matches!(c, Command::PauseAll)));
        assert!(sent.iter().any(|c| matches!(c, Command::ResumeAll)));
        assert!(sent.iter().any(|c| matches!(c, Command::StartQueue { id } if id.get() == 1)));
        assert!(sent.iter().any(|c| matches!(c, Command::StopQueue { id } if id.get() == 2)));
    }

    #[test]
    fn nothing_deletes_files_unless_asked() {
        let daemon = daemon();
        let mut assistant = Assistant::new(&daemon, WAIT);
        assistant.run(Request::Remove { id: id(2), delete_files: false }).unwrap();
        assistant.run(Request::Remove { id: id(2), delete_files: true }).unwrap();
        let removes: Vec<bool> = daemon
            .sent()
            .into_iter()
            .filter_map(|c| match c {
                Command::Remove { delete_files, .. } => Some(delete_files),
                _ => None,
            })
            .collect();
        assert_eq!(removes, [false, true]);
    }

    #[test]
    fn a_refusal_is_reported_and_the_connection_kept() {
        let daemon = daemon();
        let mut assistant = Assistant::new(&daemon, WAIT);
        assert_eq!(
            assistant.run(Request::Remove { id: id(1), delete_files: false }),
            Err(RemoteError::Refused(Failure::Other("it is running".into())))
        );
        assert_eq!(assistant.run(Request::Pause(Which::One(id(7)))), Err(RemoteError::NoDownload(id(7))));
        assert_eq!(
            assistant.run(Request::Queue { name: Some("Day".into()), start: true }),
            Err(RemoteError::NoQueue("Day".into()))
        );
        assert!(assistant.run(Request::List).is_ok());
        assert_eq!(daemon.connections(), 1);
    }

    #[test]
    fn a_connection_that_went_between_requests_is_opened_again() {
        let daemon = daemon();
        let mut assistant = Assistant::new(&daemon, WAIT);
        assert!(assistant.run(Request::List).is_ok());
        daemon.tell(Event::Removed { id: id(4) });
        daemon.tell(Event::Lost(Lost::Closed));
        assert!(assistant.run(Request::List).is_ok());
        assert_eq!(daemon.connections(), 2);
    }

    #[test]
    fn a_connection_lost_during_a_request_fails_it_and_the_next_connects_again() {
        let dropped = Arc::new(AtomicBool::new(false));
        let once = Arc::clone(&dropped);
        let daemon = FakeDaemon::new(move |c| match c {
            Command::PauseAll if !once.swap(true, Ordering::SeqCst) => vec![Event::Lost(Lost::Closed)],
            _ => Vec::new(),
        });
        let mut assistant = Assistant::new(&daemon, WAIT);
        assert!(matches!(assistant.run(Request::Pause(Which::All)), Err(RemoteError::Lost(_))));
        assert_eq!(assistant.run(Request::Pause(Which::All)), Ok(Reply::Paused(Which::All)));
        assert_eq!(daemon.connections(), 2);
    }

    #[test]
    fn a_daemon_that_cannot_be_reached_is_tried_again_next_time() {
        let daemon = FakeDaemon::unreachable("no daemon here");
        let mut assistant = Assistant::new(&daemon, WAIT);
        assert_eq!(assistant.run(Request::List), Err(RemoteError::Connect("no daemon here".into())));
        assert_eq!(assistant.run(Request::List), Err(RemoteError::Connect("no daemon here".into())));
    }
}
