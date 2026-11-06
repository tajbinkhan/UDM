use uidm_domain::ports::{Clock, DownloadControl, DownloadList, QueueRuns, QueueSounds, QueueStore};
use uidm_domain::queue::ScheduleStep;

use super::{StartQueue, StopQueue};

/// A queue its schedule started or stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scheduled {
    pub queue: String,
    pub step: ScheduleStep,
}

/// Start and stop the queues whose schedules (or grabber projects' stop
/// times) say so now.
pub struct RunSchedules<'a, Q, D, R, X, C, S> {
    queues: &'a Q,
    downloads: &'a D,
    runs: &'a R,
    control: &'a X,
    clock: &'a C,
    sounds: &'a S,
}

impl<'a, Q, D, R, X, C, S> RunSchedules<'a, Q, D, R, X, C, S>
where
    Q: QueueStore,
    D: DownloadList,
    R: QueueRuns,
    X: DownloadControl,
    C: Clock,
    S: QueueSounds,
{
    pub fn new(queues: &'a Q, downloads: &'a D, runs: &'a R, control: &'a X, clock: &'a C, sounds: &'a S) -> Self {
        RunSchedules { queues, downloads, runs, control, clock, sounds }
    }

    /// Take one look at every queue. Returns what was started and stopped.
    pub fn run(&self) -> Vec<Scheduled> {
        let now = self.clock.now();
        let queues = self.queues.edit(|qs| qs.clone());
        let mut done = Vec::new();
        for q in &queues {
            let Some(step) = q.schedule_step(now, self.runs.started_any(q.id)) else {
                continue;
            };
            let result = match step {
                ScheduleStep::Start { day } => {
                    StartQueue::new(self.queues, self.runs, self.clock, self.sounds).run(q.id, Some(day))
                }
                ScheduleStep::Stop(_) => StopQueue::new(self.queues, self.downloads, self.runs, self.control).run(q.id),
            };
            // A queue deleted meanwhile is simply skipped.
            if result.is_ok() {
                done.push(Scheduled { queue: q.name.clone(), step });
            }
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queues::memory::{Control, Downloads, FixedClock, Queues, Runs, Sounds, FIXED_NOW};
    use uidm_domain::download::Status;
    use uidm_domain::queue::{Moment, Queue, Schedule, StopReason};

    fn every_day_at(start: &str, stop: Option<&str>) -> Schedule {
        Schedule {
            start: start.into(),
            stop: stop.map(Into::into),
            days: (0..7).collect(),
            date: None,
            repeat_minutes: None,
            wake: false,
        }
    }

    #[test]
    fn a_due_queue_starts_and_a_finished_run_stops() {
        let queues = Queues::main();
        queues.edit(|qs| {
            qs.push(Queue { schedule: Some(every_day_at("09:00", Some("12:00"))), ..Queue::new(2, "Morning") });
            qs.push(Queue { schedule: Some(every_day_at("10:00", None)), ..Queue::new(3, "Later") });
        });
        let (downloads, runs, control, sounds) =
            (Downloads::with(&[(7, Status::Downloading)]), Runs::default(), Control::default(), Sounds::default());

        // 09:30: Morning is due, Later is not.
        let done = RunSchedules::new(&queues, &downloads, &runs, &control, &FIXED_NOW, &sounds).run();
        assert_eq!(done, [Scheduled { queue: "Morning".into(), step: ScheduleStep::Start { day: FIXED_NOW.0.day } }]);
        assert!(queues.get(2).running && queues.get(2).started_by_schedule);
        assert!(!queues.get(3).running);
        assert_eq!(sounds.started(), 1);

        // 12:00: its stop time stops it, and its running download waits again.
        queues.edit(|qs| qs[1].order = vec![7]);
        runs.started(2, 7);
        let noon = FixedClock(Moment { minute: 12 * 60, ..FIXED_NOW.0 });
        let done = RunSchedules::new(&queues, &downloads, &runs, &control, &noon, &sounds).run();
        assert!(
            done.contains(&Scheduled { queue: "Morning".into(), step: ScheduleStep::Stop(StopReason::ScheduleEnded) })
        );
        assert!(!queues.get(2).running);
        assert_eq!(control.held(), [7]);
    }

    #[test]
    fn nothing_due_changes_nothing() {
        let queues = Queues::main();
        let (downloads, runs, control, sounds) =
            (Downloads::default(), Runs::default(), Control::default(), Sounds::default());
        assert!(RunSchedules::new(&queues, &downloads, &runs, &control, &FIXED_NOW, &sounds).run().is_empty());
        assert_eq!(queues.changes(), 0);
    }
}
