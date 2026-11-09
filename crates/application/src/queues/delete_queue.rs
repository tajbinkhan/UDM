use uidm_domain::id::QueueId;
use uidm_domain::ports::{DownloadList, QueueRuns, QueueStore};
use uidm_domain::queue::{self, QueueError};

/// Delete a queue; its downloads move to the main queue, and the running
/// ones keep running.
pub struct DeleteQueue<'a, Q, D, R> {
    queues: &'a Q,
    downloads: &'a D,
    runs: &'a R,
}

impl<'a, Q: QueueStore, D: DownloadList, R: QueueRuns> DeleteQueue<'a, Q, D, R> {
    pub fn new(queues: &'a Q, downloads: &'a D, runs: &'a R) -> Self {
        DeleteQueue { queues, downloads, runs }
    }

    pub fn run(&self, id: QueueId) -> Result<(), QueueError> {
        let finished = self.downloads.finished();
        self.queues.edit(|qs| queue::delete(qs, id, &finished))?;
        self.runs.end(id);
        self.queues.queues_changed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Downloads, Queues, Runs};
    use uidm_domain::download::Status;
    use uidm_domain::id::DownloadId;
    use uidm_domain::queue::{Queue, MAIN_QUEUE};

    #[test]
    fn its_unfinished_downloads_move_to_the_main_queue() {
        let queues = Queues::main();
        queues.edit(|qs| {
            qs.push(Queue {
                order: vec![DownloadId::new(5), DownloadId::new(6)],
                ..Queue::new(QueueId::new(2), "Night")
            })
        });
        let downloads = Downloads::with(&[(5, Status::Done), (6, Status::Failed)]);
        let runs = Runs::default();
        runs.begin(QueueId::new(2));
        DeleteQueue::new(&queues, &downloads, &runs).run(QueueId::new(2)).unwrap();
        assert!(!runs.running(2), "its run is forgotten");
        assert_eq!(queues.edit(|qs| qs.len()), 1);
        assert_eq!(queues.get(MAIN_QUEUE).order, [DownloadId::new(6)]);
        assert_eq!(queues.changes(), 1);
    }

    #[test]
    fn the_main_queue_stays() {
        let (queues, downloads, runs) = (Queues::main(), Downloads::default(), Runs::default());
        let delete = DeleteQueue::new(&queues, &downloads, &runs);
        assert_eq!(delete.run(MAIN_QUEUE), Err(QueueError::MainQueue));
        assert_eq!(delete.run(QueueId::new(9)), Err(QueueError::NoQueue(QueueId::new(9))));
        assert_eq!(queues.changes(), 0);
    }
}
