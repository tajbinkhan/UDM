//! Checksums that sites publish next to a file, and whether a file's
//! digest matches one. Hashing the file is a port's job (`ports::FileHashes`).

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The hash a checksum was made with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Algorithm {
    Md5,
    Sha1,
    Sha256,
    Sha512,
    Crc32,
}

impl Algorithm {
    pub const ALL: [Algorithm; 5] =
        [Algorithm::Md5, Algorithm::Sha1, Algorithm::Sha256, Algorithm::Sha512, Algorithm::Crc32];

    /// The digest's length in bytes.
    pub fn digest_len(self) -> usize {
        match self {
            Algorithm::Crc32 => 4,
            Algorithm::Md5 => 16,
            Algorithm::Sha1 => 20,
            Algorithm::Sha256 => 32,
            Algorithm::Sha512 => 64,
        }
    }

    /// The name shown to users and written before the digest.
    pub fn name(self) -> &'static str {
        match self {
            Algorithm::Md5 => "md5",
            Algorithm::Sha1 => "sha1",
            Algorithm::Sha256 => "sha256",
            Algorithm::Sha512 => "sha512",
            Algorithm::Crc32 => "crc32",
        }
    }

    /// The algorithm a prefix names: `sha256`, `SHA-256`, `sha_256` and so on.
    fn from_prefix(prefix: &str) -> Option<Algorithm> {
        let plain: String =
            prefix.chars().filter(|c| !matches!(c, '-' | '_' | ' ')).collect::<String>().to_ascii_lowercase();
        Algorithm::ALL.into_iter().find(|a| a.name() == plain)
    }

    /// The only algorithm whose digest has this many hex digits.
    fn from_hex_len(len: usize) -> Option<Algorithm> {
        Algorithm::ALL.into_iter().find(|a| a.digest_len() * 2 == len)
    }
}

impl fmt::Display for Algorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Algorithm::Md5 => "MD5",
            Algorithm::Sha1 => "SHA-1",
            Algorithm::Sha256 => "SHA-256",
            Algorithm::Sha512 => "SHA-512",
            Algorithm::Crc32 => "CRC32",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ChecksumError {
    #[error("the checksum is empty")]
    Empty,
    #[error("{0} is not a checksum algorithm UDM knows")]
    UnknownAlgorithm(String),
    #[error("the checksum has characters that are not hexadecimal digits")]
    NotHex,
    /// No algorithm makes a digest of this many hex digits.
    #[error("no checksum has {0} hexadecimal digits")]
    UnknownLength(usize),
    /// A prefix named the algorithm, but the digest is the wrong length for it.
    #[error("a {algorithm} checksum has {expected} hexadecimal digits, not {found}")]
    WrongLength { algorithm: Algorithm, expected: usize, found: usize },
}

/// A checksum to compare a file with: an algorithm and its digest.
///
/// It is written as `algorithm:hex`, which is how it is stored and sent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Checksum {
    algorithm: Algorithm,
    digest: Vec<u8>,
}

impl Checksum {
    /// Reads a checksum as people paste it: a hex digest whose length names
    /// the algorithm, or one after a prefix such as `sha256:` or `SHA-1=`.
    /// A file name after the digest (the `sha256sum` output) is ignored.
    pub fn parse(text: &str) -> Result<Checksum, ChecksumError> {
        let first = text.split_whitespace().next().ok_or(ChecksumError::Empty)?;
        let (named, hex) = match first.split_once([':', '=']) {
            Some((prefix, hex)) => {
                let algorithm = Algorithm::from_prefix(prefix)
                    .ok_or_else(|| ChecksumError::UnknownAlgorithm(prefix.to_string()))?;
                (Some(algorithm), hex)
            }
            None => (None, first),
        };
        if hex.is_empty() {
            return Err(ChecksumError::Empty);
        }
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ChecksumError::NotHex);
        }
        let algorithm = match named {
            Some(a) if a.digest_len() * 2 != hex.len() => {
                return Err(ChecksumError::WrongLength { algorithm: a, expected: a.digest_len() * 2, found: hex.len() })
            }
            Some(a) => a,
            None => Algorithm::from_hex_len(hex.len()).ok_or(ChecksumError::UnknownLength(hex.len()))?,
        };
        let digest = hex.as_bytes().chunks(2).map(|pair| (hex_value(pair[0]) << 4) | hex_value(pair[1])).collect();
        Ok(Checksum { algorithm, digest })
    }

    pub fn algorithm(&self) -> Algorithm {
        self.algorithm
    }

    /// The digest in lowercase hex.
    pub fn hex(&self) -> String {
        to_hex(&self.digest)
    }

    /// Whether a file's digest, made with this checksum's algorithm, matches.
    pub fn matches(&self, digest: &[u8]) -> bool {
        self.digest == digest
    }
}

