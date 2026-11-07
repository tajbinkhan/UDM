//! The daemon's download list, kept current by applying its wire messages.
//!
//! The daemon applies every event it broadcasts, so it can answer `list`. How
//! an event changes the list is `uidm_domain::table`'s; this module maps the
//! wire messages onto those changes.

use std::ops::{Deref, DerefMut};

use crate::protocol::Outgoing;

use uidm_domain::table::Change;

/// The download list (`uidm_domain::table::DownloadTable`), fed with the
/// daemon's wire messages.
#[derive(Debug, Default, Clone)]
pub struct DownloadTable(uidm_domain::table::DownloadTable);

impl DownloadTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one daemon message into the table. Messages that do not describe a
    /// download (`hello`, `pong`, `ack`, `error`) are ignored.
    pub fn apply(&mut self, msg: &Outgoing) {
        if let Some(change) = change_of(msg) {
            self.0.change(change, unix_now());
        }
    }
}

impl Deref for DownloadTable {
    type Target = uidm_domain::table::DownloadTable;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DownloadTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// The change `msg` makes to the download list, if it makes one.
fn change_of(msg: &Outgoing) -> Option<Change<'_>> {
    Some(match msg {
        Outgoing::Added { id, url } => Change::Added { id: *id, url },
        Outgoing::Started { id, url, path, total, resumable, connections, partial, validator } => Change::Started {
            id: *id,
            url,
            path,
            total: *total,
            resumable: *resumable,
            connections: *connections,
            partial: partial.as_deref(),
            validator: validator.as_ref(),
        },
        Outgoing::Progress { id, url, done, total, parts, .. } => {
            Change::Progress { id: *id, url, done: *done, total: *total, parts: *parts }
        }
        Outgoing::Done { id, url, path, note } => Change::Done { id: *id, url, path, note: note.as_deref() },
        Outgoing::Failed { id, url, error } => Change::Failed { id: *id, url, error },
        Outgoing::Canceled { id, url } => Change::Canceled { id: *id, url },
        Outgoing::NameTaken { id, url, path, total } => Change::NameTaken { id: *id, url, path, total: *total },
        Outgoing::Removed { id } => Change::Removed { id: *id },
        Outgoing::Updated { item } => Change::Updated(item),
        Outgoing::Downloads { items } => Change::Downloads(items),
        Outgoing::Hello { .. }
        | Outgoing::Pong
        | Outgoing::Ack { .. }
        | Outgoing::Error { .. }
        | Outgoing::CommandLine { .. }
        | Outgoing::Rejected { .. }
        | Outgoing::Settings { .. }
        | Outgoing::Show
        | Outgoing::ShowDone { .. }
        | Outgoing::Queues { .. }
        | Outgoing::Countdown { .. }
        | Outgoing::CountdownCanceled
        | Outgoing::Quit
        | Outgoing::RefreshWaiting { .. }
        | Outgoing::RefreshMismatch { .. }
        | Outgoing::AddedMany { .. }
        | Outgoing::MediaInfo { .. }
        | Outgoing::LinkInfo { .. }
        | Outgoing::PickLinks { .. }
        | Outgoing::Quota { .. }
        | Outgoing::BrowserLink { .. }
        | Outgoing::CookiesWanted { .. }
        | Outgoing::RenderPage { .. }
        | Outgoing::ZipPreview { .. }
        | Outgoing::Update { .. }
        | Outgoing::UpdateFailed { .. }
        | Outgoing::GrabProgress { .. }
        | Outgoing::GrabDone { .. } => return None,
    })
}

