use uidm_domain::batch;
use uidm_domain::download::{same_address, Download, DownloadError, DuplicateStep};
use uidm_domain::ports::{DownloadControl, DownloadFiles, DownloadIntake, DownloadList, QueueStore};
use uidm_domain::queue;
use uidm_domain::settings::Duplicate;

use super::RemoveDownload;

/// What [`AddMany`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AddedMany {
    /// The new downloads, in order.
    pub ids: Vec<u64>,
    /// How many were already in the list and not added again.
    pub skipped: usize,
    /// Older downloads a new one was to replace but that stayed.
    pub not_replaced: Vec<(u64, DownloadError)>,
}

/// Add many downloads to wait in a queue, announcing the queue once rather
/// than once per download. Items that are not web addresses, or whose
/// request cannot be sent, are left out, and so are those already in the
/// list unless their own answer or the duplicate setting says to add them
/// (with a new name, or in place of a stopped download).
pub struct AddMany<'a, Q, D, X, F, I> {
    queues: &'a Q,
    downloads: &'a D,
    control: &'a X,
    files: &'a F,
    intake: &'a I,
}

impl<'a, Q, D, X, F, I> AddMany<'a, Q, D, X, F, I>
where
    Q: QueueStore,
    D: DownloadList,
    X: DownloadControl,
    F: DownloadFiles,
    I: DownloadIntake,
{
    pub fn new(queues: &'a Q, downloads: &'a D, control: &'a X, files: &'a F, intake: &'a I) -> Self {
        AddMany { queues, downloads, control, files, intake }
    }

    /// Add `items` to `queue` (the main queue when it is gone). `if_duplicate`
    /// is the duplicate setting, for items that give no answer of their own.
    pub fn run(&self, items: Vec<Download>, queue: Option<u64>, if_duplicate: Duplicate) -> AddedMany {
        let queue = self.queues.edit(|qs| queue::existing_or_main(qs, queue));
        let mut added = AddedMany::default();
        for mut d in items {
            if !batch::is_download_url(&d.url) || !self.intake.sendable(&d) {
                continue;
            }
            let old =
                d.may_be_duplicate().then(|| self.downloads.find_last(|e| same_address(&e.url, &d.url))).flatten();
            if let Some(old) = old {
                let choice = d.duplicate.unwrap_or(if_duplicate);
                if !choice.adds_in_bulk(old.status) {
                    added.skipped += 1;
                    continue;
                }
                if d.settle_duplicate(&old, choice) == DuplicateStep::Replace {
                    let remove = RemoveDownload::new(self.queues, self.downloads, self.control, self.files);
                    if let Err(e) = remove.run(old.id, false) {
                        added.not_replaced.push((old.id, e));
                    }
                }
            }
            added.ids.push(self.intake.stage(d).0);
        }
        let joined = self.queues.edit(|qs| match qs.iter_mut().find(|q| q.id == queue) {
            Some(q) => {
                q.order.extend(added.ids.iter().copied());
                true
            }
            None => false,
        });
        if joined {
            self.queues.queues_changed();
        }
        added
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, Control, Downloads, Files, Intake, Queues};
    use uidm_domain::download::Status;
    use uidm_domain::queue::MAIN_QUEUE;
    use uidm_domain::settings::FileExists;

    fn links(urls: &[&str]) -> Vec<Download> {
        urls.iter().map(|u| Download { url: u.to_string(), ..Download::default() }).collect()
    }

    #[test]
    fn duplicates_are_skipped_unless_told_to_add_them() {
        let queues = Queues::main();
        let downloads = Downloads::with(&[(1, Status::Done)]);
        let (control, files) = (Control::default(), Files::default());
        let intake = Intake::into(&downloads);
        let add = AddMany::new(&queues, &downloads, &control, &files, &intake);
        let items = links(&["https://dl.example/1.zip", "https://dl.example/other.zip", "mailto:x", "https://x/a.bad"]);
        let added = add.run(items, None, Duplicate::Ask);
        assert_eq!((added.ids.len(), added.skipped), (1, 1), "not a web address, not sendable, already there");
        assert_eq!(queues.get(MAIN_QUEUE).order, added.ids);
        let added = add.run(links(&["https://dl.example/1.zip"]), None, Duplicate::NewName);
        assert_eq!(added.ids.len(), 1);
        assert_eq!(downloads.request(added.ids[0]).unwrap().file_exists, Some(FileExists::Rename));
    }

    #[test]
    fn overwriting_replaces_a_stopped_download() {
        let queues = Queues::main();
        let mut old = row(1, Status::Failed);
        old.path = Some("/dl/1.zip".into());
        let downloads = Downloads::rows([old]);
        let (control, files) = (Control::default(), Files::default());
        let intake = Intake::into(&downloads);
        let added = AddMany::new(&queues, &downloads, &control, &files, &intake).run(
            links(&["https://dl.example/1.zip"]),
            None,
            Duplicate::Overwrite,
        );
        assert_eq!(control.forgotten(), [1], "the old one leaves the list");
        let req = downloads.request(added.ids[0]).unwrap();
        assert_eq!((req.filename.as_deref(), req.file_exists), (Some("1.zip"), Some(FileExists::Overwrite)));
    }
}
