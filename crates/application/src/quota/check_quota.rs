use uidm_domain::bytes::ByteCount;
use uidm_domain::download::{DownloadError, Status};
use uidm_domain::id::DownloadId;
use uidm_domain::ports::{Clock, DownloadControl, DownloadList, QueueRuns, QueueSounds, QueueStore, QuotaState};
use uidm_domain::quota::{self, QuotaLimit, QuotaNotice, QuotaStep};

use crate::downloads::{RetryDownload, StopRunning};
use crate::queues::StartQueue;

/// A change [`CheckQuota`] made.
#[derive(Debug, PartialEq, Eq)]
pub enum QuotaEvent {
    /// The quota was reached and everything stopped.
    Reached { used: ByteCount, limit: ByteCount },
    /// The window allows downloads again; these could not resume.
    Resumed { failed: Vec<(DownloadId, DownloadError)> },
}

/// Pause everything when the download quota is reached, warn shortly
/// before, and resume what it stopped once the window allows. Runs with the
/// scheduler.
pub struct CheckQuota<'a, Q, D, R, X, C, S, U> {
    queues: &'a Q,
    downloads: &'a D,
    runs: &'a R,
    control: &'a X,
    clock: &'a C,
    sounds: &'a S,
    quota: &'a U,
}

impl<'a, Q, D, R, X, C, S, U> CheckQuota<'a, Q, D, R, X, C, S, U>
where
    Q: QueueStore,
    D: DownloadList,
    R: QueueRuns,
    X: DownloadControl,
    C: Clock,
    S: QueueSounds,
    U: QuotaState,
{
    #[allow(clippy::too_many_arguments)] // One per port; a struct of them would only rename the list.
    pub fn new(
        queues: &'a Q,
        downloads: &'a D,
        runs: &'a R,
        control: &'a X,
        clock: &'a C,
        sounds: &'a S,
        quota: &'a U,
    ) -> Self {
        CheckQuota { queues, downloads, runs, control, clock, sounds, quota }
    }

    /// Check the quota the settings ask for (`None` when it is off) at
    /// minute `now`.
    pub fn run(&self, limit: Option<QuotaLimit>, now: u64) -> Option<QuotaEvent> {
        // With the quota off, whatever it stopped resumes.
        let limit = limit.unwrap_or(QuotaLimit { bytes: ByteCount::new(u64::MAX), window: 0, warn: false });
        let used = if limit.window == 0 { ByteCount::ZERO } else { self.quota.used(now, limit.window) };
        let notice = |paused, resumes_at| QuotaNotice { used, limit: limit.bytes, paused, resumes_at };
        match quota::step(used, limit, self.quota.paused(), self.quota.warned()) {
            QuotaStep::Keep => None,
            QuotaStep::Warn => {
                self.quota.set_warned(true);
                self.quota.announce(notice(false, None));
                None
            }
            QuotaStep::Pause { first } => {
                // Also stops what started since.
                let stopped = StopRunning::new(self.queues, self.downloads, self.runs, self.control).run();
                self.quota.hold(&stopped.queues, &stopped.downloads);
                if !first {
                    return None;
                }
                let at = self.quota.frees_at(now, limit.window, limit.bytes) * 60;
                self.quota.announce(notice(true, Some(at)));
                Some(QuotaEvent::Reached { used, limit: limit.bytes })
            }
            QuotaStep::Resume => {
                let (queues, downloads) = self.quota.release();
                self.quota.set_warned(false);
                let start = StartQueue::new(self.queues, self.runs, self.clock, self.sounds);
                for q in queues {
                    // A queue deleted while paused stays gone.
                    let _ = start.run(q, None);
                }
                let mut downloads: Vec<DownloadId> = downloads.into_iter().collect();
                downloads.sort_unstable();
                let retry = RetryDownload::new(self.queues, self.downloads, self.control);
                let failed = downloads
                    .into_iter()
                    .filter(|id| self.downloads.status(*id) == Some(Status::Canceled))
                    .filter_map(|id| retry.run(id).err().map(|e| (id, e)))
                    .collect();
                self.quota.announce(notice(false, None));
                Some(QuotaEvent::Resumed { failed })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Control, Downloads, Queues, Quota, Runs, Sounds, FIXED_NOW};
    use uidm_domain::queue::MAIN_QUEUE;

    const LIMIT: QuotaLimit = QuotaLimit { bytes: ByteCount::new(1000), window: 60, warn: true };

    struct World {
        queues: Queues,
        downloads: Downloads,
        runs: Runs,
        control: Control,
        sounds: Sounds,
    }

    impl World {
        fn new() -> Self {
            let queues = Queues::main();
            queues.edit(|qs| qs[0].running = true);
            World {
                queues,
                downloads: Downloads::with(&[(1, Status::Downloading), (2, Status::Failed)]),
                runs: Runs::default(),
                control: Control::default(),
                sounds: Sounds::default(),
            }
        }

        fn check(&self, quota: &Quota, limit: Option<QuotaLimit>) -> Option<QuotaEvent> {
            let w = self;
            CheckQuota::new(&w.queues, &w.downloads, &w.runs, &w.control, &FIXED_NOW, &w.sounds, quota).run(limit, 100)
        }
    }

    #[test]
    fn reaching_the_quota_stops_everything_once() {
        let w = World::new();
        let quota = Quota::at(ByteCount::new(1000), 130);
        assert_eq!(
            w.check(&quota, Some(LIMIT)),
            Some(QuotaEvent::Reached { used: ByteCount::new(1000), limit: ByteCount::new(1000) })
        );
        assert!(!w.queues.get(MAIN_QUEUE).running);
        assert_eq!(w.control.stopped(), [1]);
        let notice = QuotaNotice {
            used: ByteCount::new(1000),
            limit: ByteCount::new(1000),
            paused: true,
            resumes_at: Some(130 * 60),
        };
        assert_eq!(quota.announced(), [notice]);
        assert_eq!(w.check(&quota, Some(LIMIT)), None, "the next pass only stops what started since");
        assert_eq!(quota.announced().len(), 1);
    }

    #[test]
    fn the_window_freeing_up_resumes_what_the_quota_stopped() {
        let w = World::new();
        let quota = Quota::at(ByteCount::new(1000), 0);
        w.check(&quota, Some(LIMIT));
        w.downloads.set(1, Status::Canceled);
        quota.set_used(ByteCount::new(10));
        assert_eq!(w.check(&quota, Some(LIMIT)), Some(QuotaEvent::Resumed { failed: vec![] }));
        assert!(w.queues.get(MAIN_QUEUE).running);
        assert_eq!(w.control.restarted().len(), 1);
        assert!(w.downloads.entry(DownloadId::new(1)).is_none(), "it goes on as a new row");
        assert!(!quota.paused());
    }

    #[test]
    fn nine_tenths_warn_once() {
        let w = World::new();
        let quota = Quota::at(ByteCount::new(900), 0);
        assert_eq!(w.check(&quota, Some(LIMIT)), None);
        assert_eq!(w.check(&quota, Some(LIMIT)), None);
        assert_eq!(
            quota.announced(),
            [QuotaNotice { used: ByteCount::new(900), limit: ByteCount::new(1000), paused: false, resumes_at: None }]
        );
        assert!(w.queues.get(MAIN_QUEUE).running);
    }

    #[test]
    fn turning_the_quota_off_resumes_what_it_stopped() {
        let w = World::new();
        let quota = Quota::at(ByteCount::new(1000), 0);
        w.check(&quota, Some(LIMIT));
        assert!(matches!(w.check(&quota, None), Some(QuotaEvent::Resumed { .. })));
        assert_eq!(w.check(&quota, None), None);
    }
}