/// Seconds since 1970, by this computer's clock.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uidm_domain::download::Status;

    fn started(id: u64) -> Outgoing {
        Outgoing::Started {
            id,
            url: "https://x/f.zip".into(),
            path: "C:\\dl\\f.zip".into(),
            total: Some(1000),
            resumable: true,
            connections: 4,
            partial: None,
            validator: None,
        }
    }

    fn progress(id: u64, done: u64) -> Outgoing {
        Outgoing::Progress { id, url: "https://x/f.zip".into(), done, total: Some(1000), parts: None, segments: None }
    }

    #[test]
    fn full_lifecycle() {
        let mut t = DownloadTable::new();
        t.apply(&Outgoing::Added { id: 1, url: "https://x/f.zip".into() });
        assert_eq!(t.get(1).unwrap().status, Status::Queued);

        t.apply(&started(1));
        let e = t.get(1).unwrap();
        assert_eq!(e.status, Status::Downloading);
        assert_eq!(e.path.as_deref(), Some("C:\\dl\\f.zip"));
        assert_eq!(e.connections, 4);

        t.apply(&progress(1, 400));
        assert_eq!(t.get(1).unwrap().done, 400);

        t.apply(&Outgoing::Done { id: 1, url: "https://x/f.zip".into(), path: "C:\\dl\\f.zip".into(), note: None });
        let e = t.get(1).unwrap();
        assert_eq!(e.status, Status::Done);
        assert_eq!(e.done, 1000, "done fills to the total");
    }

    #[test]
    fn started_before_added_is_not_reset() {
        // The engine can report `started` before the daemon applies `added`.
        let mut t = DownloadTable::new();
        t.apply(&started(7));
        t.apply(&Outgoing::Added { id: 7, url: "https://x/f.zip".into() });
        assert_eq!(t.get(7).unwrap().status, Status::Downloading);
    }

    #[test]
    fn late_progress_does_not_revive_a_canceled_download() {
        let mut t = DownloadTable::new();
        t.apply(&started(2));
        t.apply(&Outgoing::Canceled { id: 2, url: "u".into() });
        t.apply(&progress(2, 500));
        let e = t.get(2).unwrap();
        assert_eq!(e.status, Status::Canceled);
        assert_eq!(e.done, 500);
    }

    #[test]
    fn failure_records_error_and_removal_forgets() {
        let mut t = DownloadTable::new();
        t.apply(&started(3));
        t.apply(&Outgoing::Failed { id: 3, url: "u".into(), error: "http error: timed out".into() });
        assert_eq!(t.get(3).unwrap().error.as_deref(), Some("http error: timed out"));
        t.apply(&Outgoing::Removed { id: 3 });
        assert!(t.get(3).is_none());
        assert!(t.is_empty());
    }

    #[test]
    fn a_taken_name_waits_for_the_save_as_dialog() {
        let mut t = DownloadTable::new();
        t.apply(&Outgoing::Added { id: 6, url: "u".into() });
        t.apply(&Outgoing::NameTaken { id: 6, url: "u".into(), path: r"C:\dl\report.pdf".into(), total: Some(42) });
        let e = t.get(6).unwrap();
        assert_eq!(e.status, Status::Confirming);
        assert_eq!(e.path.as_deref(), Some(r"C:\dl\report.pdf"));
        assert_eq!(e.total, Some(42));
        assert_eq!(e.connections, 0, "never started, so a retry does not reuse the file");
    }

    #[test]
    fn snapshot_replaces_and_round_trips() {
        let mut a = DownloadTable::new();
        a.apply(&started(1));
        a.apply(&started(2));
        let snap = a.snapshot();
        assert_eq!(snap.iter().map(|e| e.id).collect::<Vec<_>>(), vec![1, 2]);

        // A client that joins late adopts the snapshot wholesale.
        let mut b = DownloadTable::new();
        b.apply(&started(9));
        b.apply(&Outgoing::Downloads { items: snap.clone() });
        assert_eq!(b.snapshot(), snap);

        // The snapshot survives the wire.
        let json = serde_json::to_string(&Outgoing::Downloads { items: snap.clone() }).unwrap();
        let back: Outgoing = serde_json::from_str(&json).unwrap();
        let mut c = DownloadTable::new();
        c.apply(&back);
        assert_eq!(c.snapshot(), snap);
    }

    #[test]
    fn ignores_non_download_messages() {
        let mut t = DownloadTable::new();
        t.apply(&Outgoing::Pong);
        t.apply(&Outgoing::Error { message: "x".into() });
        assert!(t.is_empty());
    }
}
