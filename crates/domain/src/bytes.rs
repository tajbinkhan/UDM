//! Counts of bytes: file sizes, how much of a download is in hand, the
//! quota's count, a segment's offsets. `ByteCount` wraps the `u64` the wire
//! and the JSON files carry, and serializes as that number
//! (`serde(transparent)`), so the files and the messages keep their shape.
//! Any number is a valid count.
//!
//! Its arithmetic saturates: a count never wraps below zero or past
//! `u64::MAX`. Speeds (bytes per second) are not counts and stay `u64`.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// A number of bytes, or a byte offset into a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByteCount(u64);

const KIB: u64 = 1024;
const MIB: u64 = 1024 * 1024;

impl ByteCount {
    /// No bytes.
    pub const ZERO: ByteCount = ByteCount(0);

    /// `bytes` bytes.
    pub const fn new(bytes: u64) -> Self {
        ByteCount(bytes)
    }

    /// `kib` kibibytes (the settings' "KB"), saturating.
    pub const fn kib(kib: u64) -> Self {
        ByteCount(kib.saturating_mul(KIB))
    }

    /// `mib` mebibytes (the settings' "MB"), saturating.
    pub const fn mib(mib: u64) -> Self {
        ByteCount(mib.saturating_mul(MIB))
    }

    /// The number, as the wire and the files carry it.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Whole kibibytes in it, rounded down.
    pub const fn whole_kib(self) -> u64 {
        self.0 / KIB
    }

    /// Whole mebibytes in it, rounded down.
    pub const fn whole_mib(self) -> u64 {
        self.0 / MIB
    }

    /// The count as a float, for fractions and rates.
    pub const fn as_f64(self) -> f64 {
        self.0 as f64
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub const fn checked_add(self, other: ByteCount) -> Option<ByteCount> {
        match self.0.checked_add(other.0) {
            Some(n) => Some(ByteCount(n)),
            None => None,
        }
    }

    pub const fn checked_sub(self, other: ByteCount) -> Option<ByteCount> {
        match self.0.checked_sub(other.0) {
            Some(n) => Some(ByteCount(n)),
            None => None,
        }
    }

    pub const fn saturating_add(self, other: ByteCount) -> ByteCount {
        ByteCount(self.0.saturating_add(other.0))
    }

    pub const fn saturating_sub(self, other: ByteCount) -> ByteCount {
        ByteCount(self.0.saturating_sub(other.0))
    }

    /// The count in mebibytes with one decimal, as messages say it:
    /// "2.0 MB".
    pub fn megabytes(self) -> String {
        format!("{:.1} MB", self.0 as f64 / MIB as f64)
    }
}

impl Add for ByteCount {
    type Output = ByteCount;

    /// Saturating, like every count's arithmetic.
    fn add(self, other: ByteCount) -> ByteCount {
        self.saturating_add(other)
    }
}

impl AddAssign for ByteCount {
    fn add_assign(&mut self, other: ByteCount) {
        *self = self.saturating_add(other);
    }
}

impl Sub for ByteCount {
    type Output = ByteCount;

    /// Saturating at zero, like every count's arithmetic.
    fn sub(self, other: ByteCount) -> ByteCount {
        self.saturating_sub(other)
    }
}

impl SubAssign for ByteCount {
    fn sub_assign(&mut self, other: ByteCount) {
        *self = self.saturating_sub(other);
    }
}

impl Sum for ByteCount {
    fn sum<I: Iterator<Item = ByteCount>>(iter: I) -> ByteCount {
        iter.fold(ByteCount::ZERO, Add::add)
    }
}

impl<'a> Sum<&'a ByteCount> for ByteCount {
    fn sum<I: Iterator<Item = &'a ByteCount>>(iter: I) -> ByteCount {
        iter.copied().sum()
    }
}

impl From<u64> for ByteCount {
    fn from(bytes: u64) -> Self {
        ByteCount(bytes)
    }
}

impl From<ByteCount> for u64 {
    fn from(bytes: ByteCount) -> Self {
        bytes.0
    }
}

impl fmt::Display for ByteCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_plain_numbers_in_json() {
        assert_eq!(serde_json::to_string(&ByteCount::new(1_048_576)).unwrap(), "1048576");
        assert_eq!(serde_json::from_str::<ByteCount>("42").unwrap(), ByteCount::new(42));
        assert_eq!(serde_json::to_string(&Some(ByteCount::ZERO)).unwrap(), "0");
        assert_eq!(serde_json::from_str::<Option<ByteCount>>("null").unwrap(), None);
        assert_eq!(serde_json::from_str::<Vec<(u64, ByteCount)>>("[[5,7]]").unwrap(), vec![(5, ByteCount::new(7))]);
    }

    #[test]
    fn arithmetic_saturates() {
        let (a, b) = (ByteCount::new(5), ByteCount::new(7));
        assert_eq!(a - b, ByteCount::ZERO);
        assert_eq!(b - a, ByteCount::new(2));
        assert_eq!(ByteCount::new(u64::MAX) + a, ByteCount::new(u64::MAX));
        assert_eq!(a.checked_sub(b), None);
        assert_eq!(a.checked_add(b), Some(ByteCount::new(12)));
        let mut c = a;
        c += b;
        c -= ByteCount::new(2);
        assert_eq!(c, ByteCount::new(10));
        assert_eq!([a, b].iter().sum::<ByteCount>(), ByteCount::new(12));
    }

    #[test]
    fn units_and_words() {
        assert_eq!(ByteCount::kib(2), ByteCount::new(2048));
        assert_eq!(ByteCount::mib(3).whole_mib(), 3);
        assert_eq!(ByteCount::new(2047).whole_kib(), 1);
        assert_eq!(ByteCount::mib(2).megabytes(), "2.0 MB");
        assert_eq!(ByteCount::new(7).to_string(), "7");
        assert_eq!(ByteCount::mib(u64::MAX), ByteCount::new(u64::MAX));
    }
}
