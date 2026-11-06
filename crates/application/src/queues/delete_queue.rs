use uidm_domain::ports::{DownloadList, QueueStore};
use uidm_domain::queue::{self, QueueError};

/// Delete a queue; its downloads move to the main queue.
pub struct DeleteQueue<'a, Q, D> {
    queues: &'a Q,
    downloads: &'a D,
}

impl<'a, Q: QueueStore, D: DownloadList> DeleteQueue<'a, Q, D> {
    pub fn new(queues: &'a Q, downloads: &'a D) -> Self {
        DeleteQueue { queues, downloads }
    }

    pub fn run(&self, id: u64) -> Result<(), QueueError> {
        let finished = self.downloads.finished();
        self.queues.edit(|qs| queue::delete(qs, id, &finished))?;
        self.queues.queues_changed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queues::memory::{Downloads, Queues};
    use uidm_domain::download::Status;
    use uidm_domain::queue::{Queue, MAIN_QUEUE};

    #[test]
    fn its_unfinished_downloads_move_to_the_main_queue() {
        let queues = Queues::main();
        queues.edit(|qs| qs.push(Queue { order: vec![5, 6], ..Queue::new(2, "Night") }));
        let downloads = Downloads::with(&[(5, Status::Done), (6, Status::Failed)]);
        DeleteQueue::new(&queues, &downloads).run(2).unwrap();
        assert_eq!(queues.edit(|qs| qs.len()), 1);
        assert_eq!(queues.get(MAIN_QUEUE).order, [6]);
        assert_eq!(queues.changes(), 1);
    }

    #[test]
    fn the_main_queue_stays() {
        let (queues, downloads) = (Queues::main(), Downloads::default());
        let delete = DeleteQueue::new(&queues, &downloads);
        assert_eq!(delete.run(MAIN_QUEUE), Err(QueueError::MainQueue));
        assert_eq!(delete.run(9), Err(QueueError::NoQueue(9)));
        assert_eq!(queues.changes(), 0);
    }
}
