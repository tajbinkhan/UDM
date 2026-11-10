use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, TryRecvError};
use std::time::Duration;

use pullbay_domain::download::DownloadEntry;
use pullbay_domain::id::DownloadId;
use pullbay_domain::queue::Queue;

use crate::daemon::{Command, DaemonClient, Event, Failure, Lost};

/// Why a request from another program did not get done.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoteError {
    /// The daemon could not be reached or started, and why.
    #[error("could not reach Pullbay's daemon: {0}")]
    Connect(String),
    /// The connection went before the answer came, and why.
    #[error("lost the connection to Pullbay's daemon: {0}")]
    Lost(String),
    /// Nothing answered within this many seconds.
    #[error("Pullbay's daemon did not answer within {0} seconds")]
    NoAnswer(u64),
    /// The daemon refused the request.
    #[error(transparent)]
    Refused(Failure),
    #[error("there is no download {0}")]
    NoDownload(DownloadId),
    /// The queue as it was named.
    #[error("there is no queue named {0}")]
    NoQueue(String),
    #[error("download {0} is not running")]
    NotRunning(DownloadId),
}

impl RemoteError {
    /// Whether the daemon was never reached, or went: the request may not
    /// have arrived at all.
    pub fn unreachable(&self) -> bool {
        matches!(self, RemoteError::Connect(_) | RemoteError::Lost(_) | RemoteError::NoAnswer(_))
    }
}

fn lost(why: Lost) -> RemoteError {
    match why {
        Lost::Closed => RemoteError::Lost("the daemon closed the connection".into()),
        Lost::Failed(why) => RemoteError::Lost(why),
    }
}

/// Every download and queue, as the daemon lists them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    /// Oldest first.
    pub downloads: Vec<DownloadEntry>,
    /// The main queue first.
    pub queues: Vec<Queue>,
}

impl Listing {
    /// Download `id`'s entry.
    pub fn download(&self, id: DownloadId) -> Result<&DownloadEntry, RemoteError> {
        self.downloads.iter().find(|e| e.id == id).ok_or(RemoteError::NoDownload(id))
    }
}

/// One program's connection to the daemon through the `DaemonClient` port,
/// as a stream of events it reads in turn. Opening it starts the daemon
/// when none runs, as the port does.
pub struct Session<'a, C: DaemonClient + ?Sized> {
    client: &'a C,
    events: Receiver<Event>,
    /// How long an answer may take.
    wait: Duration,
}

impl<'a, C: DaemonClient + ?Sized> Session<'a, C> {
    /// Connect through `client`, waiting up to `wait` for the connection
    /// and for each answer after it.
    pub fn open(client: &'a C, wait: Duration) -> Result<Self, RemoteError> {
        let (tx, events) = channel();
        client.connect(
            false,
            Box::new(move |event| {
                // The session is gone once nobody reads; nothing is lost then.
                let _ = tx.send(event);
            }),
        );
        let session = Session { client, events, wait };
        loop {
            match session.events.recv_timeout(wait) {
                Ok(Event::Connected) => return Ok(session),
                Ok(Event::Lost(Lost::Failed(why))) => return Err(RemoteError::Connect(why)),
                Ok(Event::Lost(Lost::Closed)) | Err(RecvTimeoutError::Disconnected) => {
                    return Err(RemoteError::Connect("the daemon closed the connection".into()))
                }
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => return Err(RemoteError::NoAnswer(wait.as_secs())),
            }
        }
    }

    /// How long an answer may take.
    pub fn wait(&self) -> Duration {
        self.wait
    }

    /// Send `command`.
    pub fn send(&self, command: Command) -> Result<(), RemoteError> {
        if self.client.send(command) {
            Ok(())
        } else {
            Err(RemoteError::Lost("the request could not be sent".into()))
        }
    }

    /// The next event, however long it takes: for following a download.
    pub fn next(&self) -> Result<Event, RemoteError> {
        match self.events.recv() {
            Ok(Event::Lost(why)) => Err(lost(why)),
            Ok(event) => Ok(event),
            Err(_) => Err(RemoteError::Lost("the connection ended".into())),
        }
    }

    /// The next event, if it comes within the wait: for an answer.
    pub fn answer(&self) -> Result<Event, RemoteError> {
        match self.events.recv_timeout(self.wait) {
            Ok(Event::Lost(why)) => Err(lost(why)),
            Ok(event) => Ok(event),
            Err(RecvTimeoutError::Timeout) => Err(RemoteError::NoAnswer(self.wait.as_secs())),
            Err(RecvTimeoutError::Disconnected) => Err(RemoteError::Lost("the connection ended".into())),
        }
    }

    /// Skip what was heard since the last request, which nobody waits for
    /// any more, for a session that stays open between requests. Fails when
    /// the connection went meanwhile; nothing was sent on it then.
    pub fn catch_up(&self) -> Result<(), RemoteError> {
        loop {
            match self.events.try_recv() {
                Ok(Event::Lost(why)) => return Err(lost(why)),
                Ok(_) => {}
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => return Err(RemoteError::Lost("the connection ended".into())),
            }
        }
    }

