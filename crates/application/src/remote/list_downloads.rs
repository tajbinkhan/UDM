use super::{Listing, RemoteError, Session};
use crate::daemon::DaemonClient;

/// Every download and queue the daemon has: `pullbay-cli list`.
pub struct ListDownloads<'a, 's, C: DaemonClient + ?Sized> {
    session: &'a Session<'s, C>,
}

impl<'a, 's, C: DaemonClient + ?Sized> ListDownloads<'a, 's, C> {
    pub fn new(session: &'a Session<'s, C>) -> Self {
        ListDownloads { session }
    }

    pub fn run(&self) -> Result<Listing, RemoteError> {
        self.session.list()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, FakeDaemon};
    use pullbay_domain::download::Status;
    use std::time::Duration;

    #[test]
    fn it_lists_what_the_daemon_has() {
        let daemon = FakeDaemon::with_list(vec![row(1, Status::Done), row(2, Status::Failed)]);
        let session = Session::open(&daemon, Duration::from_secs(5)).unwrap();
        let listing = ListDownloads::new(&session).run().unwrap();
        let statuses: Vec<_> = listing.downloads.iter().map(|e| e.status).collect();
        assert_eq!(statuses, [Status::Done, Status::Failed]);
        assert_eq!(listing.queues.len(), 1);
    }
}
