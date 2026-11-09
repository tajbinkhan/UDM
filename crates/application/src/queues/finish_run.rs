use pullbay_domain::id::QueueId;
use pullbay_domain::ports::{QueueFinish, QueueRuns, QueueStore};

/// End a queue's run once its downloads are done: the queue stops, and when
/// the run started anything, the end of the run does what the queue asks
/// (a sound, a file to open, sleep or shut down).
pub struct FinishRun<'a, Q, R, F> {
    queues: &'a Q,
    runs: &'a R,
    finish: &'a F,
}

impl<'a, Q: QueueStore, R: QueueRuns, F: QueueFinish> FinishRun<'a, Q, R, F> {
    pub fn new(queues: &'a Q, runs: &'a R, finish: &'a F) -> Self {
        FinishRun { queues, runs, finish }
    }

    /// End queue `id`'s run. Returns the queue's name, or `None` when it is
    /// gone.
    pub fn run(&self, id: QueueId) -> Option<String> {
        let started = self.runs.started_in_run(id);
        self.runs.end(id);
        self.runs.forget_retries(&started);
        let (name, end) =
            self.queues.edit(|qs| qs.iter_mut().find(|q| q.id == id).map(|q| (q.name.clone(), q.finish_run())))?;
        self.queues.queues_changed();
        if !started.is_empty() {
            self.finish.run_ended(&name, &end);
        }
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Finish, Queues, Runs};
    use pullbay_domain::id::DownloadId;
    use pullbay_domain::queue::{RunEnd, WhenDone, MAIN_QUEUE};

    #[test]
    fn a_run_that_started_something_ends_as_the_queue_asks() {
        let queues = Queues::main();
        queues.edit(|qs| {
            qs[0].running = true;
            qs[0].when_done = WhenDone::Hibernate;
        });
        let (runs, finish) = (Runs::default(), Finish::default());
        runs.begin(MAIN_QUEUE);
        runs.note_started(MAIN_QUEUE, DownloadId::new(1));
        assert_eq!(runs.count_retry(DownloadId::new(1), 3), Some(1));
        assert_eq!(FinishRun::new(&queues, &runs, &finish).run(MAIN_QUEUE).as_deref(), Some("Main queue"));
        assert!(!queues.get(MAIN_QUEUE).running);
        assert!(!runs.running(MAIN_QUEUE));
        assert_eq!(runs.count_retry(DownloadId::new(1), 3), Some(1), "its retries start over");
        let end = RunEnd { action: WhenDone::Hibernate, open: None, force_close: false };
        assert_eq!(finish.ended(), [("Main queue".to_string(), end)]);
    }

    #[test]
    fn a_run_that_started_nothing_ends_quietly() {
        let queues = Queues::main();
        queues.edit(|qs| qs[0].running = true);
        let (runs, finish) = (Runs::default(), Finish::default());
        FinishRun::new(&queues, &runs, &finish).run(MAIN_QUEUE).unwrap();
        assert!(finish.ended().is_empty());
        assert_eq!(FinishRun::new(&queues, &runs, &finish).run(QueueId::new(9)), None, "no such queue");
    }
}
