use pullbay_domain::id::{DownloadId, QueueId};
use pullbay_domain::ports::{ControlError, DownloadControl, DownloadList, QueueStore};
use pullbay_domain::queue::{self, QueueError};

use crate::queues::MoveToQueue;

/// Why [`StartOrWait`] could not do as asked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StartOrWaitError {
    #[error(transparent)]
    Start(#[from] ControlError),
    #[error(transparent)]
    Queue(#[from] QueueError),
}

/// Start a waiting download now, or have it wait in a queue: the one asked
/// for, else the one "Download later" uses, else the main queue. A download
/// already in a queue stays where it is.
pub struct StartOrWait<'a, Q, D, X> {
    queues: &'a Q,
    downloads: &'a D,
    control: &'a X,
}

impl<'a, Q: QueueStore, D: DownloadList, X: DownloadControl> StartOrWait<'a, Q, D, X> {
    pub fn new(queues: &'a Q, downloads: &'a D, control: &'a X) -> Self {
        StartOrWait { queues, downloads, control }
    }

    /// `later` is the queue the settings send "Download later" to.
    pub fn run(
        &self,
        id: DownloadId,
        start: bool,
        queue: Option<QueueId>,
        later: Option<QueueId>,
    ) -> Result<(), StartOrWaitError> {
        if start {
            return Ok(self.control.dispatch(id)?);
        }
        let target = self
            .queues
            .edit(|qs| queue::queue_of(qs, id).is_none().then(|| queue::existing_or_main(qs, queue.or(later))));
        if let Some(target) = target {
            MoveToQueue::new(self.queues, self.downloads).run(id, target, None)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Control, Downloads, Queues};
    use pullbay_domain::download::Status;
    use pullbay_domain::queue::{Queue, MAIN_QUEUE};

    #[test]
    fn start_hands_it_to_the_engine() {
        let (queues, downloads) = (Queues::main(), Downloads::with(&[(1, Status::Waiting)]));
        let control = Control::default();
        StartOrWait::new(&queues, &downloads, &control).run(DownloadId::new(1), true, None, None).unwrap();
        assert_eq!(control.dispatched(), [1]);
        let refusing = Control::refusing(&[1]);
        let err =
            StartOrWait::new(&queues, &downloads, &refusing).run(DownloadId::new(1), true, None, None).unwrap_err();
        assert_eq!(err.to_string(), "download 1 cannot start");
    }

    #[test]
    fn waiting_goes_to_the_later_queue_unless_one_is_named() {
        let queues = Queues::main();
        queues.edit(|qs| qs.push(Queue::new(QueueId::new(2), "Later")));
        let downloads = Downloads::with(&[(1, Status::Waiting), (2, Status::Waiting), (3, Status::Waiting)]);
        let control = Control::default();
        let wait = StartOrWait::new(&queues, &downloads, &control);
        wait.run(DownloadId::new(1), false, None, Some(QueueId::new(2))).unwrap();
        wait.run(DownloadId::new(2), false, Some(MAIN_QUEUE), Some(QueueId::new(2))).unwrap();
        assert_eq!(queues.get(2).order, [DownloadId::new(1)]);
        assert_eq!(queues.get(MAIN_QUEUE).order, [DownloadId::new(2)]);
        wait.run(DownloadId::new(1), false, Some(MAIN_QUEUE), None).unwrap();
        assert_eq!(queues.get(2).order, [DownloadId::new(1)], "one already in a queue stays there");
        assert!(control.dispatched().is_empty());
    }
}
