use uidm_domain::batch;
use uidm_domain::download::{Download, DownloadError, Mismatch, Status};
use uidm_domain::ports::{DownloadControl, DownloadList, QueueStore};

use super::RetryDownload;

/// What [`RefreshLink`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refreshed {
    /// The download waits for the next link the browser hands over.
    Waiting,
    /// The new link is not for the file the partial copy came from; nothing
    /// changed, and why.
    Mismatch(Mismatch),
    /// The download has the new link: a waiting one keeps waiting with it,
    /// any other started again.
    NewLink(String),
}

/// Give a stopped download a new link, or wait for the browser's next one.
pub struct RefreshLink<'a, Q, D, X> {
    queues: &'a Q,
    downloads: &'a D,
    control: &'a X,
}

impl<'a, Q: QueueStore, D: DownloadList, X: DownloadControl> RefreshLink<'a, Q, D, X> {
    pub fn new(queues: &'a Q, downloads: &'a D, control: &'a X) -> Self {
        RefreshLink { queues, downloads, control }
    }

    /// Without `download`, wait for the next link the browser hands over.
    /// With it, check the link against the partial file unless `restart`;
    /// on a match (or with `restart`) keep it and start again, resuming the
    /// partial file when the link is for the same file.
    pub fn run(&self, id: u64, download: Option<Download>, restart: bool) -> Result<Refreshed, DownloadError> {
        let entry = self.downloads.entry(id).ok_or(DownloadError::NoDownload(id))?;
        if !entry.status.is_retryable() {
            return Err(DownloadError::LinkNeedsStop);
        }
        let Some(download) = download else {
            self.control.wait_for_link(Some(id));
            return Ok(Refreshed::Waiting);
        };
        if !batch::is_download_url(&download.url) {
            return Err(DownloadError::NotDownloadLink(download.url));
        }
        if self.control.waiting_for_link() == Some(id) {
            self.control.wait_for_link(None);
        }
        if entry.started() && !restart {
            if let Some(partial) = entry.partial.as_deref().or(entry.path.as_deref()) {
                let check = self.control.check_partial(&download, partial);
                if let Some(reason) = check.map_err(|e| DownloadError::Unreachable(e.to_string()))? {
                    return Ok(Refreshed::Mismatch(reason));
                }
            }
        }
        let url = download.url.clone();
        self.downloads.keep_request(id, download);
        if entry.status == Status::Waiting {
            self.downloads
                .update(id, |e| {
                    e.url = url.clone();
                    true
                })
                .ok_or(DownloadError::NoDownload(id))?;
        } else {
            RetryDownload::new(self.queues, self.downloads, self.control).run(id)?;
        }
        Ok(Refreshed::NewLink(url))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, Control, Downloads, Queues};
    use uidm_domain::ports::ControlError;

    fn link(url: &str) -> Option<Download> {
        Some(Download { url: url.into(), ..Download::default() })
    }

    fn started(id: u64, status: Status) -> Downloads {
        let mut e = row(id, status);
        e.connections = 2;
        e.partial = Some("/tmp/1.part".into());
        Downloads::rows([e])
    }

    #[test]
    fn without_a_link_it_waits_for_the_browser() {
        let (queues, downloads, control) =
            (Queues::main(), Downloads::with(&[(1, Status::Failed)]), Control::default());
        let refresh = RefreshLink::new(&queues, &downloads, &control);
        assert_eq!(refresh.run(1, None, false), Ok(Refreshed::Waiting));
        assert_eq!(control.waiting_for_link(), Some(1));
    }

    #[test]
    fn a_matching_link_restarts_and_stops_the_wait() {
        let (queues, downloads) = (Queues::main(), started(1, Status::Failed));
        let control = Control::checking(Ok(None));
        control.wait_for_link(Some(1));
        let done = RefreshLink::new(&queues, &downloads, &control).run(1, link("https://new.example/1.zip"), false);
        assert_eq!(done, Ok(Refreshed::NewLink("https://new.example/1.zip".into())));
        assert_eq!(control.waiting_for_link(), None);
        assert_eq!(control.restarted()[0].download.url, "https://new.example/1.zip");
    }

    #[test]
    fn a_different_file_changes_nothing_unless_restarted() {
        let (queues, downloads) = (Queues::main(), started(1, Status::Interrupted));
        let control = Control::checking(Ok(Some(Mismatch::Changed)));
        let refresh = RefreshLink::new(&queues, &downloads, &control);
        assert_eq!(refresh.run(1, link("https://new.example/x"), false), Ok(Refreshed::Mismatch(Mismatch::Changed)));
        assert!(control.restarted().is_empty());
        assert!(refresh.run(1, link("https://new.example/x"), true).is_ok(), "restart skips the check");
    }

    #[test]
    fn a_waiting_download_keeps_waiting_with_its_new_link() {
        let (queues, downloads, control) =
            (Queues::main(), Downloads::with(&[(1, Status::Waiting)]), Control::default());
        RefreshLink::new(&queues, &downloads, &control).run(1, link("https://new.example/1.zip"), false).unwrap();
        assert_eq!(downloads.entry(1).unwrap().url, "https://new.example/1.zip");
        assert_eq!(downloads.request(1).unwrap().url, "https://new.example/1.zip");
        assert!(control.restarted().is_empty());
    }

    #[test]
    fn bad_links_and_running_downloads_are_refused() {
        let queues = Queues::main();
        let downloads = Downloads::with(&[(1, Status::Downloading), (2, Status::Failed)]);
        let control = Control::checking(Err(ControlError("timed out".into())));
        let refresh = RefreshLink::new(&queues, &downloads, &control);
        assert_eq!(refresh.run(1, None, false), Err(DownloadError::LinkNeedsStop));
        assert_eq!(refresh.run(2, link("mailto:x"), false), Err(DownloadError::NotDownloadLink("mailto:x".into())));
        let unreachable = started(3, Status::Failed);
        let refresh = RefreshLink::new(&queues, &unreachable, &control);
        assert_eq!(refresh.run(3, link("https://x/"), false), Err(DownloadError::Unreachable("timed out".into())));
    }
}
