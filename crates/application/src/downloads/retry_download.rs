use std::path::PathBuf;

use uidm_domain::download::DownloadError;
use uidm_domain::id::DownloadId;
use uidm_domain::ports::{DownloadControl, DownloadList, QueueStore};
use uidm_domain::queue;

use crate::queues::MoveToQueue;

/// Start a failed, canceled or interrupted download again as a new row,
/// from its kept request or from its address alone. It resumes its partial
/// file when the server still has the same file, and keeps its place in its
/// queue.
pub struct RetryDownload<'a, Q, D, X> {
    queues: &'a Q,
    downloads: &'a D,
    control: &'a X,
}

impl<'a, Q: QueueStore, D: DownloadList, X: DownloadControl> RetryDownload<'a, Q, D, X> {
    pub fn new(queues: &'a Q, downloads: &'a D, control: &'a X) -> Self {
        RetryDownload { queues, downloads, control }
    }

    /// Returns the new row's id and its address.
    pub fn run(&self, id: DownloadId) -> Result<(DownloadId, String), DownloadError> {
        let entry = self.downloads.entry(id).ok_or(DownloadError::NoDownload(id))?;
        if !entry.status.is_retryable() {
            return Err(DownloadError::NotRetryable(id));
        }
        let download = entry.retry_request(self.downloads.take_request(id));
        let queue = self.queues.edit(|qs| {
            let q = queue::queue_of(qs, id);
            queue::leave(qs, id);
            q
        });
        self.downloads.drop_row(id);
        let started = entry.started();
        let path = entry.path.clone().filter(|_| started).map(PathBuf::from);
        let partial = entry.partial.clone().filter(|_| started).map(PathBuf::from);
        let (new_id, url) = self.control.restart(download, path, partial, entry.limit_remember);
        if let Some(q) = queue {
            // The new row takes the old one's queue; a queue deleted
            // meanwhile leaves it outside any.
            let _ = MoveToQueue::new(self.queues, self.downloads).run(new_id, q, None);
        }
        Ok((new_id, url))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, Control, Downloads, Queues, RESTARTED_ID};
    use uidm_domain::download::{Download, Status};
    use uidm_domain::queue::MAIN_QUEUE;

    #[test]
    fn a_started_download_resumes_into_its_files_and_keeps_its_queue() {
        let queues = Queues::main();
        queues.edit(|qs| qs[0].order = vec![DownloadId::new(1)]);
        let mut old = row(1, Status::Interrupted);
        old.connections = 4;
        old.path = Some("/dl/1.zip".into());
        old.partial = Some("/tmp/1.zip.part".into());
        old.limit_remember = false;
        let downloads = Downloads::rows([old]);
        downloads.keep_request(
            DownloadId::new(1),
            Download { url: "https://dl.example/kept.zip".into(), ..Download::default() },
        );
        let control = Control::default();
        let (new_id, url) = RetryDownload::new(&queues, &downloads, &control).run(DownloadId::new(1)).unwrap();
        assert_eq!((new_id, url.as_str()), (RESTARTED_ID, "https://dl.example/kept.zip"));
        let r = &control.restarted()[0];
        assert_eq!(r.path, Some(PathBuf::from("/dl/1.zip")));
        assert_eq!(r.partial, Some(PathBuf::from("/tmp/1.zip.part")));
        assert!(!r.remember_limit, "its limit lasts only until it stops");
        assert!(!downloads.has_row(1), "the old row goes");
        // The new row joins the old one's queue once the list has it.
        assert!(queues.get(MAIN_QUEUE).order.is_empty());
    }

    #[test]
    fn one_that_never_started_gets_no_files_of_its_own() {
        let queues = Queues::main();
        let mut old = row(1, Status::Canceled);
        old.path = Some("/dl/someone-elses.zip".into());
        let downloads = Downloads::rows([old]);
        let control = Control::default();
        RetryDownload::new(&queues, &downloads, &control).run(DownloadId::new(1)).unwrap();
        let r = &control.restarted()[0];
        assert_eq!((r.path.as_ref(), r.download.url.as_str()), (None, "https://dl.example/1.zip"));
    }

    #[test]
    fn only_a_stopped_download_is_retried() {
        let queues = Queues::main();
        let downloads = Downloads::with(&[(1, Status::Done)]);
        let control = Control::default();
        let retry = RetryDownload::new(&queues, &downloads, &control);
        assert_eq!(retry.run(DownloadId::new(1)), Err(DownloadError::NotRetryable(DownloadId::new(1))));
        assert_eq!(retry.run(DownloadId::new(9)), Err(DownloadError::NoDownload(DownloadId::new(9))));
        assert!(control.restarted().is_empty());
    }
}
