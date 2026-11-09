use uidm_domain::id::QueueId;
use uidm_domain::ports::{DownloadList, QueueStore};
use uidm_domain::queue::{self, Queue, QueueError};

/// Create a queue (id 0) or change one.
pub struct SaveQueue<'a, Q, D> {
    queues: &'a Q,
    downloads: &'a D,
}

impl<'a, Q: QueueStore, D: DownloadList> SaveQueue<'a, Q, D> {
    pub fn new(queues: &'a Q, downloads: &'a D) -> Self {
        SaveQueue { queues, downloads }
    }

    /// Save `edited` and return its id.
    pub fn run(&self, edited: Queue) -> Result<QueueId, QueueError> {
        let finished = self.downloads.finished();
        let id = self.queues.edit(|qs| queue::save(qs, edited, &finished))?;
        self.queues.queues_changed();
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Downloads, Queues};
    use uidm_domain::download::Status;
    use uidm_domain::id::DownloadId;
    use uidm_domain::queue::{QueueKind, MAIN_QUEUE};

    #[test]
    fn a_new_queue_gets_the_next_id() {
        let (queues, downloads) = (Queues::main(), Downloads::default());
        let id = SaveQueue::new(&queues, &downloads).run(Queue::new(QueueId::new(0), " Night ")).unwrap();
        assert_eq!(id, MAIN_QUEUE.next());
        assert_eq!(queues.get(id).name, "Night");
        assert_eq!(queues.changes(), 1);
    }

    #[test]
    fn names_are_unique_without_case() {
        let (queues, downloads) = (Queues::main(), Downloads::default());
        let save = SaveQueue::new(&queues, &downloads);
        save.run(Queue::new(QueueId::new(0), "Night")).unwrap();
        assert_eq!(save.run(Queue::new(QueueId::new(0), "NIGHT")), Err(QueueError::NameTaken("NIGHT".into())));
        assert_eq!(queues.changes(), 1, "a refused change is not announced");
    }

    #[test]
    fn a_one_time_queue_drops_finished_downloads() {
        let queues = Queues::main();
        queues.edit(|qs| qs[0].order = vec![DownloadId::new(1), DownloadId::new(2)]);
        let downloads = Downloads::with(&[(1, Status::Done), (2, Status::Waiting)]);
        let save = SaveQueue::new(&queues, &downloads);
        save.run(Queue { kind: QueueKind::Synchronize, ..queues.get(MAIN_QUEUE) }).unwrap();
        assert_eq!(
            queues.get(MAIN_QUEUE).order,
            [DownloadId::new(1), DownloadId::new(2)],
            "a synchronizing queue keeps them"
        );
        save.run(Queue { kind: QueueKind::OneTime, ..queues.get(MAIN_QUEUE) }).unwrap();
        assert_eq!(queues.get(MAIN_QUEUE).order, [DownloadId::new(2)]);
    }

    #[test]
    fn an_invalid_queue_is_refused() {
        let (queues, downloads) = (Queues::main(), Downloads::default());
        let err = SaveQueue::new(&queues, &downloads).run(Queue::new(QueueId::new(0), " ")).unwrap_err();
        assert_eq!(err.to_string(), "A queue needs a name.");
        assert_eq!(queues.changes(), 0);
    }
}
