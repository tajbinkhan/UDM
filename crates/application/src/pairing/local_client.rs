use std::sync::{Arc, Mutex};

use udm_domain::download::CommandLine;
use udm_domain::sync::LockExt;

use crate::daemon::{ClientId, Command, DaemonClient, DaemonError, Event, Failure, Reply, Requests};

/// A connection to the daemon from inside it: each command goes straight
/// to the service's `Requests` port, as a client's frames do, and its
/// answers come back as events. The remote-control page runs the `remote`
/// use cases on one, so it asks the daemon what `udm-cli` would,
/// without a socket or the daemon's token.
pub struct LocalClient {
    requests: Arc<dyn Requests>,
    id: ClientId,
    reply: Mutex<Option<Reply>>,
}

impl LocalClient {
    /// A connection as client `id`, which no other client may have.
    pub fn new(requests: Arc<dyn Requests>, id: ClientId) -> Self {
        LocalClient { requests, id, reply: Mutex::new(None) }
    }
}

impl DaemonClient for LocalClient {
    fn connect(&self, _restart: bool, on_event: Box<dyn FnMut(Event) + Send>) {
        // The events of one connection come in turn; the lock only makes
        // the callback shareable, and a panic in it leaves nothing half done.
        let on_event = Mutex::new(on_event);
        let reply = Reply::new(move |event| {
            (on_event.lock_or_recover())(event);
            true
        });
        *self.reply.lock_or_recover() = Some(reply.clone());
        reply.send(Event::Connected);
    }

    fn send(&self, command: Command) -> bool {
        let Some(reply) = self.reply.lock_or_recover().clone() else {
            return false;
        };
        self.requests.handle(self.id, command, &reply);
        true
    }

    fn run_command_line(&self, _command: CommandLine) -> Result<bool, DaemonError> {
        Err(DaemonError::Refused(Failure::Other("a connection inside the daemon runs no command lines".into())))
    }
}

impl Drop for LocalClient {
    fn drop(&mut self) {
        self.requests.gone(self.id);
    }
}