impl fmt::Display for Checksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.algorithm.name(), self.hex())
    }
}

impl TryFrom<String> for Checksum {
    type Error = ChecksumError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Checksum::parse(&text)
    }
}

impl From<Checksum> for String {
    fn from(c: Checksum) -> Self {
        c.to_string()
    }
}

/// A digest as lowercase hex, for showing the file's own digest.
pub fn to_hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The value of an ASCII hex digit; `parse` has checked every byte is one.
fn hex_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_length_names_the_algorithm() {
        let cases = [
            ("d87f7e0c", Algorithm::Crc32),
            ("d41d8cd98f00b204e9800998ecf8427e", Algorithm::Md5),
            ("da39a3ee5e6b4b0d3255bfef95601890afd80709", Algorithm::Sha1),
            ("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", Algorithm::Sha256),
            (
                "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
                 47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
                Algorithm::Sha512,
            ),
        ];
        for (hex, algorithm) in cases {
            let c = Checksum::parse(hex).unwrap();
            assert_eq!(c.algorithm(), algorithm, "{hex}");
            assert_eq!(c.hex(), hex);
        }
    }

    #[test]
    fn a_prefix_names_the_algorithm_in_any_spelling() {
        let hex = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        for prefix in ["sha256:", "SHA256:", "SHA-256:", "sha_256=", "Sha-256="] {
            let c = Checksum::parse(&format!("{prefix}{hex}")).unwrap();
            assert_eq!(c.algorithm(), Algorithm::Sha256, "{prefix}");
        }
        let c = Checksum::parse("md5:D41D8CD98F00B204E9800998ECF8427E").unwrap();
        assert_eq!(c.algorithm(), Algorithm::Md5);
        assert_eq!(c.hex(), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn spaces_and_a_file_name_after_the_digest_are_ignored() {
        let line = "  d41d8cd98f00b204e9800998ecf8427e *empty.bin\n";
        assert_eq!(Checksum::parse(line).unwrap().algorithm(), Algorithm::Md5);
    }

    #[test]
    fn bad_checksums_say_why() {
        assert_eq!(Checksum::parse(""), Err(ChecksumError::Empty));
        assert_eq!(Checksum::parse("   "), Err(ChecksumError::Empty));
        assert_eq!(Checksum::parse("sha256:"), Err(ChecksumError::Empty));
        assert_eq!(Checksum::parse("d41d8cd98f00b204e9800998ecf8427g"), Err(ChecksumError::NotHex));
        assert_eq!(Checksum::parse("abc"), Err(ChecksumError::UnknownLength(3)));
        assert_eq!(
            Checksum::parse("blake3:d41d8cd98f00b204e9800998ecf8427e"),
            Err(ChecksumError::UnknownAlgorithm("blake3".into()))
        );
        assert_eq!(
            Checksum::parse("sha1:d41d8cd98f00b204e9800998ecf8427e"),
            Err(ChecksumError::WrongLength { algorithm: Algorithm::Sha1, expected: 40, found: 32 })
        );
    }

    #[test]
    fn matching_compares_the_digest() {
        let c = Checksum::parse("d87f7e0c").unwrap();
        assert!(c.matches(&[0xd8, 0x7f, 0x7e, 0x0c]));
        assert!(!c.matches(&[0xd8, 0x7f, 0x7e, 0x0d]));
    }

    #[test]
    fn it_is_stored_as_algorithm_and_hex() {
        let c = Checksum::parse("D41D8CD98F00B204E9800998ECF8427E").unwrap();
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, "\"md5:d41d8cd98f00b204e9800998ecf8427e\"");
        assert_eq!(serde_json::from_str::<Checksum>(&json).unwrap(), c);
        assert!(serde_json::from_str::<Checksum>("\"nope\"").is_err());
    }
}
