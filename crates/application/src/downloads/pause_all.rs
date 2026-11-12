use udm_domain::ports::{DownloadControl, DownloadList, PausedDownloads, QueueRuns, QueueStore};

use super::StopRunning;

/// Stop every started queue and every queued or running download, and
/// remember them for [`ResumeAll`](super::ResumeAll).
pub struct PauseAll<'a, Q, D, R, X, P> {
    queues: &'a Q,
    downloads: &'a D,
    runs: &'a R,
    control: &'a X,
    paused: &'a P,
}

impl<'a, Q, D, R, X, P> PauseAll<'a, Q, D, R, X, P>
where
    Q: QueueStore,
    D: DownloadList,
    R: QueueRuns,
    X: DownloadControl,
    P: PausedDownloads,
{
    pub fn new(queues: &'a Q, downloads: &'a D, runs: &'a R, control: &'a X, paused: &'a P) -> Self {
        PauseAll { queues, downloads, runs, control, paused }
    }

    pub fn run(&self) {
        let stopped = StopRunning::new(self.queues, self.downloads, self.runs, self.control).run();
        self.paused.remember(&stopped.queues, &stopped.downloads);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Control, Downloads, Paused, Queues, Runs};
    use udm_domain::download::Status;
    use udm_domain::id::DownloadId;
    use udm_domain::queue::MAIN_QUEUE;

    #[test]
    fn what_was_stopped_is_remembered() {
        let queues = Queues::main();
        queues.edit(|qs| qs[0].running = true);
        let downloads = Downloads::with(&[(1, Status::Downloading), (2, Status::Failed)]);
        let (runs, control, paused) = (Runs::default(), Control::default(), Paused::default());
        PauseAll::new(&queues, &downloads, &runs, &control, &paused).run();
        let (q, d) = paused.take();
        assert_eq!(q.into_iter().collect::<Vec<_>>(), [MAIN_QUEUE]);
        assert_eq!(d.into_iter().collect::<Vec<_>>(), [DownloadId::new(1)]);
    }
}
