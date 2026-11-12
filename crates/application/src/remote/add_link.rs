use udm_domain::address::{Address, Link};
use udm_domain::download::Download;
use udm_domain::filename::FileName;
use udm_domain::id::DownloadId;
use udm_domain::queue::{self, Queue};
use udm_domain::settings::Duplicate;

use super::{RemoteError, Session};
use crate::daemon::{Command, DaemonClient, Event};

/// A link to add, as an AI assistant or another program asks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddRequest {
    pub url: Address,
    /// The folder to save in, a full path; missing goes by the settings and
    /// the file's category.
    pub folder: Option<String>,
    /// The name to save it as; missing takes the server's.
    pub filename: Option<FileName>,
    /// The queue to wait in without starting, by its name or id; missing
    /// starts the download now.
    pub queue: Option<String>,
}

impl AddRequest {
    /// Download `url` now, with the settings' folder and the server's name.
    pub fn new(url: Address) -> Self {
        AddRequest { url, folder: None, filename: None, queue: None }
    }

    /// The download the daemon is asked for. It shows no save-as dialog,
    /// whatever the settings say (a folder UDM cannot write to still
    /// asks), and an address already in the list is added again under a new
    /// name, as `udm-cli get` does: nobody is at the window to answer.
    fn download(&self, queue: Option<&Queue>) -> Download {
        Download {
            url: Link::from(self.url.clone()),
            folder: self.folder.clone(),
            filename: self.filename.as_ref().map(|f| f.as_str().to_string()),
            start: Some(queue.is_none()),
            queue: queue.map(|q| q.id),
            ask: Some(false),
            duplicate: Some(Duplicate::NewName),
            ..Download::default()
        }
    }
}

/// What adding a link did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub id: DownloadId,
    /// The queue it waits in, when it was not started.
    pub queue: Option<Queue>,
}

/// Add a link to the list, starting it now or leaving it to wait in a
/// queue, and return once the daemon has taken it: `udm-mcp`'s
/// `add_download`. Unlike `GetFile`, it does not follow the download.
pub struct AddLink<'a, 's, C: DaemonClient + ?Sized> {
    session: &'a Session<'s, C>,
}

impl<'a, 's, C: DaemonClient + ?Sized> AddLink<'a, 's, C> {
    pub fn new(session: &'a Session<'s, C>) -> Self {
        AddLink { session }
    }

    pub fn run(&self, request: &AddRequest) -> Result<Added, RemoteError> {
        let queue = match &request.queue {
            Some(name) => {
                let queues = self.session.list()?.queues;
                Some(queue::named(&queues, name).cloned().ok_or_else(|| RemoteError::NoQueue(name.clone()))?)
            }
            None => None,
        };
        self.session.send(Command::Add(request.download(queue.as_ref())))?;
        // The daemon announces the new download before it answers.
        loop {
            match self.session.answer()? {
                Event::Ack { id, .. } => return Ok(Added { id, queue }),
                Event::Rejected { error, .. } | Event::Error { error } => return Err(RemoteError::Refused(error)),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Failure;
    use crate::memory::FakeDaemon;
    use std::time::Duration;
    use udm_domain::id::QueueId;
    use udm_domain::queue::MAIN_QUEUE;

    const URL: &str = "https://dl.example/a.iso";

    fn daemon() -> FakeDaemon {
        let queues = vec![Queue::main(), Queue::new(QueueId::new(2), "Night")];
        FakeDaemon::listing(Vec::new(), queues, |c| match c {
            Command::Add(d) => vec![
                Event::Added { id: DownloadId::new(5), url: d.url.clone() },
                Event::Ack { id: DownloadId::new(5), url: d.url.clone(), status: "accepted".into() },
            ],
            _ => Vec::new(),
        })
    }

    fn added(daemon: &FakeDaemon) -> Download {
        match daemon.sent().into_iter().find(|c| matches!(c, Command::Add(_))) {
            Some(Command::Add(d)) => d,
            _ => panic!("nothing was added"),
        }
    }

    fn request() -> AddRequest {
        AddRequest::new(Address::parse(URL).unwrap())
    }

    #[test]
    fn a_link_starts_now_with_no_dialog() {
        let daemon = daemon();
        let session = Session::open(&daemon, Duration::from_secs(5)).unwrap();
        let mut request = request();
        request.folder = Some("/dl".into());
        request.filename = Some(FileName::parse("b.iso").unwrap());
        assert_eq!(AddLink::new(&session).run(&request), Ok(Added { id: DownloadId::new(5), queue: None }));
        let d = added(&daemon);
        assert_eq!(d.url.as_str(), URL);
        assert_eq!((d.folder.as_deref(), d.filename.as_deref()), (Some("/dl"), Some("b.iso")));
        assert_eq!((d.start, d.queue, d.ask, d.duplicate), (Some(true), None, Some(false), Some(Duplicate::NewName)));
    }

    #[test]
    fn a_link_for_a_queue_waits_in_it() {
        let daemon = daemon();
        let session = Session::open(&daemon, Duration::from_secs(5)).unwrap();
        let request = AddRequest { queue: Some("night".into()), ..request() };
        let got = AddLink::new(&session).run(&request).unwrap();
        assert_eq!(got.queue.map(|q| q.id), Some(QueueId::new(2)));
        let d = added(&daemon);
        assert_eq!((d.start, d.queue), (Some(false), Some(QueueId::new(2))));

        let request = AddRequest { queue: Some(MAIN_QUEUE.get().to_string()), ..self::request() };
        assert_eq!(AddLink::new(&session).run(&request).unwrap().queue.map(|q| q.id), Some(MAIN_QUEUE));
    }

    #[test]
    fn an_unknown_queue_adds_nothing() {
        let daemon = daemon();
        let session = Session::open(&daemon, Duration::from_secs(5)).unwrap();
        let request = AddRequest { queue: Some("Day".into()), ..request() };
        assert_eq!(AddLink::new(&session).run(&request), Err(RemoteError::NoQueue("Day".into())));
        assert!(!daemon.sent().iter().any(|c| matches!(c, Command::Add(_))));
    }

    #[test]
    fn a_refused_link_says_why() {
        let daemon = FakeDaemon::new(|c| match c {
            Command::Add(d) => vec![Event::Rejected { url: d.url.clone(), error: Failure::Other("no".into()) }],
            _ => Vec::new(),
        });
        let session = Session::open(&daemon, Duration::from_secs(5)).unwrap();
        assert_eq!(AddLink::new(&session).run(&request()), Err(RemoteError::Refused(Failure::Other("no".into()))));
    }
}
