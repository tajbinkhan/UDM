use udm_domain::address::Link;
use udm_domain::download::Download;
use udm_domain::id::{DownloadId, QueueId};
use udm_domain::ports::{DownloadIntake, QueueStore};
use udm_domain::queue;

/// Add a download to a queue without starting it or opening a connection:
/// the queue it names, else the one "Download later" uses, else the main
/// queue.
pub struct AddLater<'a, Q, I> {
    queues: &'a Q,
    intake: &'a I,
}

impl<'a, Q: QueueStore, I: DownloadIntake> AddLater<'a, Q, I> {
    pub fn new(queues: &'a Q, intake: &'a I) -> Self {
        AddLater { queues, intake }
    }

    /// `later` is the queue the settings send "Download later" to. Returns
    /// the new download's id and address.
    pub fn run(&self, download: Download, later: Option<QueueId>) -> (DownloadId, Link) {
        let wanted = download.queue.or(later);
        let queue = self.queues.edit(|qs| queue::existing_or_main(qs, wanted));
        let (id, url) = self.intake.stage(download);
        let joined = self.queues.edit(|qs| match qs.iter_mut().find(|q| q.id == queue) {
            Some(q) => {
                q.order.push(id);
                true
            }
            None => false,
        });
        if joined {
            self.queues.queues_changed();
        }
        (id, url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Downloads, Intake, Queues};
    use udm_domain::queue::{Queue, MAIN_QUEUE};

    fn link(url: &str, queue: Option<u64>) -> Download {
        Download { url: url.into(), queue: queue.map(QueueId::new), ..Download::default() }
    }

    #[test]
    fn it_waits_in_its_own_queue_the_later_queue_or_the_main_one() {
        let queues = Queues::main();
        queues.edit(|qs| qs.push(Queue::new(QueueId::new(2), "Later")));
        let downloads = Downloads::default();
        let intake = Intake::into(&downloads);
        let later = AddLater::new(&queues, &intake);
        let (a, _) = later.run(link("https://x/a", Some(MAIN_QUEUE.get())), Some(QueueId::new(2)));
        let (b, _) = later.run(link("https://x/b", None), Some(QueueId::new(2)));
        let (c, _) = later.run(link("https://x/c", Some(9)), None);
        assert_eq!(queues.get(MAIN_QUEUE).order, [a, c], "a queue that is gone means the main one");
        assert_eq!(queues.get(2).order, [b]);
        assert!(downloads.has_row(a), "it is in the list, waiting");
    }
}
