use uidm_domain::download::Status;
use uidm_domain::id::DownloadId;
use uidm_domain::ports::{DownloadControl, DownloadList, QueueFinish, QueueRuns, QueueStore, SyncChecks};

use super::FinishRun;

/// Something a pass over the running queues did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueEvent {
    /// A download that failed in this run waits to be tried again.
    Retried { queue: String, id: DownloadId, attempt: u32, of: u32 },
    /// A run ended with nothing left to do.
    Finished { queue: String },
}

/// One pass over the started queues: retry the downloads that failed in
/// this run while retries are left, start waiting ones up to each queue's
/// limit, check a synchronizing queue's finished downloads with the
/// server, and end the runs that are done.
pub struct RunQueues<'a, Q, D, R, X, S, F> {
    queues: &'a Q,
    downloads: &'a D,
    runs: &'a R,
    control: &'a X,
    checks: &'a S,
    finish: &'a F,
}

impl<'a, Q, D, R, X, S, F> RunQueues<'a, Q, D, R, X, S, F>
where
    Q: QueueStore,
    D: DownloadList,
    R: QueueRuns,
    X: DownloadControl,
    S: SyncChecks,
    F: QueueFinish,
{
    pub fn new(queues: &'a Q, downloads: &'a D, runs: &'a R, control: &'a X, checks: &'a S, finish: &'a F) -> Self {
        RunQueues { queues, downloads, runs, control, checks, finish }
    }

    pub fn run(&self) -> Vec<QueueEvent> {
        let mut events = Vec::new();
        let running: Vec<_> = self.queues.edit(|qs| qs.iter().filter(|q| q.running).cloned().collect());
        for q in running {
            let statuses = |order: &[DownloadId]| -> Vec<(DownloadId, Status)> {
                order.iter().filter_map(|id| self.downloads.status(*id).map(|s| (*id, s))).collect()
            };
            let started = self.runs.started_in_run(q.id);

            // Failed in this run and retries left: wait again.
            for (id, status) in statuses(&q.order) {
                if status == Status::Failed && started.contains(&id) {
                    if let Some(attempt) = self.runs.count_retry(id, q.retries) {
                        self.downloads.set_waiting(id);
                        events.push(QueueEvent::Retried { queue: q.name.clone(), id, attempt, of: q.retries });
                    }
                }
            }

            let statuses = statuses(&q.order);
            let checking = self.checks.checking();
            let mut active = statuses.iter().filter(|(id, s)| s.is_active() || checking.contains(id)).count();
            for (id, status) in &statuses {
                if active >= q.max_concurrent {
                    break;
                }
                if *status == Status::Waiting && self.control.dispatch(*id).is_ok() {
                    self.runs.note_started(q.id, *id);
                    active += 1;
                }
                // A synchronizing run checks each finished download once,
                // as many at a time as it downloads.
                let unchecked = || !self.runs.started_in_run(q.id).contains(id);
                if q.kind.is_sync() && *status == Status::Done && !checking.contains(id) && unchecked() {
                    self.runs.note_started(q.id, *id);
                    self.checks.check(*id);
                    active += 1;
                }
            }

            if q.run_finished(&statuses, active, self.runs.started_any(q.id)) {
                if let Some(queue) = FinishRun::new(self.queues, self.runs, self.finish).run(q.id) {
                    events.push(QueueEvent::Finished { queue });
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Checks, Control, Downloads, Finish, Queues, Runs};
    use uidm_domain::id::QueueId;
    use uidm_domain::queue::{Queue, QueueKind, MAIN_QUEUE};

    fn running(order: &[u64], max: usize) -> Queues {
        let queues = Queues::main();
        queues.edit(|qs| {
            qs[0].running = true;
            qs[0].order = order.iter().copied().map(DownloadId::new).collect();
            qs[0].max_concurrent = max;
        });
        queues
    }

    #[test]
    fn waiting_downloads_start_up_to_the_queues_limit() {
        let queues = running(&[1, 2, 3, 4], 2);
        let downloads =
            Downloads::with(&[(1, Status::Downloading), (2, Status::Waiting), (3, Status::Waiting), (4, Status::Done)]);
        let (runs, control, checks, finish) =
            (Runs::default(), Control::default(), Checks::default(), Finish::default());
        runs.begin(MAIN_QUEUE);
        let events = RunQueues::new(&queues, &downloads, &runs, &control, &checks, &finish).run();
        assert!(events.is_empty());
        assert_eq!(control.dispatched(), [2], "one runs already, so one more fits");
        assert!(runs.started_in_run(MAIN_QUEUE).contains(&DownloadId::new(2)));
    }

    #[test]
    fn a_download_that_failed_in_this_run_is_retried_while_retries_last() {
        let queues = running(&[1], 1);
        queues.edit(|qs| qs[0].retries = 1);
        let downloads = Downloads::with(&[(1, Status::Failed)]);
        let (runs, control, checks, finish) =
            (Runs::default(), Control::default(), Checks::default(), Finish::default());
        runs.begin(MAIN_QUEUE);
        runs.note_started(MAIN_QUEUE, DownloadId::new(1));
        let pass = RunQueues::new(&queues, &downloads, &runs, &control, &checks, &finish);
        let events = pass.run();
        assert_eq!(
            events,
            [QueueEvent::Retried { queue: "Main queue".into(), id: DownloadId::new(1), attempt: 1, of: 1 }]
        );
        assert_eq!(control.dispatched(), [1], "it starts again at once");
        // It fails again: no retries left, so the run ends.
        downloads.set(1, Status::Failed);
        assert_eq!(pass.run(), [QueueEvent::Finished { queue: "Main queue".into() }]);
        assert_eq!(finish.ended().len(), 1);
    }

    #[test]
    fn a_synchronizing_queue_checks_each_finished_download_once() {
        let queues = Queues::main();
        queues.edit(|qs| {
            qs.push(Queue {
                running: true,
                kind: QueueKind::Synchronize,
                order: vec![DownloadId::new(1), DownloadId::new(2)],
                max_concurrent: 5,
                ..Queue::new(QueueId::new(2), "Mirror")
            })
        });
        let downloads = Downloads::with(&[(1, Status::Done), (2, Status::Done)]);
        let (runs, control, finish) = (Runs::default(), Control::default(), Finish::default());
        let checks = Checks::going_on(&[2]);
        runs.begin(QueueId::new(2));
        let pass = RunQueues::new(&queues, &downloads, &runs, &control, &checks, &finish);
        assert!(pass.run().is_empty(), "a check is going on");
        assert_eq!(checks.started(), [1], "2 is being checked already");
        assert!(pass.run().is_empty());
        assert_eq!(checks.started(), [1], "1 was checked in this run");
    }

    #[test]
    fn a_queue_with_nothing_to_do_ends_its_run() {
        let queues = running(&[1], 1);
        let downloads = Downloads::with(&[(1, Status::Done)]);
        let (runs, control, checks, finish) =
            (Runs::default(), Control::default(), Checks::default(), Finish::default());
        runs.begin(MAIN_QUEUE);
        let events = RunQueues::new(&queues, &downloads, &runs, &control, &checks, &finish).run();
        assert_eq!(events, [QueueEvent::Finished { queue: "Main queue".into() }]);
        assert!(!queues.get(MAIN_QUEUE).running);
        assert!(finish.ended().is_empty(), "it started nothing, so nothing happens at its end");
    }
}
