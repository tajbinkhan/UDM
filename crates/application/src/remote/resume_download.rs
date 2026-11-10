use pullbay_domain::download::{Answer, Status};
use pullbay_domain::id::DownloadId;

use super::{RemoteError, Session};
use crate::daemon::{Command, DaemonClient, Event};

/// What resuming a download did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resumed {
    /// It was running already.
    Running(DownloadId),
    /// It waited in a queue or for its save-as dialog, and starts now with
    /// its id.
    Started(DownloadId),
    /// It had stopped, failed or been interrupted, and goes on from its
    /// partial file as a new download with a new id, as Retry does.
    Retried { old: DownloadId, new: DownloadId },
}

/// Start a stopped download again, or everything a pause stopped:
/// `pullbay-cli resume`.
pub struct ResumeDownload<'a, 's, C: DaemonClient + ?Sized> {
    session: &'a Session<'s, C>,
}

impl<'a, 's, C: DaemonClient + ?Sized> ResumeDownload<'a, 's, C> {
    pub fn new(session: &'a Session<'s, C>) -> Self {
        ResumeDownload { session }
    }

    /// Start download `id` again. The daemon refuses one that cannot go on,
    /// such as a finished one.
    pub fn run(&self, id: DownloadId) -> Result<Resumed, RemoteError> {
        match self.session.list()?.download(id)?.status {
            Status::Queued | Status::Downloading => Ok(Resumed::Running(id)),
            Status::Waiting | Status::Confirming => {
                self.session.ask(Command::Confirm(Answer::start(id)))?;
                Ok(Resumed::Started(id))
            }
            Status::Done | Status::Failed | Status::Canceled | Status::Interrupted => {
                let heard = self.session.ask(Command::Retry { id })?;
                heard
                    .into_iter()
                    .find_map(|e| match e {
                        Event::Ack { id: new, .. } => Some(Resumed::Retried { old: id, new }),
                        _ => None,
                    })
                    .ok_or(RemoteError::NoAnswer(self.session.wait().as_secs()))
            }
        }
    }

    /// Start again what a pause of everything stopped, and every
    /// interrupted download.
    pub fn all(&self) -> Result<(), RemoteError> {
        self.session.ask(Command::ResumeAll).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Failure;
    use crate::memory::{row, FakeDaemon};
    use pullbay_domain::queue::Queue;
    use std::time::Duration;

    fn daemon() -> FakeDaemon {
        let rows =
            vec![row(1, Status::Downloading), row(2, Status::Waiting), row(3, Status::Canceled), row(4, Status::Done)];
        FakeDaemon::listing(rows, vec![Queue::main()], |c| match c {
            Command::Retry { id } if id.get() == 3 => {
                vec![Event::Ack { id: DownloadId::new(9), url: "https://dl.example/3.zip".into(), status: "s".into() }]
            }
            Command::Retry { .. } => vec![Event::Error { error: Failure::Other("cannot retry".into()) }],
            _ => Vec::new(),
        })
    }

    fn resume(daemon: &FakeDaemon, id: u64) -> Result<Resumed, RemoteError> {
        let session = Session::open(daemon, Duration::from_secs(5)).unwrap();
        ResumeDownload::new(&session).run(DownloadId::new(id))
    }

    #[test]
    fn each_kind_of_stop_is_resumed_its_own_way() {
        let daemon = daemon();
        assert_eq!(resume(&daemon, 1), Ok(Resumed::Running(DownloadId::new(1))));
        assert_eq!(resume(&daemon, 2), Ok(Resumed::Started(DownloadId::new(2))));
        assert!(daemon.sent().iter().any(|c| matches!(c, Command::Confirm(a) if a.id.get() == 2 && a.start)));
        assert_eq!(resume(&daemon, 3), Ok(Resumed::Retried { old: DownloadId::new(3), new: DownloadId::new(9) }));
    }

    #[test]
    fn the_daemon_decides_what_cannot_go_on() {
        let daemon = daemon();
        assert_eq!(resume(&daemon, 4), Err(RemoteError::Refused(Failure::Other("cannot retry".into()))));
        assert_eq!(resume(&daemon, 5), Err(RemoteError::NoDownload(DownloadId::new(5))));
    }

    #[test]
    fn everything_paused_can_be_resumed_at_once() {
        let daemon = daemon();
        let session = Session::open(&daemon, Duration::from_secs(5)).unwrap();
        assert_eq!(ResumeDownload::new(&session).all(), Ok(()));
        assert!(daemon.sent().iter().any(|c| matches!(c, Command::ResumeAll)));
    }
}
