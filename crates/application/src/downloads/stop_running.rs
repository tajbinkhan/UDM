use uidm_domain::download::Status;
use uidm_domain::id::{DownloadId, QueueId};
use uidm_domain::ports::{DownloadControl, DownloadList, QueueRuns, QueueStore};

use crate::queues::StopQueue;

/// What [`StopRunning`] stopped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stopped {
    pub queues: Vec<QueueId>,
    pub downloads: Vec<DownloadId>,
}

/// Stop every started queue and every queued or running download, keeping
/// their progress.
pub struct StopRunning<'a, Q, D, R, X> {
    queues: &'a Q,
    downloads: &'a D,
    runs: &'a R,
    control: &'a X,
}

impl<'a, Q: QueueStore, D: DownloadList, R: QueueRuns, X: DownloadControl> StopRunning<'a, Q, D, R, X> {
    pub fn new(queues: &'a Q, downloads: &'a D, runs: &'a R, control: &'a X) -> Self {
        StopRunning { queues, downloads, runs, control }
    }

    pub fn run(&self) -> Stopped {
        let queues: Vec<QueueId> = self.queues.edit(|qs| qs.iter().filter(|q| q.running).map(|q| q.id).collect());
        let stop = StopQueue::new(self.queues, self.downloads, self.runs, self.control);
        for q in &queues {
            // A queue deleted meanwhile has nothing left to stop.
            let _ = stop.run(*q);
        }
        let downloads = self.downloads.ids_where(|_, s| matches!(s, Status::Queued | Status::Downloading));
        for id in &downloads {
            self.control.stop(*id);
        }
        Stopped { queues, downloads }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Control, Downloads, Queues, Runs};
    use uidm_domain::queue::{Queue, MAIN_QUEUE};

    #[test]
    fn running_queues_and_downloads_stop() {
        let queues = Queues::main();
        queues.edit(|qs| {
            qs[0].running = true;
            qs.push(Queue::new(QueueId::new(2), "Idle"));
        });
        let downloads = Downloads::with(&[(1, Status::Downloading), (2, Status::Waiting), (3, Status::Queued)]);
        let (runs, control) = (Runs::default(), Control::default());
        let stopped = StopRunning::new(&queues, &downloads, &runs, &control).run();
        assert_eq!(
            stopped,
            Stopped { queues: vec![MAIN_QUEUE], downloads: vec![DownloadId::new(1), DownloadId::new(3)] }
        );
        assert!(!queues.get(MAIN_QUEUE).running);
        assert_eq!(control.stopped(), [1, 3]);
    }
}
