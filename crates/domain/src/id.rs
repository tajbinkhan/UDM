//! The ids that name downloads, queues and paired devices. Each wraps the `u64` the wire
//! and the JSON files carry, and serializes as that number
//! (`serde(transparent)`), so the files and the messages keep their shape.
//! Any number is a valid id: the daemon hands them out, and an id that names
//! nothing is found by looking it up, not by its value.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A download in the list. The engine hands them out in rising order,
/// starting from 1; 0 never names a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DownloadId(u64);

impl DownloadId {
    /// The id no download has (the engine starts at 1), for a run outside
    /// the list or a slot whose download is gone.
    pub const NONE: DownloadId = DownloadId(0);

    /// The download numbered `id`.
    pub const fn new(id: u64) -> Self {
        DownloadId(id)
    }

    /// Its number, as the wire and the files carry it.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The id after this one, for handing out ids in order.
    pub const fn next(self) -> Self {
        DownloadId(self.0.saturating_add(1))
    }
}

impl From<u64> for DownloadId {
    fn from(id: u64) -> Self {
        DownloadId(id)
    }
}

impl From<DownloadId> for u64 {
    fn from(id: DownloadId) -> Self {
        id.0
    }
}

impl fmt::Display for DownloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A queue. The main queue is `QueueId::MAIN` (1); a queue being created
/// is sent as `QueueId::NEW` (0) until the daemon gives it an id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QueueId(u64);

impl QueueId {
    /// The main queue, which every download without another queue waits in.
    pub const MAIN: QueueId = QueueId(1);

    /// A queue not saved yet: saving it gives it a new id.
    pub const NEW: QueueId = QueueId(0);

    /// The queue numbered `id`.
    pub const fn new(id: u64) -> Self {
        QueueId(id)
    }

    /// Its number, as the wire and the files carry it.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The id after this one, for giving a new queue an id above every
    /// other.
    pub const fn next(self) -> Self {
        QueueId(self.0.saturating_add(1))
    }
}

impl From<u64> for QueueId {
    fn from(id: u64) -> Self {
        QueueId(id)
    }
}

impl From<QueueId> for u64 {
    fn from(id: QueueId) -> Self {
        id.0
    }
}

impl fmt::Display for QueueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A phone or other browser paired with the remote-control page
/// (`pairing`). The daemon hands them out in rising order from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(u64);

impl DeviceId {
    /// The device numbered `id`.
    pub const fn new(id: u64) -> Self {
        DeviceId(id)
    }

    /// Its number, as the wire and the files carry it.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The id after this one.
    pub const fn next(self) -> Self {
        DeviceId(self.0.saturating_add(1))
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_plain_numbers_in_json() {
        assert_eq!(serde_json::to_string(&DownloadId::new(42)).unwrap(), "42");
        assert_eq!(serde_json::from_str::<DownloadId>("42").unwrap(), DownloadId::new(42));
        assert_eq!(serde_json::to_string(&Some(QueueId::MAIN)).unwrap(), "1");
        assert_eq!(serde_json::from_str::<Vec<QueueId>>("[0,7]").unwrap(), vec![QueueId::NEW, QueueId::new(7)]);
    }

    #[test]
    fn ids_show_as_their_number() {
        assert_eq!(DownloadId::new(7).to_string(), "7");
        assert_eq!(QueueId::MAIN.to_string(), "1");
        assert_eq!(DownloadId::new(7).next(), DownloadId::new(8));
        assert_eq!(QueueId::MAIN.next().get(), 2);
    }
}
