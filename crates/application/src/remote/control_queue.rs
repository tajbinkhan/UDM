use udm_domain::queue::{self, Queue, MAIN_QUEUE};

use super::{RemoteError, Session};
use crate::daemon::{Command, DaemonClient};

/// Start or stop a queue, named by its name or id, or the main queue:
/// `udm-cli queue start|stop`.
pub struct ControlQueue<'a, 's, C: DaemonClient + ?Sized> {
    session: &'a Session<'s, C>,
}

impl<'a, 's, C: DaemonClient + ?Sized> ControlQueue<'a, 's, C> {
    pub fn new(session: &'a Session<'s, C>) -> Self {
        ControlQueue { session }
    }

    /// Start (`start`) or stop the queue `name` names, or the main queue
    /// when `None`. Returns the queue as it was.
    pub fn run(&self, name: Option<&str>, start: bool) -> Result<Queue, RemoteError> {
        let queues = self.session.list()?.queues;
        let found = match name {
            Some(name) => queue::named(&queues, name),
            None => queues.iter().find(|q| q.id == MAIN_QUEUE),
        };
        let queue = found.cloned().ok_or_else(|| RemoteError::NoQueue(name.unwrap_or_default().to_string()))?;
        let id = queue.id;
        self.session.ask(if start { Command::StartQueue { id } } else { Command::StopQueue { id } })?;
        Ok(queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::FakeDaemon;
    use std::time::Duration;
    use udm_domain::id::QueueId;

    fn daemon() -> FakeDaemon {
        FakeDaemon::listing(Vec::new(), vec![Queue::main(), Queue::new(QueueId::new(2), "Night")], |_| Vec::new())
    }

    #[test]
    fn a_queue_is_found_by_name_or_id_or_is_the_main_one() {
        let daemon = daemon();
        let session = Session::open(&daemon, Duration::from_secs(5)).unwrap();
        let control = ControlQueue::new(&session);
        assert_eq!(control.run(Some("night"), true).map(|q| q.id), Ok(QueueId::new(2)));
        assert_eq!(control.run(Some("2"), false).map(|q| q.id), Ok(QueueId::new(2)));
        assert_eq!(control.run(None, true).map(|q| q.id), Ok(MAIN_QUEUE));
        let sent = daemon.sent();
        assert!(sent.iter().any(|c| matches!(c, Command::StartQueue { id } if id.get() == 2)));
        assert!(sent.iter().any(|c| matches!(c, Command::StopQueue { id } if id.get() == 2)));
        assert!(sent.iter().any(|c| matches!(c, Command::StartQueue { id } if *id == MAIN_QUEUE)));
        assert_eq!(control.run(Some("Day"), true), Err(RemoteError::NoQueue("Day".into())));
    }
}
