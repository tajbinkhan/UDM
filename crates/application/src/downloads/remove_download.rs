use uidm_domain::download::DownloadError;
use uidm_domain::id::DownloadId;
use uidm_domain::ports::{DownloadControl, DownloadFiles, DownloadList, QueueStore};
use uidm_domain::queue;

/// Forget a download that is no longer running. Its files stay on disk
/// unless asked, when they go to the Recycle Bin.
pub struct RemoveDownload<'a, Q, D, X, F> {
    queues: &'a Q,
    downloads: &'a D,
    control: &'a X,
    files: &'a F,
}

impl<'a, Q: QueueStore, D: DownloadList, X: DownloadControl, F: DownloadFiles> RemoveDownload<'a, Q, D, X, F> {
    pub fn new(queues: &'a Q, downloads: &'a D, control: &'a X, files: &'a F) -> Self {
        RemoveDownload { queues, downloads, control, files }
    }

    pub fn run(&self, id: DownloadId, delete_files: bool) -> Result<(), DownloadError> {
        let entry = self.downloads.entry(id).ok_or(DownloadError::NoDownload(id))?;
        if !entry.status.is_terminal() {
            return Err(DownloadError::StillRunning(id));
        }
        self.control.forget(id);
        if self.queues.edit(|qs| queue::leave(qs, id)) {
            self.queues.queues_changed();
        }
        if delete_files && entry.wrote_files() {
            self.files
                .recycle(entry.path.as_deref(), entry.partial.as_deref())
                .map_err(|e| DownloadError::FilesStay { url: entry.url.to_string(), reason: e.to_string() })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, Control, Downloads, Files, Queues};
    use uidm_domain::download::Status;
    use uidm_domain::queue::MAIN_QUEUE;

    fn saved(id: u64, status: Status, connections: usize) -> uidm_domain::download::DownloadEntry {
        let mut e = row(id, status);
        e.connections = connections;
        e.path = Some(format!("/dl/{id}.zip"));
        e
    }

    #[test]
    fn it_leaves_the_list_and_its_queue() {
        let queues = Queues::main();
        queues.edit(|qs| qs[0].order = vec![DownloadId::new(1)]);
        let downloads = Downloads::with(&[(1, Status::Failed)]);
        let (control, files) = (Control::default(), Files::default());
        RemoveDownload::new(&queues, &downloads, &control, &files).run(DownloadId::new(1), false).unwrap();
        assert_eq!(control.forgotten(), [1]);
        assert!(queues.get(MAIN_QUEUE).order.is_empty());
        assert_eq!(queues.changes(), 1);
        assert!(files.recycled().is_empty(), "its files stay unless asked");
    }

    #[test]
    fn its_files_go_only_when_it_wrote_them() {
        let queues = Queues::main();
        let downloads = Downloads::rows([saved(1, Status::Canceled, 0), saved(2, Status::Done, 0)]);
        let (control, files) = (Control::default(), Files::default());
        let remove = RemoveDownload::new(&queues, &downloads, &control, &files);
        remove.run(DownloadId::new(1), true).unwrap();
        remove.run(DownloadId::new(2), true).unwrap();
        assert_eq!(files.recycled(), [(Some("/dl/2.zip".into()), None)], "1 never got past its save-as dialog");
        assert_eq!(queues.changes(), 0, "it was in no queue");
    }

    #[test]
    fn a_running_or_unknown_download_stays() {
        let queues = Queues::main();
        let downloads = Downloads::with(&[(1, Status::Downloading)]);
        let (control, files) = (Control::default(), Files::default());
        let remove = RemoveDownload::new(&queues, &downloads, &control, &files);
        assert_eq!(remove.run(DownloadId::new(1), false), Err(DownloadError::StillRunning(DownloadId::new(1))));
        assert_eq!(remove.run(DownloadId::new(9), false), Err(DownloadError::NoDownload(DownloadId::new(9))));
        assert!(control.forgotten().is_empty());
    }

    #[test]
    fn files_that_cannot_go_are_reported_after_it_left_the_list() {
        let queues = Queues::main();
        let downloads = Downloads::rows([saved(1, Status::Done, 1)]);
        let (control, files) = (Control::default(), Files::failing());
        let err = RemoveDownload::new(&queues, &downloads, &control, &files).run(DownloadId::new(1), true).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Removed https://dl.example/1.zip from the list, but its files stay: the file is open"
        );
        assert_eq!(control.forgotten(), [1]);
    }
}
