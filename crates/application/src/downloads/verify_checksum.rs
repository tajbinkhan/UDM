use std::path::Path;

use pullbay_domain::checksum::{to_hex, Checksum};
use pullbay_domain::ports::{FileHashes, HashError};

/// What comparing a file with a checksum found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// Whether the file's digest is the checksum's.
    pub matches: bool,
    /// The file's own digest in lowercase hex, made with the checksum's
    /// algorithm, for showing next to the one that was pasted.
    pub actual: String,
}

/// Compare a file with a checksum a site published. It reads the whole
/// file, so the window runs it on a worker.
pub struct VerifyChecksum<'a, H: ?Sized> {
    hashes: &'a H,
}

impl<'a, H: FileHashes + ?Sized> VerifyChecksum<'a, H> {
    pub fn new(hashes: &'a H) -> Self {
        VerifyChecksum { hashes }
    }

    pub fn run(&self, path: &Path, checksum: &Checksum) -> Result<Verified, HashError> {
        let digest = self.hashes.digest(path, checksum.algorithm())?;
        Ok(Verified { matches: checksum.matches(&digest), actual: to_hex(&digest) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Hashes;

    #[test]
    fn a_file_whose_digest_is_the_checksum_matches() {
        let hashes = Hashes::with("/dl/a.iso", vec![0xd8, 0x7f, 0x7e, 0x0c]);
        let sum = Checksum::parse("crc32:D87F7E0C").unwrap();
        let verified = VerifyChecksum::new(&hashes).run(Path::new("/dl/a.iso"), &sum).unwrap();
        assert_eq!(verified, Verified { matches: true, actual: "d87f7e0c".into() });
    }

    #[test]
    fn a_different_digest_does_not_match_and_is_shown() {
        let hashes = Hashes::with("/dl/a.iso", vec![1, 2, 3, 4]);
        let sum = Checksum::parse("d87f7e0c").unwrap();
        let verified = VerifyChecksum::new(&hashes).run(Path::new("/dl/a.iso"), &sum).unwrap();
        assert_eq!(verified, Verified { matches: false, actual: "01020304".into() });
    }

    #[test]
    fn an_unreadable_file_is_an_error() {
        let hashes = Hashes::default();
        let sum = Checksum::parse("d87f7e0c").unwrap();
        assert!(VerifyChecksum::new(&hashes).run(Path::new("/dl/gone.iso"), &sum).is_err());
    }
}
