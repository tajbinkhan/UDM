use udm_domain::id::DownloadId;
use udm_domain::ports::{Converter, DownloadControl, DownloadList};

/// How [`CancelDownload`] stopped a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Canceled {
    /// A conversion stops; its row ends as canceled when ffmpeg exits.
    Conversion,
    /// A download that waited for its save-as dialog or in a queue.
    Waiting,
    /// A download the engine had.
    Running,
}

/// Cancel a row: a conversion, a download waiting for its dialog or in a
/// queue, or one the engine runs.
pub struct CancelDownload<'a, D, X, C> {
    downloads: &'a D,
    control: &'a X,
    converter: &'a C,
}

impl<'a, D: DownloadList, X: DownloadControl, C: Converter> CancelDownload<'a, D, X, C> {
    pub fn new(downloads: &'a D, control: &'a X, converter: &'a C) -> Self {
        CancelDownload { downloads, control, converter }
    }

    pub fn run(&self, id: DownloadId) -> Canceled {
        if self.converter.stop(id) {
            return Canceled::Conversion;
        }
        if self.downloads.status(id).is_some_and(|s| s.waits()) {
            self.control.cancel_waiting(id);
            return Canceled::Waiting;
        }
        self.control.stop(id);
        Canceled::Running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Control, Convert, Downloads};
    use udm_domain::download::Status;

    #[test]
    fn each_kind_of_row_stops_its_own_way() {
        let downloads = Downloads::with(&[(1, Status::Confirming), (2, Status::Waiting), (3, Status::Downloading)]);
        let (control, converter) = (Control::default(), Convert::running(&[9]));
        let cancel = CancelDownload::new(&downloads, &control, &converter);
        assert_eq!(cancel.run(DownloadId::new(9)), Canceled::Conversion);
        assert_eq!(cancel.run(DownloadId::new(1)), Canceled::Waiting);
        assert_eq!(cancel.run(DownloadId::new(2)), Canceled::Waiting);
        assert_eq!(cancel.run(DownloadId::new(3)), Canceled::Running);
        assert_eq!(converter.stopped(), [9]);
        assert_eq!(control.canceled_waiting(), [1, 2]);
        assert_eq!(control.stopped(), [3]);
    }
}
