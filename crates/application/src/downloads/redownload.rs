use uidm_domain::download::{DownloadError, Status};
use uidm_domain::id::DownloadId;
use uidm_domain::ports::{DownloadControl, DownloadFiles, DownloadList};

/// Start a finished or stopped download again from the first byte, under
/// the same id, so it keeps its place and description. It goes to the same
/// name and folder; a file still there is handled by the file-exists
/// setting, as anybody else's file would be.
pub struct Redownload<'a, D, X, F> {
    downloads: &'a D,
    control: &'a X,
    files: &'a F,
}

impl<'a, D: DownloadList, X: DownloadControl, F: DownloadFiles> Redownload<'a, D, X, F> {
    pub fn new(downloads: &'a D, control: &'a X, files: &'a F) -> Self {
        Redownload { downloads, control, files }
    }

    /// Returns the address it downloads again.
    pub fn run(&self, id: DownloadId) -> Result<String, DownloadError> {
        let entry = self.downloads.entry(id).ok_or(DownloadError::NoDownload(id))?;
        let download = entry.redownload_request(self.downloads.request(id))?;
        // No resume record may be left: a partial file still there is a
        // taken name like any other, not something to resume.
        if entry.status != Status::Done {
            let paths: Vec<&str> = [entry.path.as_deref(), entry.partial.as_deref()].into_iter().flatten().collect();
            self.files.forget_resume(&paths);
        }
        let reset = self.downloads.update(id, |e| {
            if !e.status.can_download_again() {
                return false;
            }
            e.reset_for_redownload();
            true
        });
        match reset {
            None => return Err(DownloadError::NoDownload(id)),
            Some(false) => return Err(DownloadError::StartedMeanwhile),
            Some(true) => {}
        }
        self.downloads.keep_request(id, download.clone());
        self.control.start_again(id, download);
        Ok(entry.url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, Control, Downloads, Files};
    use uidm_domain::bytes::ByteCount;

    #[test]
    fn a_stopped_download_starts_over_under_its_id() {
        let mut old = row(1, Status::Failed);
        old.connections = 2;
        old.done = ByteCount::new(500);
        old.path = Some("/dl/1.zip".into());
        old.partial = Some("/tmp/1.zip.part".into());
        let downloads = Downloads::rows([old]);
        let (control, files) = (Control::default(), Files::default());
        let url = Redownload::new(&downloads, &control, &files).run(DownloadId::new(1)).unwrap();
        assert_eq!(url, "https://dl.example/1.zip");
        let e = downloads.entry(DownloadId::new(1)).unwrap();
        assert_eq!((e.status, e.done, e.partial), (Status::Queued, ByteCount::ZERO, None));
        assert_eq!(files.forgot_resume(), ["/dl/1.zip", "/tmp/1.zip.part"]);
        let (id, d) = &control.started_again()[0];
        assert_eq!((*id, d.filename.as_deref()), (1, Some("1.zip")));
        assert!(downloads.request(DownloadId::new(1)).is_some(), "its request is kept for the next retry");
    }

    #[test]
    fn a_finished_download_keeps_its_file_until_the_new_one_lands() {
        let mut old = row(1, Status::Done);
        old.path = Some("/dl/1.zip".into());
        let downloads = Downloads::rows([old]);
        let (control, files) = (Control::default(), Files::default());
        Redownload::new(&downloads, &control, &files).run(DownloadId::new(1)).unwrap();
        assert!(files.forgot_resume().is_empty());
    }

    #[test]
    fn a_running_download_is_refused() {
        let downloads = Downloads::with(&[(1, Status::Downloading)]);
        let (control, files) = (Control::default(), Files::default());
        let again = Redownload::new(&downloads, &control, &files);
        assert_eq!(again.run(DownloadId::new(1)), Err(DownloadError::NotFinishedOrStopped));
        assert_eq!(again.run(DownloadId::new(9)), Err(DownloadError::NoDownload(DownloadId::new(9))));
        assert!(control.started_again().is_empty());
    }
}
