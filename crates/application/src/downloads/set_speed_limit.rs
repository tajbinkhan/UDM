use pullbay_domain::download::{self, DownloadError};
use pullbay_domain::id::DownloadId;
use pullbay_domain::ports::{DownloadControl, DownloadList};

/// Set or clear a download's own speed limit. It is kept on the row (so in
/// the history) and in the kept request, and a running download changes
/// speed at once.
pub struct SetSpeedLimit<'a, D, X> {
    downloads: &'a D,
    control: &'a X,
}

impl<'a, D: DownloadList, X: DownloadControl> SetSpeedLimit<'a, D, X> {
    pub fn new(downloads: &'a D, control: &'a X) -> Self {
        SetSpeedLimit { downloads, control }
    }

    /// Limit download `id` to `bytes_per_sec`; `None` or 0 lifts the limit.
    pub fn run(&self, id: DownloadId, bytes_per_sec: Option<u64>) -> Result<(), DownloadError> {
        let limit = download::speed_limit(bytes_per_sec)?;
        self.downloads
            .update(id, |e| {
                e.speed_limit = limit;
                true
            })
            .ok_or(DownloadError::NoDownload(id))?;
        self.downloads.edit_request(id, |d| d.speed_limit = limit);
        self.control.set_limit(id, limit);
        Ok(())
    }
}

/// Set whether a download keeps its own speed limit when it is stopped and
/// resumed.
pub struct SetLimitRemember<'a, D> {
    downloads: &'a D,
}

impl<'a, D: DownloadList> SetLimitRemember<'a, D> {
    pub fn new(downloads: &'a D) -> Self {
        SetLimitRemember { downloads }
    }

    pub fn run(&self, id: DownloadId, remember: bool) -> Result<(), DownloadError> {
        self.downloads
            .update(id, |e| {
                e.limit_remember = remember;
                true
            })
            .ok_or(DownloadError::NoDownload(id))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Control, Downloads};
    use pullbay_domain::download::{Download, Status};

    #[test]
    fn the_limit_reaches_the_row_the_request_and_the_engine() {
        let downloads = Downloads::with(&[(1, Status::Downloading)]);
        downloads.keep_request(DownloadId::new(1), Download::default());
        let control = Control::default();
        SetSpeedLimit::new(&downloads, &control).run(DownloadId::new(1), Some(64 * 1024)).unwrap();
        assert_eq!(downloads.entry(DownloadId::new(1)).unwrap().speed_limit, Some(64 * 1024));
        assert_eq!(downloads.request(DownloadId::new(1)).unwrap().speed_limit, Some(64 * 1024));
        assert_eq!(control.limits(), [(1, Some(64 * 1024))]);
    }

    #[test]
    fn a_limit_out_of_range_or_an_unknown_download_is_refused() {
        let downloads = Downloads::with(&[(1, Status::Downloading)]);
        let control = Control::default();
        let set = SetSpeedLimit::new(&downloads, &control);
        assert!(matches!(set.run(DownloadId::new(1), Some(10)), Err(DownloadError::BadSpeedLimit { .. })));
        assert_eq!(set.run(DownloadId::new(9), None), Err(DownloadError::NoDownload(DownloadId::new(9))));
        assert!(control.limits().is_empty());
    }

    #[test]
    fn remembering_the_limit_is_kept_on_the_row() {
        let downloads = Downloads::with(&[(1, Status::Waiting)]);
        SetLimitRemember::new(&downloads).run(DownloadId::new(1), false).unwrap();
        assert!(!downloads.entry(DownloadId::new(1)).unwrap().limit_remember);
        assert_eq!(
            SetLimitRemember::new(&downloads).run(DownloadId::new(9), true),
            Err(DownloadError::NoDownload(DownloadId::new(9)))
        );
    }
}
