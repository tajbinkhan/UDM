use udm_domain::id::DownloadId;
use udm_domain::ports::{DownloadList, QueueStore};
use udm_domain::queue::{self, QueueError};

/// Take a download out of its queue, keeping it in the list. Nothing puts it
/// back by itself, so no queue starts it until it is started by hand or
/// moved to a queue again.
pub struct RemoveFromQueue<'a, Q, D> {
    queues: &'a Q,
    downloads: &'a D,
}

impl<'a, Q: QueueStore, D: DownloadList> RemoveFromQueue<'a, Q, D> {
    pub fn new(queues: &'a Q, downloads: &'a D) -> Self {
        RemoveFromQueue { queues, downloads }
    }

    pub fn run(&self, id: DownloadId) -> Result<(), QueueError> {
        self.downloads.status(id).ok_or(QueueError::NoDownload(id))?;
        if !self.queues.edit(|qs| queue::leave(qs, id)) {
            return Err(QueueError::NotQueued);
        }
        self.queues.queues_changed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Downloads, Queues};
    use udm_domain::download::Status;
    use udm_domain::queue::MAIN_QUEUE;

    #[test]
    fn it_leaves_the_queue_but_stays_listed() {
        let queues = Queues::main();
        queues.edit(|qs| qs[0].order = vec![DownloadId::new(1)]);
        let downloads = Downloads::with(&[(1, Status::Waiting)]);
        let remove = RemoveFromQueue::new(&queues, &downloads);
        remove.run(DownloadId::new(1)).unwrap();
        assert!(queues.get(MAIN_QUEUE).order.is_empty());
        assert_eq!(downloads.status(DownloadId::new(1)), Some(Status::Waiting));
        assert_eq!(remove.run(DownloadId::new(1)), Err(QueueError::NotQueued), "it is in no queue now");
        assert_eq!(remove.run(DownloadId::new(9)), Err(QueueError::NoDownload(DownloadId::new(9))));
        assert_eq!(queues.changes(), 1);
    }
}
