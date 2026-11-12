use udm_domain::download::OnDone;
use udm_domain::id::DownloadId;
use udm_domain::ports::{DownloadList, PowerControl};
use udm_domain::queue::PowerRequest;

use crate::power::RunWhenDone;

/// Do what a finished download asked to happen once it finished (close UDM,
/// or sleep, hibernate or shut down after the countdown), then clear it so
/// downloading it again does not repeat it.
pub struct AfterDownload<'a, D, P> {
    downloads: &'a D,
    power: &'a P,
}

impl<'a, D: DownloadList, P: PowerControl> AfterDownload<'a, D, P> {
    pub fn new(downloads: &'a D, power: &'a P) -> Self {
        AfterDownload { downloads, power }
    }

    /// Returns what was asked, or `None` when it asked nothing.
    pub fn run(&self, id: DownloadId) -> Option<OnDone> {
        let mut then = None;
        self.downloads.update(id, |e| {
            then = e.take_on_done();
            then.is_some()
        });
        let then = then?;
        let file = self.downloads.entry(id).map(|e| e.display_name());
        let request = PowerRequest { action: then.then, queue: String::new(), file, force_close: then.force_close };
        RunWhenDone::new(self.power).run(&request);
        Some(then)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, Downloads, Power};
    use udm_domain::download::Status;
    use udm_domain::queue::WhenDone;

    #[test]
    fn a_finished_download_shuts_down_once_after_the_countdown() {
        let mut e = row(1, Status::Done);
        e.path = Some("/dl/movie.mkv".into());
        e.on_done = Some(OnDone { then: WhenDone::ShutDown, ..OnDone::default() });
        let downloads = Downloads::rows([e]);
        let power = Power::default();
        let after = AfterDownload::new(&downloads, &power);
        assert!(after.run(DownloadId::new(1)).is_some());
        let (request, _) = &power.started()[0];
        assert_eq!((request.action, request.file.as_deref()), (WhenDone::ShutDown, Some("movie.mkv")));
        assert_eq!(after.run(DownloadId::new(1)), None, "it was cleared");
        assert_eq!(power.started().len(), 1);
    }

    #[test]
    fn an_unfinished_download_does_nothing_yet() {
        let mut e = row(1, Status::Downloading);
        e.on_done = Some(OnDone { then: WhenDone::Exit, ..OnDone::default() });
        let downloads = Downloads::rows([e]);
        let power = Power::default();
        assert_eq!(AfterDownload::new(&downloads, &power).run(DownloadId::new(1)), None);
        assert_eq!(power.exits(), 0);
        assert!(downloads.entry(DownloadId::new(1)).unwrap().on_done.is_some(), "it keeps its choice");
    }
}
