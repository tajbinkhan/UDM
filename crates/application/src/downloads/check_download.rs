use std::path::Path;

use udm_domain::checksum::to_hex;
use udm_domain::download::Status;
use udm_domain::id::DownloadId;
use udm_domain::ports::{DownloadList, FileHashes};

/// What checking a finished download against its checksum found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// It has no checksum, or it is not a finished file.
    Unchecked,
    Matches,
    /// The file is not the one the checksum names, or could not be read;
    /// the download is now failed with the reason.
    Failed(String),
}

/// Compare a finished download with the checksum it was added with, and
/// fail it when the file does not match. It reads the whole file, so the
/// daemon runs it on a thread of its own.
pub struct CheckDownload<'a, D, H: ?Sized> {
    downloads: &'a D,
    hashes: &'a H,
}

impl<'a, D: DownloadList, H: FileHashes + ?Sized> CheckDownload<'a, D, H> {
    pub fn new(downloads: &'a D, hashes: &'a H) -> Self {
        CheckDownload { downloads, hashes }
    }

    /// Whether download `id` is a finished file with a checksum to check.
    pub fn wanted(&self, id: DownloadId) -> bool {
        self.downloads.entry(id).is_some_and(|e| e.status == Status::Done && e.path.is_some() && e.checksum.is_some())
    }

    pub fn run(&self, id: DownloadId) -> CheckOutcome {
        let Some(entry) = self.downloads.entry(id).filter(|e| e.status == Status::Done) else {
            return CheckOutcome::Unchecked;
        };
        let (Some(path), Some(checksum)) = (entry.path, entry.checksum) else {
            return CheckOutcome::Unchecked;
        };
        let error = match self.hashes.digest(Path::new(&path), checksum.algorithm()) {
            Ok(digest) if checksum.matches(&digest) => return CheckOutcome::Matches,
            Ok(digest) => format!(
                "The file does not match its {} checksum: expected {}, got {}",
                checksum.algorithm(),
                checksum.hex(),
                to_hex(&digest)
            ),
            Err(e) => format!("Could not check the file's {} checksum: {e}", checksum.algorithm()),
        };
        self.downloads.update(id, |e| {
            e.status = Status::Failed;
            e.error = Some(error.clone());
            true
        });
        CheckOutcome::Failed(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, Downloads, Hashes};
    use udm_domain::checksum::Checksum;

    fn finished(checksum: Option<&str>) -> Downloads {
        let mut e = row(1, Status::Done);
        e.path = Some("/dl/a.iso".into());
        e.checksum = checksum.map(|c| Checksum::parse(c).unwrap());
        Downloads::rows([e])
    }

    #[test]
    fn a_matching_file_stays_done() {
        let downloads = finished(Some("crc32:d87f7e0c"));
        let hashes = Hashes::with("/dl/a.iso", vec![0xd8, 0x7f, 0x7e, 0x0c]);
        let check = CheckDownload::new(&downloads, &hashes);
        assert!(check.wanted(DownloadId::new(1)));
        assert_eq!(check.run(DownloadId::new(1)), CheckOutcome::Matches);
        assert_eq!(downloads.entry(DownloadId::new(1)).unwrap().status, Status::Done);
    }

    #[test]
    fn a_mismatch_fails_the_download_and_says_why() {
        let downloads = finished(Some("crc32:d87f7e0c"));
        let hashes = Hashes::with("/dl/a.iso", vec![1, 2, 3, 4]);
        let outcome = CheckDownload::new(&downloads, &hashes).run(DownloadId::new(1));
        assert!(matches!(outcome, CheckOutcome::Failed(_)));
        let e = downloads.entry(DownloadId::new(1)).unwrap();
        assert_eq!(e.status, Status::Failed);
        assert_eq!(
            e.error.as_deref(),
            Some("The file does not match its CRC32 checksum: expected d87f7e0c, got 01020304")
        );
    }

    #[test]
    fn an_unreadable_file_fails_the_download() {
        let downloads = finished(Some("crc32:d87f7e0c"));
        let hashes = Hashes::default();
        let outcome = CheckDownload::new(&downloads, &hashes).run(DownloadId::new(1));
        assert!(matches!(outcome, CheckOutcome::Failed(_)));
        assert_eq!(downloads.entry(DownloadId::new(1)).unwrap().status, Status::Failed);
    }

    #[test]
    fn without_a_checksum_nothing_is_checked() {
        let downloads = finished(None);
        let hashes = Hashes::default();
        let check = CheckDownload::new(&downloads, &hashes);
        assert!(!check.wanted(DownloadId::new(1)));
        assert_eq!(check.run(DownloadId::new(1)), CheckOutcome::Unchecked);
        assert_eq!(downloads.entry(DownloadId::new(1)).unwrap().status, Status::Done);
    }
}
