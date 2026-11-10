//! File digests for checking a download against a published checksum.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use pullbay_domain::checksum::Algorithm;
use pullbay_domain::ports::{FileHashes, HashError};

/// Read the file this many bytes at a time.
const CHUNK: usize = 1 << 20;

/// Hashes files on disk.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiskHashes;

impl FileHashes for DiskHashes {
    fn digest(&self, path: &Path, algorithm: Algorithm) -> Result<Vec<u8>, HashError> {
        let file = File::open(path).map_err(|e| unreadable(path, &e))?;
        digest_of(file, algorithm).map_err(|e| unreadable(path, &e))
    }
}

fn unreadable(path: &Path, e: &io::Error) -> HashError {
    HashError(format!("Could not read {}: {e}", path.display()))
}

/// The running state of one of the hashes.
enum Hasher {
    Md5(md5::Md5),
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
    Crc32(crc32fast::Hasher),
}

impl Hasher {
    fn new(algorithm: Algorithm) -> Hasher {
        match algorithm {
            Algorithm::Md5 => Hasher::Md5(md5::Digest::new()),
            Algorithm::Sha1 => Hasher::Sha1(sha1::Digest::new()),
            Algorithm::Sha256 => Hasher::Sha256(sha2::Digest::new()),
            Algorithm::Sha512 => Hasher::Sha512(sha2::Digest::new()),
            Algorithm::Crc32 => Hasher::Crc32(crc32fast::Hasher::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Hasher::Md5(h) => md5::Digest::update(h, bytes),
            Hasher::Sha1(h) => sha1::Digest::update(h, bytes),
            Hasher::Sha256(h) => sha2::Digest::update(h, bytes),
            Hasher::Sha512(h) => sha2::Digest::update(h, bytes),
            Hasher::Crc32(h) => h.update(bytes),
        }
    }

    fn finish(self) -> Vec<u8> {
        match self {
            Hasher::Md5(h) => md5::Digest::finalize(h).to_vec(),
            Hasher::Sha1(h) => sha1::Digest::finalize(h).to_vec(),
            Hasher::Sha256(h) => sha2::Digest::finalize(h).to_vec(),
            Hasher::Sha512(h) => sha2::Digest::finalize(h).to_vec(),
            Hasher::Crc32(h) => h.finalize().to_be_bytes().to_vec(),
        }
    }
}

/// The digest of everything `reader` holds.
pub fn digest_of(mut reader: impl Read, algorithm: Algorithm) -> io::Result<Vec<u8>> {
    let mut hasher = Hasher::new(algorithm);
    let mut buf = vec![0u8; CHUNK];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(hasher.finish()),
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pullbay_domain::checksum::{to_hex, Checksum};

    /// The well-known digests of "The quick brown fox jumps over the lazy dog".
    const FOX: &[u8] = b"The quick brown fox jumps over the lazy dog";

    fn hex(algorithm: Algorithm, data: &[u8]) -> String {
        to_hex(&digest_of(data, algorithm).unwrap())
    }

    #[test]
    fn each_algorithm_gives_its_known_digest() {
        assert_eq!(hex(Algorithm::Md5, FOX), "9e107d9d372bb6826bd81d3542a419d6");
        assert_eq!(hex(Algorithm::Sha1, FOX), "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12");
        assert_eq!(hex(Algorithm::Sha256, FOX), "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592");
        assert_eq!(
            hex(Algorithm::Sha512, FOX),
            "07e547d9586f6a73f73fbac0435ed76951218fb7d0c8d788a309d785436bbb64\
             2e93a252a954f23912547d1e8a3b5ed6e1bfd7097821233fa0538f3db854fee6"
        );
        assert_eq!(hex(Algorithm::Crc32, FOX), "414fa339");
    }

    #[test]
    fn a_file_on_disk_matches_its_checksum() {
        let dir = std::env::temp_dir().join(format!("pullbay-hash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fox.txt");
        std::fs::write(&path, FOX).unwrap();
        let sum = Checksum::parse("sha1:2fd4e1c67a2d28fced849ee1bb76e7391b93eb12").unwrap();
        let digest = DiskHashes.digest(&path, sum.algorithm()).unwrap();
        assert!(sum.matches(&digest));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_file_is_an_error() {
        let missing = std::env::temp_dir().join("pullbay-hash-no-such-file.bin");
        assert!(DiskHashes.digest(&missing, Algorithm::Md5).is_err());
    }

    #[test]
    fn reading_in_chunks_gives_the_same_digest() {
        let big: Vec<u8> = (0..(CHUNK * 2 + 7)).map(|i| i as u8).collect();
        let whole = {
            use sha2::Digest;
            sha2::Sha256::digest(&big).to_vec()
        };
        assert_eq!(digest_of(big.as_slice(), Algorithm::Sha256).unwrap(), whole);
    }
}
