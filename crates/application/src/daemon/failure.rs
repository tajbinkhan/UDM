//! Why the daemon could not do what a client asked, as the client hears it:
//! the error's kind and values, so the window can say it in the user's
//! language. Each message reads as the daemon's English text did before.

use pullbay_domain::download::{DownloadError, Mismatch};
use pullbay_domain::ports::{ProbeError, SettingsError};
use pullbay_domain::queue::QueueError;
use pullbay_domain::update::UpdateError;

use super::service::ServiceError;

/// A failure the daemon tells a client about.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Failure {
    /// The service refused a command.
    #[error(transparent)]
    Service(#[from] ServiceError),
    /// New settings the daemon would not keep.
    #[error(transparent)]
    Settings(#[from] SettingsError),
    /// A new link is not for the file a partial download came from.
    #[error(transparent)]
    Mismatch(#[from] Mismatch),
    /// The daemon could not read a client's message; the parser's complaint.
    #[error("invalid message: {0}")]
    InvalidMessage(String),
    /// A server could not be asked, or what it answered could not be read,
    /// in the probe's own words (a server's or the network's text).
    #[error("{0}")]
    Probe(String),
    /// A failure in the daemon's own words only: from a daemon too old to
    /// send its kind, or of a kind this client does not know.
    #[error("{0}")]
    Other(String),
}

impl From<DownloadError> for Failure {
    fn from(e: DownloadError) -> Self {
        Failure::Service(e.into())
    }
}

impl From<QueueError> for Failure {
    fn from(e: QueueError) -> Self {
        Failure::Service(e.into())
    }
}

impl From<UpdateError> for Failure {
    fn from(e: UpdateError) -> Self {
        Failure::Service(e.into())
    }
}

impl From<pullbay_domain::tools::ToolError> for Failure {
    fn from(e: pullbay_domain::tools::ToolError) -> Self {
        Failure::Service(e.into())
    }
}

impl From<pullbay_domain::gallery::GalleryError> for Failure {
    fn from(e: pullbay_domain::gallery::GalleryError) -> Self {
        Failure::Service(e.into())
    }
}

impl From<ProbeError> for Failure {
    fn from(e: ProbeError) -> Self {
        Failure::Probe(e.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pullbay_domain::bytes::ByteCount;
    use pullbay_domain::id::QueueId;
    use pullbay_domain::settings::InvalidSettings;

    /// Clients that read only the text hear what they heard before.
    #[test]
    fn messages_read_as_the_errors_they_carry() {
        let queue = Failure::from(QueueError::NoQueue(QueueId::new(7)));
        assert_eq!(queue, Failure::Service(ServiceError::Queue(QueueError::NoQueue(QueueId::new(7)))));
        assert_eq!(queue.to_string(), "no queue with id 7");
        let update = Failure::from(UpdateError::NewestAlready);
        assert_eq!(update, Failure::Service(ServiceError::Update(UpdateError::NewestAlready)));
        assert_eq!(update.to_string(), "This is the newest version.");
        let settings = Failure::from(SettingsError::Invalid(InvalidSettings::QuotaZero));
        assert_eq!(settings.to_string(), "The quota must be at least 1 MB.");
        assert_eq!(Failure::InvalidMessage("eof".into()).to_string(), "invalid message: eof");
        assert_eq!(Failure::from(ProbeError("HTTP 404".into())), Failure::Probe("HTTP 404".into()));
        assert_eq!(Failure::Other("as sent".into()).to_string(), "as sent");
        let size = Failure::from(Mismatch::Size {
            new: ByteCount::new(2 * 1024 * 1024),
            partial: ByteCount::new(1024 * 1024),
        });
        assert_eq!(size.to_string(), "The new link is for a 2.0 MB file; the partial download is of a 1.0 MB file.");
    }
}
