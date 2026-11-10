use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use pullbay_domain::address::Address;
use pullbay_domain::id::{DeviceId, DownloadId};
use pullbay_domain::pairing::{Credential, PairingError};
use tracing::info;

use super::{LocalClient, PairingDesk};
use crate::daemon::{ClientId, Requests};
use crate::remote::{
    AddLink, AddRequest, ForgetDownload, ListDownloads, Listing, PauseDownload, RemoteError, ResumeDownload, Session,
};

/// How long the page waits for the daemon's answer.
const WAIT: Duration = Duration::from_secs(10);

/// What a paired phone may ask: the list, a link to add, and pausing,
/// resuming and removing one download. Nothing else of the daemon's
/// commands is reachable from the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhoneRequest {
    List,
    /// Download a link now, into the folder the settings choose, with the
    /// server's name.
    Add(Address),
    Pause(DownloadId),
    Resume(DownloadId),
    /// Take a download out of the list; its files stay unless
    /// `delete_files`, which the page asks for only when the user ticks it.
    Remove {
        id: DownloadId,
        delete_files: bool,
    },
    /// Forget this phone: its credential stops working.
    Unpair,
}

impl PhoneRequest {
    /// Its name in the log.
    fn name(&self) -> &'static str {
        match self {
            PhoneRequest::List => "list",
            PhoneRequest::Add(_) => "add",
            PhoneRequest::Pause(_) => "pause",
            PhoneRequest::Resume(_) => "resume",
            PhoneRequest::Remove { .. } => "remove",
            PhoneRequest::Unpair => "unpair",
        }
    }
}

/// What a phone's request did.
#[derive(Debug, Clone, PartialEq)]
pub enum PhoneReply {
    Listing(Listing),
    /// The link was added as this download.
    Added(DownloadId),
    Done,
}

/// Why a phone's request was not done.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PhoneError {
    #[error(transparent)]
    Pairing(#[from] PairingError),
    #[error(transparent)]
    Remote(#[from] RemoteError),
}

/// The port the remote-control page's server drives, as the daemon's
/// server drives `Requests`.
pub trait PhoneGate: Send + Sync {
    /// Pair the device at `peer` that sent the code shown.
    fn pair(&self, code: &str, name: &str, peer: IpAddr) -> Result<Credential, PairingError>;

    /// Do `request` for the paired device `credential` names.
    fn run(&self, credential: &str, peer: IpAddr, request: PhoneRequest) -> Result<PhoneReply, PhoneError>;
}

/// The page's requests, each checked against the paired devices and then
/// run through the daemon's own commands on a `LocalClient`.
pub struct Phone {
    desk: Arc<PairingDesk>,
    requests: Arc<dyn Requests>,
    /// A client id no other client has, for each request's connection.
    next_client: Box<dyn Fn() -> ClientId + Send + Sync>,
}

impl Phone {
    pub fn new(
        desk: Arc<PairingDesk>,
        requests: Arc<dyn Requests>,
        next_client: Box<dyn Fn() -> ClientId + Send + Sync>,
    ) -> Self {
        Phone { desk, requests, next_client }
    }
}

impl PhoneGate for Phone {
    fn pair(&self, code: &str, name: &str, peer: IpAddr) -> Result<Credential, PairingError> {
        self.desk.pair(code, name, peer)
    }

    fn run(&self, credential: &str, peer: IpAddr, request: PhoneRequest) -> Result<PhoneReply, PhoneError> {
        let device: DeviceId = self.desk.check(credential, peer)?;
        if !matches!(request, PhoneRequest::List) {
            info!(device_id = device.get(), action = request.name(), "remote-control request");
        }
        if request == PhoneRequest::Unpair {
            self.desk.remove(device)?;
            return Ok(PhoneReply::Done);
        }
        let client = LocalClient::new(Arc::clone(&self.requests), (self.next_client)());
        let session = Session::open(&client, WAIT)?;
        Ok(match request {
            PhoneRequest::List => PhoneReply::Listing(ListDownloads::new(&session).run()?),
            PhoneRequest::Add(url) => PhoneReply::Added(AddLink::new(&session).run(&AddRequest::new(url))?.id),
            // The page's connection hears no announcements, so it does not
            // wait for the download to stop; the list shows it.
            PhoneRequest::Pause(id) => PauseDownload::new(&session).ask(id).map(|_| PhoneReply::Done)?,
            PhoneRequest::Resume(id) => ResumeDownload::new(&session).run(id).map(|_| PhoneReply::Done)?,
            PhoneRequest::Remove { id, delete_files } => {
                ForgetDownload::new(&session).run(id, delete_files).map(|()| PhoneReply::Done)?
            }
            PhoneRequest::Unpair => PhoneReply::Done,
        })
    }
}