    /// Send `command` and wait until the daemon has handled it, which a
    /// `Ping` after it shows: the daemon reads a client's requests in
    /// order. Returns what was heard meanwhile, or the daemon's refusal.
    pub fn ask(&self, command: Command) -> Result<Vec<Event>, RemoteError> {
        self.send(command)?;
        self.send(Command::Ping)?;
        let (mut heard, mut refused) = (Vec::new(), None);
        loop {
            match self.answer()? {
                Event::Pong => break,
                // The first refusal is the command's; read on to the pong, so
                // it does not end the next request's wait.
                Event::Error { error } => {
                    refused.get_or_insert(error);
                }
                event => heard.push(event),
            }
        }
        match refused {
            Some(error) => Err(RemoteError::Refused(error)),
            None => Ok(heard),
        }
    }

    /// Every download and queue.
    pub fn list(&self) -> Result<Listing, RemoteError> {
        let mut listing = None;
        let mut queues = Vec::new();
        for event in self.ask(Command::List)? {
            match event {
                Event::Downloads { items } => listing = Some(items),
                Event::Queues { items } => queues = items,
                _ => {}
            }
        }
        // The answer to `List` always holds the downloads; without them the
        // daemon did not answer it.
        let downloads = listing.ok_or(RemoteError::NoAnswer(self.wait.as_secs()))?;
        Ok(Listing { downloads, queues })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::FakeDaemon;
    use pullbay_domain::download::Status;
    use pullbay_domain::queue::MAIN_QUEUE;

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn a_daemon_that_cannot_be_reached_says_why() {
        let daemon = FakeDaemon::unreachable("no daemon here");
        let error = Session::open(&daemon, WAIT).err();
        assert_eq!(error, Some(RemoteError::Connect("no daemon here".into())));
        assert!(error.is_some_and(|e| e.unreachable()));
    }

    #[test]
    fn a_request_is_done_once_the_ping_after_it_is_answered() {
        let daemon = FakeDaemon::new(|c| match c {
            Command::PauseAll => vec![Event::Canceled { id: DownloadId::new(1), url: "http://x/a".into() }],
            _ => Vec::new(),
        });
        let session = Session::open(&daemon, WAIT).unwrap();
        let heard = session.ask(Command::PauseAll).unwrap();
        assert_eq!(heard, [Event::Canceled { id: DownloadId::new(1), url: "http://x/a".into() }]);
        assert!(matches!(daemon.sent().as_slice(), [Command::PauseAll, Command::Ping]));
    }

    #[test]
    fn a_refusal_is_the_requests_and_the_next_request_still_waits_for_its_own_answer() {
        let daemon = FakeDaemon::new(|c| match c {
            Command::StartQueue { .. } => vec![Event::Error { error: Failure::Other("no".into()) }],
            _ => Vec::new(),
        });
        let session = Session::open(&daemon, WAIT).unwrap();
        let refused = session.ask(Command::StartQueue { id: MAIN_QUEUE });
        assert_eq!(refused, Err(RemoteError::Refused(Failure::Other("no".into()))));
        assert_eq!(session.ask(Command::ResumeAll), Ok(Vec::new()));
    }

    #[test]
    fn the_list_is_the_downloads_and_the_queues() {
        let daemon = FakeDaemon::with_list(vec![crate::memory::row(4, Status::Done)]);
        let session = Session::open(&daemon, WAIT).unwrap();
        let listing = session.list().unwrap();
        assert_eq!(listing.downloads.len(), 1);
        assert_eq!(listing.queues[0].id, MAIN_QUEUE);
        assert!(listing.download(DownloadId::new(4)).is_ok());
        assert_eq!(listing.download(DownloadId::new(5)), Err(RemoteError::NoDownload(DownloadId::new(5))));
    }

    #[test]
    fn catching_up_skips_what_nobody_asked_for_and_notices_a_lost_connection() {
        let daemon = FakeDaemon::with_list(Vec::new());
        let session = Session::open(&daemon, WAIT).unwrap();
        daemon.tell(Event::Removed { id: DownloadId::new(3) });
        assert_eq!(session.catch_up(), Ok(()));
        assert!(session.list().is_ok());
        daemon.tell(Event::Lost(Lost::Closed));
        assert!(matches!(session.catch_up(), Err(RemoteError::Lost(_))));
    }

    #[test]
    fn a_lost_connection_ends_the_wait() {
        let daemon = FakeDaemon::new(|c| match c {
            Command::List => vec![Event::Lost(Lost::Closed)],
            _ => Vec::new(),
        });
        let session = Session::open(&daemon, WAIT).unwrap();
        assert!(matches!(session.list(), Err(RemoteError::Lost(_))));
    }
}
