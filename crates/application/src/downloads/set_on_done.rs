use uidm_domain::download::{DownloadError, OnDone};
use uidm_domain::id::DownloadId;
use uidm_domain::ports::DownloadList;

/// Set or clear what a download does once it finishes.
pub struct SetOnDone<'a, D> {
    downloads: &'a D,
}

impl<'a, D: DownloadList> SetOnDone<'a, D> {
    pub fn new(downloads: &'a D) -> Self {
        SetOnDone { downloads }
    }

    pub fn run(&self, id: DownloadId, on_done: Option<OnDone>) -> Result<(), DownloadError> {
        let mut result = Ok(());
        self.downloads
            .update(id, |e| {
                result = e.set_on_done(on_done);
                result.is_ok()
            })
            .ok_or(DownloadError::NoDownload(id))?;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Downloads;
    use uidm_domain::download::Status;
    use uidm_domain::queue::WhenDone;

    #[test]
    fn only_an_unfinished_download_takes_an_on_done() {
        let downloads = Downloads::with(&[(1, Status::Downloading), (2, Status::Done)]);
        let then = OnDone { then: WhenDone::Sleep, ..OnDone::default() };
        let set = SetOnDone::new(&downloads);
        set.run(DownloadId::new(1), Some(then)).unwrap();
        assert_eq!(downloads.entry(DownloadId::new(1)).unwrap().on_done, Some(then));
        assert_eq!(set.run(DownloadId::new(2), Some(then)), Err(DownloadError::AlreadyFinished));
        assert_eq!(set.run(DownloadId::new(9), None), Err(DownloadError::NoDownload(DownloadId::new(9))));
    }
}
