use pullbay_domain::id::DownloadId;

use super::{RemoteError, Session};
use crate::daemon::{Command, DaemonClient};

/// Take a download out of the list, and with `delete_files` its files to
/// the Recycle Bin or trash: `pullbay-cli remove`.
pub struct ForgetDownload<'a, 's, C: DaemonClient + ?Sized> {
    session: &'a Session<'s, C>,
}

impl<'a, 's, C: DaemonClient + ?Sized> ForgetDownload<'a, 's, C> {
    pub fn new(session: &'a Session<'s, C>) -> Self {
        ForgetDownload { session }
    }

    /// Forget download `id`. The daemon refuses while it runs.
    pub fn run(&self, id: DownloadId, delete_files: bool) -> Result<(), RemoteError> {
        self.session.list()?.download(id)?;
        self.session.ask(Command::Remove { id, delete_files }).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::{Event, Failure};
    use crate::memory::{row, FakeDaemon};
    use pullbay_domain::download::Status;
    use pullbay_domain::queue::Queue;
    use std::time::Duration;

    #[test]
    fn a_download_is_forgotten_unless_the_daemon_refuses() {
        let daemon =
            FakeDaemon::listing(vec![row(1, Status::Done), row(2, Status::Downloading)], vec![Queue::main()], |c| {
                match c {
                    Command::Remove { id, .. } if id.get() == 2 => {
                        vec![Event::Error { error: Failure::Other("still running".into()) }]
                    }
                    _ => Vec::new(),
                }
            });
        let session = Session::open(&daemon, Duration::from_secs(5)).unwrap();
        let forget = ForgetDownload::new(&session);
        assert_eq!(forget.run(DownloadId::new(1), true), Ok(()));
        assert!(daemon.sent().iter().any(|c| matches!(c, Command::Remove { id, delete_files: true } if id.get() == 1)));
        assert_eq!(
            forget.run(DownloadId::new(2), false),
            Err(RemoteError::Refused(Failure::Other("still running".into())))
        );
        assert_eq!(forget.run(DownloadId::new(3), false), Err(RemoteError::NoDownload(DownloadId::new(3))));
    }
}
