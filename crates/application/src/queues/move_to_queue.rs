use uidm_domain::download::Status;
use uidm_domain::ports::{DownloadList, QueueStore};
use uidm_domain::queue::{self, QueueError};

/// Put a download in a queue, at a position or at the end. A download that
/// is not running waits there.
pub struct MoveToQueue<'a, Q, D> {
    queues: &'a Q,
    downloads: &'a D,
}

impl<'a, Q: QueueStore, D: DownloadList> MoveToQueue<'a, Q, D> {
    pub fn new(queues: &'a Q, downloads: &'a D) -> Self {
        MoveToQueue { queues, downloads }
    }

    pub fn run(&self, id: u64, to: u64, position: Option<usize>) -> Result<(), QueueError> {
        let status = self.downloads.status(id).ok_or(QueueError::NoDownload(id))?;
        let (sync, current) =
            self.queues.edit(|qs| (qs.iter().find(|q| q.id == to).map(|q| q.kind.is_sync()), queue::queue_of(qs, id)));
        let sync = sync.ok_or(QueueError::NoQueue(to))?;
        if !queue::may_wait(status, sync) {
            return Err(QueueError::Finished);
        }
        // Joining a queue (not just moving within it) makes a stopped
        // download wait to run again.
        let reorder = position.is_some() && current == Some(to);
        if status.is_retryable() && status != Status::Waiting && !reorder && !self.downloads.set_waiting(id) {
            return Err(QueueError::NoDownload(id));
        }
        self.queues.edit(|qs| queue::place(qs, id, to, position))?;
        self.queues.queues_changed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Downloads, Queues};
    use uidm_domain::queue::{Queue, QueueKind, MAIN_QUEUE};

    #[test]
    fn a_stopped_download_joins_and_waits() {
        let queues = Queues::main();
        let downloads = Downloads::with(&[(1, Status::Failed)]);
        MoveToQueue::new(&queues, &downloads).run(1, MAIN_QUEUE, None).unwrap();
        assert_eq!(queues.get(MAIN_QUEUE).order, [1]);
        assert_eq!(downloads.status(1), Some(Status::Waiting));
        assert_eq!(queues.changes(), 1);
    }

    #[test]
    fn moving_within_its_queue_keeps_its_state() {
        let queues = Queues::main();
        queues.edit(|qs| qs[0].order = vec![1, 2, 3]);
        let downloads = Downloads::with(&[(1, Status::Waiting), (2, Status::Waiting), (3, Status::Canceled)]);
        MoveToQueue::new(&queues, &downloads).run(3, MAIN_QUEUE, Some(0)).unwrap();
        assert_eq!(queues.get(MAIN_QUEUE).order, [3, 1, 2]);
        assert_eq!(downloads.status(3), Some(Status::Canceled));
    }

    #[test]
    fn it_leaves_its_old_queue() {
        let queues = Queues::main();
        queues.edit(|qs| {
            qs[0].order = vec![1];
            qs.push(Queue::new(2, "Night"));
        });
        let downloads = Downloads::with(&[(1, Status::Waiting)]);
        MoveToQueue::new(&queues, &downloads).run(1, 2, None).unwrap();
        assert!(queues.get(MAIN_QUEUE).order.is_empty());
        assert_eq!(queues.get(2).order, [1]);
    }

    #[test]
    fn only_a_synchronizing_queue_takes_a_finished_download() {
        let queues = Queues::main();
        queues.edit(|qs| qs.push(Queue { kind: QueueKind::Synchronize, ..Queue::new(2, "Mirror") }));
        let downloads = Downloads::with(&[(1, Status::Done), (2, Status::Confirming)]);
        let mv = MoveToQueue::new(&queues, &downloads);
        assert_eq!(mv.run(1, MAIN_QUEUE, None), Err(QueueError::Finished));
        mv.run(1, 2, None).unwrap();
        assert_eq!(mv.run(2, 2, None), Err(QueueError::Finished), "the save-as dialog is still open");
        assert_eq!(mv.run(9, 2, None), Err(QueueError::NoDownload(9)));
        assert_eq!(mv.run(1, 7, None), Err(QueueError::NoQueue(7)));
        assert_eq!(queues.changes(), 1);
    }
}
