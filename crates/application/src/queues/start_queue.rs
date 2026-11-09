use pullbay_domain::ports::{Clock, QueueRuns, QueueSounds, QueueStore};
use pullbay_domain::queue::QueueError;

/// Start a queue: its waiting downloads run in order, as the scheduler
/// starts them.
pub struct StartQueue<'a, Q, R, C, S> {
    queues: &'a Q,
    runs: &'a R,
    clock: &'a C,
    sounds: &'a S,
}

impl<'a, Q: QueueStore, R: QueueRuns, C: Clock, S: QueueSounds> StartQueue<'a, Q, R, C, S> {
    pub fn new(queues: &'a Q, runs: &'a R, clock: &'a C, sounds: &'a S) -> Self {
        StartQueue { queues, runs, clock, sounds }
    }

    /// Start queue `id`. `scheduled_day` is the day whose scheduled run this
    /// is, or `None` when it is started by hand.
    pub fn run(&self, id: u64, scheduled_day: Option<i64>) -> Result<(), QueueError> {
        let now = self.clock.now();
        let was_running = self.queues.edit(|qs| {
            let q = qs.iter_mut().find(|q| q.id == id).ok_or(QueueError::NoQueue(id))?;
            Ok::<_, QueueError>(q.start(scheduled_day, now))
        })?;
        self.queues.queues_changed();
        self.runs.begin(id);
        if !was_running {
            self.sounds.queue_started();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Queues, Runs, Sounds, FIXED_NOW};
    use pullbay_domain::queue::MAIN_QUEUE;

    #[test]
    fn a_queue_started_by_hand_runs_and_chimes_once() {
        let (queues, runs, sounds) = (Queues::main(), Runs::default(), Sounds::default());
        let start = StartQueue::new(&queues, &runs, &FIXED_NOW, &sounds);
        start.run(MAIN_QUEUE, None).unwrap();
        let q = queues.get(MAIN_QUEUE);
        assert!(q.running && !q.started_by_schedule);
        assert_eq!(q.last_scheduled_day, None);
        assert!(runs.running(MAIN_QUEUE));
        start.run(MAIN_QUEUE, None).unwrap();
        assert_eq!(sounds.started(), 1, "starting a running queue again is silent");
        assert_eq!(queues.changes(), 2);
    }

    #[test]
    fn a_scheduled_start_remembers_its_day_and_time() {
        let (queues, runs, sounds) = (Queues::main(), Runs::default(), Sounds::default());
        StartQueue::new(&queues, &runs, &FIXED_NOW, &sounds).run(MAIN_QUEUE, Some(FIXED_NOW.0.day - 1)).unwrap();
        let q = queues.get(MAIN_QUEUE);
        assert!(q.started_by_schedule);
        assert_eq!(q.last_scheduled_day, Some(FIXED_NOW.0.day - 1));
        assert_eq!(q.last_scheduled_start, Some(FIXED_NOW.0.total()));
    }

    #[test]
    fn an_unknown_queue_is_an_error() {
        let (queues, runs, sounds) = (Queues::main(), Runs::default(), Sounds::default());
        let start = StartQueue::new(&queues, &runs, &FIXED_NOW, &sounds);
        assert_eq!(start.run(9, None), Err(QueueError::NoQueue(9)));
        assert!(!runs.running(9));
        assert_eq!((queues.changes(), sounds.started()), (0, 0));
    }
}
