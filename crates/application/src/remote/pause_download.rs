use pullbay_domain::download::Status;
use pullbay_domain::id::DownloadId;

use super::{RemoteError, Session};
use crate::daemon::{Command, DaemonClient, Event};

/// Stop a running download, keeping its partial file so it resumes later,
/// or every one: `pullbay-cli pause`.
pub struct PauseDownload<'a, 's, C: DaemonClient + ?Sized> {
    session: &'a Session<'s, C>,
}

impl<'a, 's, C: DaemonClient + ?Sized> PauseDownload<'a, 's, C> {
    pub fn new(session: &'a Session<'s, C>) -> Self {
        PauseDownload { session }
    }

    /// Stop download `id`, and wait until it has stopped. A download
    /// waiting for its save-as dialog is stopped too.
    pub fn run(&self, id: DownloadId) -> Result<(), RemoteError> {
        let status = self.session.list()?.download(id)?.status;
        if !(status.is_active() || status == Status::Confirming) {
            return Err(RemoteError::NotRunning(id));
        }
        if self.session.ask(Command::Cancel { id })?.iter().any(|e| stopped(e, id)) {
            return Ok(());
        }
        // The engine stops it in its own time.
        while !stopped(&self.session.answer()?, id) {}
        Ok(())
    }

    /// Stop every started queue and every running download, as the
    /// window's Stop all does; `ResumeDownload::all` starts them again.
    pub fn all(&self) -> Result<(), RemoteError> {
        self.session.ask(Command::PauseAll).map(drop)
    }
}

/// Whether `event` says download `id` stopped running.
fn stopped(event: &Event, id: DownloadId) -> bool {
    match event {
        Event::Canceled { id: i, .. } | Event::Failed { id: i, .. } | Event::Done { id: i, .. } => *i == id,
        Event::Removed { id: i } => *i == id,
        Event::Updated { item } => item.id == id && !item.status.is_active() && item.status != Status::Confirming,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, FakeDaemon};
    use pullbay_domain::queue::Queue;
    use std::time::Duration;

    fn session_on(daemon: &FakeDaemon) -> Session<'_, FakeDaemon> {
        Session::open(daemon, Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn a_running_download_is_stopped_and_waited_for() {
        let daemon = FakeDaemon::listing(vec![row(3, Status::Downloading)], vec![Queue::main()], |c| match c {
            Command::Cancel { id } => vec![Event::Canceled { id: *id, url: "https://dl.example/3.zip".into() }],
            _ => Vec::new(),
        });
        assert_eq!(PauseDownload::new(&session_on(&daemon)).run(DownloadId::new(3)), Ok(()));
        assert!(daemon.sent().iter().any(|c| matches!(c, Command::Cancel { id } if id.get() == 3)));
    }

    #[test]
    fn only_a_running_download_can_be_paused() {
        let daemon = FakeDaemon::with_list(vec![row(3, Status::Done)]);
        let session = session_on(&daemon);
        assert_eq!(
            PauseDownload::new(&session).run(DownloadId::new(3)),
            Err(RemoteError::NotRunning(DownloadId::new(3)))
        );
        assert_eq!(
            PauseDownload::new(&session).run(DownloadId::new(4)),
            Err(RemoteError::NoDownload(DownloadId::new(4)))
        );
        assert!(!daemon.sent().iter().any(|c| matches!(c, Command::Cancel { .. })));
    }

    #[test]
    fn a_download_that_never_stops_is_not_waited_for_forever() {
        let daemon = FakeDaemon::with_list(vec![row(3, Status::Queued)]);
        let session = Session::open(&daemon, Duration::from_millis(50)).unwrap();
        assert!(matches!(PauseDownload::new(&session).run(DownloadId::new(3)), Err(RemoteError::NoAnswer(_))));
    }

    #[test]
    fn everything_can_be_paused_at_once() {
        let daemon = FakeDaemon::with_list(Vec::new());
        assert_eq!(PauseDownload::new(&session_on(&daemon)).all(), Ok(()));
        assert!(matches!(daemon.sent().as_slice(), [Command::PauseAll, Command::Ping]));
    }
}
