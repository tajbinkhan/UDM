use std::path::Path;

use uidm_domain::download::{DownloadEntry, DownloadError, Status};
use uidm_domain::ports::{Attention, DownloadControl, DownloadFiles, DownloadList, QueueStore};

use super::RetryDownload;

/// What [`UseExisting`] did with the older download.
#[derive(Debug, PartialEq, Eq)]
pub enum Existing {
    /// The finished file is gone: add the new download after all.
    Gone,
    /// It was pointed at: its completion dialog shown again, or the window
    /// brought up for a running one.
    Shown { id: u64, url: String },
    /// It was stopped and started again, as a new row.
    Resumed { id: u64, url: String },
    /// It was stopped and could not start again.
    NotResumed { id: u64, url: String, error: DownloadError },
}

/// Point the user at an older download instead of adding its address again:
/// show a finished one's completion dialog again, resume a stopped one, and
/// bring the window up for a running one.
pub struct UseExisting<'a, Q, D, X, F, A> {
    queues: &'a Q,
    downloads: &'a D,
    control: &'a X,
    files: &'a F,
    attention: &'a A,
}

impl<'a, Q, D, X, F, A> UseExisting<'a, Q, D, X, F, A>
where
    Q: QueueStore,
    D: DownloadList,
    X: DownloadControl,
    F: DownloadFiles,
    A: Attention,
{
    pub fn new(queues: &'a Q, downloads: &'a D, control: &'a X, files: &'a F, attention: &'a A) -> Self {
        UseExisting { queues, downloads, control, files, attention }
    }

    pub fn run(&self, old: &DownloadEntry) -> Existing {
        let (id, url) = (old.id, old.url.clone());
        match old.status {
            Status::Done if !old.path.as_deref().is_some_and(|p| self.files.is_file(Path::new(p))) => Existing::Gone,
            Status::Done => {
                self.attention.show_done(id);
                Existing::Shown { id, url }
            }
            s if s.is_retryable() => match RetryDownload::new(self.queues, self.downloads, self.control).run(id) {
                Ok((id, url)) => Existing::Resumed { id, url },
                Err(error) => Existing::NotResumed { id, url, error },
            },
            _ => {
                self.attention.show_window();
                Existing::Shown { id, url }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, Control, Downloads, Files, Queues, Shown, RESTARTED_ID};

    fn with_file(id: u64, status: Status) -> DownloadEntry {
        let mut e = row(id, status);
        e.path = Some(format!("/dl/{id}.zip"));
        e
    }

    #[test]
    fn a_finished_download_shows_its_dialog_while_its_file_is_there() {
        let queues = Queues::main();
        let downloads = Downloads::rows([with_file(1, Status::Done)]);
        let (control, shown) = (Control::default(), Shown::default());
        let files = Files::with_existing(&["/dl/1.zip"]);
        let use_existing = UseExisting::new(&queues, &downloads, &control, &files, &shown);
        let url = "https://dl.example/1.zip".to_string();
        assert_eq!(use_existing.run(&with_file(1, Status::Done)), Existing::Shown { id: 1, url });
        assert_eq!(shown.done(), [1]);
        let no_files = Files::default();
        let gone = UseExisting::new(&queues, &downloads, &control, &no_files, &shown);
        assert_eq!(gone.run(&with_file(1, Status::Done)), Existing::Gone);
    }

    #[test]
    fn a_stopped_one_resumes_and_a_running_one_brings_the_window_up() {
        let queues = Queues::main();
        let downloads = Downloads::with(&[(1, Status::Failed), (2, Status::Downloading)]);
        let (control, files, shown) = (Control::default(), Files::default(), Shown::default());
        let use_existing = UseExisting::new(&queues, &downloads, &control, &files, &shown);
        let resumed = use_existing.run(&row(1, Status::Failed));
        assert_eq!(resumed, Existing::Resumed { id: RESTARTED_ID, url: "https://dl.example/1.zip".into() });
        assert!(matches!(use_existing.run(&row(2, Status::Downloading)), Existing::Shown { id: 2, .. }));
        assert_eq!(shown.windows(), 1);
    }
}
