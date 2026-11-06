use uidm_domain::download::Status;
use uidm_domain::ports::{
    Clock, ControlError, DownloadControl, DownloadList, PausedDownloads, QueueRuns, QueueSounds, QueueStore,
};

use crate::queues::StartQueue;

/// What [`ResumeAll`] could not start again.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Resumed {
    pub failed: Vec<(u64, ControlError)>,
}

/// Start again what [`PauseAll`](super::PauseAll) stopped, and every
/// interrupted download.
pub struct ResumeAll<'a, Q, D, R, X, P, C, S> {
    queues: &'a Q,
    downloads: &'a D,
    runs: &'a R,
    control: &'a X,
    paused: &'a P,
    clock: &'a C,
    sounds: &'a S,
}

impl<'a, Q, D, R, X, P, C, S> ResumeAll<'a, Q, D, R, X, P, C, S>
where
    Q: QueueStore,
    D: DownloadList,
    R: QueueRuns,
    X: DownloadControl,
    P: PausedDownloads,
    C: Clock,
    S: QueueSounds,
{
    #[allow(clippy::too_many_arguments)] // One per port; a struct of them would only rename the list.
    pub fn new(
        queues: &'a Q,
        downloads: &'a D,
        runs: &'a R,
        control: &'a X,
        paused: &'a P,
        clock: &'a C,
        sounds: &'a S,
    ) -> Self {
        ResumeAll { queues, downloads, runs, control, paused, clock, sounds }
    }

    pub fn run(&self) -> Resumed {
        let (queues, paused) = self.paused.take();
        let start = StartQueue::new(self.queues, self.runs, self.clock, self.sounds);
        for q in queues {
            // A queue deleted while paused stays gone.
            let _ = start.run(q, None);
        }
        let ids = self
            .downloads
            .ids_where(|id, s| s == Status::Interrupted || (s == Status::Canceled && paused.contains(&id)));
        let failed = ids.into_iter().filter_map(|id| self.control.resume(id).err().map(|e| (id, e))).collect();
        Resumed { failed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Control, Downloads, Paused, Queues, Runs, Sounds, FIXED_NOW};
    use uidm_domain::queue::MAIN_QUEUE;

    #[test]
    fn paused_and_interrupted_downloads_resume() {
        let queues = Queues::main();
        let downloads = Downloads::with(&[
            (1, Status::Canceled),
            (2, Status::Canceled),
            (3, Status::Interrupted),
            (4, Status::Failed),
        ]);
        let (runs, paused, sounds) = (Runs::default(), Paused::default(), Sounds::default());
        let control = Control::refusing(&[3]);
        paused.remember(&[MAIN_QUEUE], &[1]);
        let resumed = ResumeAll::new(&queues, &downloads, &runs, &control, &paused, &FIXED_NOW, &sounds).run();
        assert!(queues.get(MAIN_QUEUE).running);
        assert_eq!(control.resumed(), [1], "2 was canceled by hand, 4 failed");
        assert_eq!(resumed.failed, [(3, ControlError("download 3 cannot resume".into()))]);
        assert_eq!(paused.take(), Default::default(), "it is forgotten");
    }
}
