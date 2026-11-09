use pullbay_domain::ports::{DownloadControl, DownloadList, QueueRuns, QueueStore};
use pullbay_domain::queue::QueueError;

/// Stop a queue. Its running downloads stop and wait in it again, with
/// their progress kept.
pub struct StopQueue<'a, Q, D, R, X> {
    queues: &'a Q,
    downloads: &'a D,
    runs: &'a R,
    control: &'a X,
}

impl<'a, Q: QueueStore, D: DownloadList, R: QueueRuns, X: DownloadControl> StopQueue<'a, Q, D, R, X> {
    pub fn new(queues: &'a Q, downloads: &'a D, runs: &'a R, control: &'a X) -> Self {
        StopQueue { queues, downloads, runs, control }
    }

    pub fn run(&self, id: u64) -> Result<(), QueueError> {
        let order = self.queues.edit(|qs| {
            let q = qs.iter_mut().find(|q| q.id == id).ok_or(QueueError::NoQueue(id))?;
            q.stop();
            Ok::<_, QueueError>(q.order.clone())
        })?;
        self.queues.queues_changed();
        self.runs.end(id);
        for download in order.into_iter().filter(|d| self.downloads.is_running(*d)) {
            self.control.hold(download);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Control, Downloads, Queues, Runs};
    use pullbay_domain::download::Status;
    use pullbay_domain::queue::{WhenDone, MAIN_QUEUE};

    #[test]
    fn the_running_downloads_are_held_and_the_run_ends() {
        let queues = Queues::main();
        queues.edit(|qs| {
            qs[0].order = vec![1, 2, 3];
            qs[0].running = true;
            qs[0].when_done_once = Some(WhenDone::ShutDown);
        });
        let downloads = Downloads::with(&[(1, Status::Downloading), (2, Status::Waiting), (3, Status::Queued)]);
        let (runs, control) = (Runs::default(), Control::default());
        runs.begin(MAIN_QUEUE);
        StopQueue::new(&queues, &downloads, &runs, &control).run(MAIN_QUEUE).unwrap();
        let q = queues.get(MAIN_QUEUE);
        assert!(!q.running);
        assert_eq!(q.when_done_once, None, "this run's choices are forgotten");
        assert_eq!(control.held(), [1, 3]);
        assert!(!runs.running(MAIN_QUEUE));
        assert_eq!(queues.changes(), 1);
    }

    #[test]
    fn an_unknown_queue_is_an_error() {
        let (queues, downloads, runs, control) =
            (Queues::main(), Downloads::default(), Runs::default(), Control::default());
        assert_eq!(StopQueue::new(&queues, &downloads, &runs, &control).run(9), Err(QueueError::NoQueue(9)));
        assert_eq!(queues.changes(), 0);
    }
}
