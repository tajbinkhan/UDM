//! Packages: downloads kept together under one name and one folder, as the
//! link picker adds them, and the archives among their files.
//!
//! A package lists its downloads in the order they were added, like a
//! queue lists its own. Volumes of one split archive added together become
//! one package of their own (see [`plan_packages`]), so they are unpacked
//! together once every part is in.
//!
//! The archive rules here are pure: [`volume`] reads what a file name says
//! about the archive it belongs to, [`archive_sets`] groups names into
//! archives with their parts in order and the parts that are missing, and
//! [`entry_path`] decides where, under the folder being unpacked to, an
//! archive's entry may be written. Unpacking itself is an adapter's work
//! (the [`Unpacker`](crate::ports::Unpacker) port).
//!
//! Split archives come in three families:
//!
//! - **parts**, RAR's newer naming: `movie.part1.rar`, `movie.part2.rar`
//!   (or `part01`, `part001`). The first part is opened.
//! - **classic**, an archive with numbered companions: RAR's older
//!   `movie.rar` with `movie.r00`, `movie.r01`, ..., and a spanned zip's
//!   `movie.zip` with `movie.z01`, `movie.z02`, ... The `.rar` or `.zip`
//!   is opened.
//! - **pieces**, an archive cut into pieces that joined make it again:
//!   `backup.7z.001`, `backup.7z.002`, or `.zip.001`, `.rar.001`,
//!   `.tar.001`. The first piece is opened, or the pieces are joined.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::bytes::ByteCount;
use crate::download::Status;
use crate::filename::{not_a_device, sanitize_filename, FileName};
use crate::id::{DownloadId, PackageId};

/// The longest package name, in characters.
pub const MAX_NAME: usize = 120;
/// The most files one archive may unpack, so an archive of a million empty
/// files cannot bury the folder.
pub const MAX_FILES: u64 = 100_000;
/// Room left free on the disk after unpacking, so unpacking never fills it.
pub const SPACE_MARGIN: ByteCount = ByteCount::mib(64);

/// Why text is not a package name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackageError {
    #[error("The package needs a name.")]
    EmptyName,
    #[error("A package name may be at most {max} characters long.", max = MAX_NAME)]
    LongName,
    /// Nothing is left of it as a folder name.
    #[error("{0} cannot be used as a folder name.")]
    NotAFolder(String),
}

/// A package's name, trimmed, with the folder name it gives.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PackageName {
    text: String,
    folder: FileName,
}

impl PackageName {
    /// `name` as a package name: trimmed, not empty, at most [`MAX_NAME`]
    /// characters, and usable as a folder name once cleaned like a file
    /// name.
    pub fn parse(name: &str) -> Result<PackageName, PackageError> {
        let text = name.trim();
        if text.is_empty() {
            return Err(PackageError::EmptyName);
        }
        if text.chars().count() > MAX_NAME {
            return Err(PackageError::LongName);
        }
        let folder = sanitize_filename(text).ok_or_else(|| PackageError::NotAFolder(text.to_string()))?;
        Ok(PackageName { text: text.to_string(), folder })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The name as a folder's name: characters Windows forbids become `_`.
    pub fn folder_name(&self) -> &FileName {
        &self.folder
    }
}

impl TryFrom<String> for PackageName {
    type Error = PackageError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        PackageName::parse(&name)
    }
}

impl From<PackageName> for String {
    fn from(n: PackageName) -> Self {
        n.text
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// Downloads kept together under one name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Package {
    pub id: PackageId,
    pub name: PackageName,
    /// The folder its files were all sent to, when they were; `None` leaves
    /// each in its own category's folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Its downloads, in the order they were added.
    pub downloads: Vec<DownloadId>,
    /// How far unpacking its archives got.
    #[serde(default)]
    pub unpack: Unpack,
}

impl Package {
    /// A new package, not unpacked.
    pub fn new(id: PackageId, name: PackageName, folder: Option<String>, downloads: Vec<DownloadId>) -> Package {
        Package { id, name, folder, downloads, unpack: Unpack::Waiting }
    }

    /// Whether it can be unpacked now: not being unpacked, and not done.
    pub fn can_unpack(&self) -> bool {
        !matches!(self.unpack, Unpack::Unpacking | Unpack::Done { .. })
    }
}

/// How far unpacking a package's archives got.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Unpack {
    /// Not unpacked: waiting for its files, or for the user.
    #[default]
    Waiting,
    /// Being unpacked now.
    Unpacking,
    /// Its archives are encrypted, and the user is asked for the password;
    /// `wrong` when the last one given did not open them.
    NeedsPassword {
        #[serde(default)]
        wrong: bool,
    },
    /// Unpacked: `files` files went into `folder`.
    Done { folder: String, files: u64 },
    /// It could not be unpacked, and why.
    Failed { reason: UnpackFailure },
}

impl Unpack {
    /// The state a package is in again after the daemon restarts: one being
    /// unpacked when it stopped waits again (what was unpacked stays).
    pub fn after_restart(self) -> Unpack {
        match self {
            Unpack::Unpacking => Unpack::Waiting,
            other => other,
        }
    }
}

/// Why a package's archives could not be unpacked. The window shows these,
/// translated; they travel on the wire and in `history.json` by their
/// `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UnpackFailure {
    #[error("There are no archives among its files.")]
    NoArchives,
    #[error("Not all of its archives are downloaded yet.")]
    NotComplete,
    #[error("Part {part} of {archive} is missing.")]
    MissingPart { archive: String, part: u32 },
    #[error("{archive} needs 7-Zip, which was not found. Install 7-Zip, or set where it is in Options.")]
    NeedsSevenZip { archive: String },
    #[error("{archive} holds a file that would be written outside the folder: {entry}")]
    Unsafe { archive: String, entry: String },
    #[error("{archive} holds more than {max} files.")]
    TooManyFiles { archive: String, max: u64 },
    #[error("Unpacking needs {needed} bytes, but only {free} bytes are free.")]
    NoSpace { needed: ByteCount, free: ByteCount },
    #[error("{archive} is damaged, or not an archive: {detail}")]
    Damaged { archive: String, detail: String },
    /// A file could not be read or written; the system says why.
    #[error("{archive}: {detail}")]
    Io { archive: String, detail: String },
}

/// What unpacking one archive came to, as the unpacker reports it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UnpackError {
    /// It is encrypted, and no password was given.
    #[error("The archive needs a password.")]
    NeedsPassword,
    /// The password given does not open it.
    #[error("The password is wrong.")]
    WrongPassword,
    #[error(transparent)]
    Failed(#[from] UnpackFailure),
}

/// A password for an encrypted archive. It is never written down: it has no
/// `Serialize`, and its `Debug` hides it, so a log line cannot show it.
#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    /// `text` as a password; `None` when it is empty.
    pub fn new(text: String) -> Option<Password> {
        (!text.is_empty()).then_some(Password(text))
    }

    /// The password itself, for the unpacker.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(..)")
    }
}

/// How many downloads added together become packages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Packaging {
    /// None: the downloads stand alone.
    #[default]
    None,
    /// Each split archive among them becomes a package of its own.
    Archives,
    /// All of them become one package, saved in `folder` when it is given.
    One { name: PackageName, folder: Option<String> },
}

/// The packages downloads added together make: for each, its name and the
/// indexes of its downloads among `names`, the downloads' file names.
pub fn plan_packages(names: &[&str], packaging: &Packaging) -> Vec<(PackageName, Vec<usize>)> {
    match packaging {
        Packaging::None => Vec::new(),
        Packaging::One { name, .. } if !names.is_empty() => vec![(name.clone(), (0..names.len()).collect())],
        Packaging::One { .. } => Vec::new(),
        Packaging::Archives => archive_sets(names)
            .into_iter()
            .filter(|set| set.parts.len() > 1)
            .filter_map(|set| Some((PackageName::parse(&set.base).ok()?, set.parts)))
            .collect(),
    }
}

/// Whether a package with downloads in these states has all its files: none
/// is missing and every one is finished.
pub fn complete(statuses: &[Option<Status>]) -> bool {
    !statuses.is_empty() && statuses.iter().all(|s| *s == Some(Status::Done))
}

/// The kinds of archive UDM knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveKind {
    Zip,
    SevenZip,
    Rar,
    Tar,
    TarGz,
    TarXz,
    TarBz2,
    Gzip,
    Xz,
    Bzip2,
}

impl ArchiveKind {
    /// UDM unpacks it itself; the others need 7-Zip.
    pub fn built_in(self) -> bool {
        matches!(self, ArchiveKind::Zip | ArchiveKind::Tar | ArchiveKind::TarGz | ArchiveKind::TarXz)
            || matches!(self, ArchiveKind::Gzip | ArchiveKind::Xz)
    }

    /// The kind an ending names (lowercase, without its dot), and how many
    /// of the name's last dotted parts it takes: `tar.gz` takes two.
    fn from_endings(last: &str, before: Option<&str>) -> Option<(ArchiveKind, usize)> {
        let tar = before == Some("tar");
        Some(match last {
            "zip" => (ArchiveKind::Zip, 1),
            "7z" => (ArchiveKind::SevenZip, 1),
            "rar" => (ArchiveKind::Rar, 1),
            "tar" => (ArchiveKind::Tar, 1),
            "tgz" => (ArchiveKind::TarGz, 1),
            "txz" => (ArchiveKind::TarXz, 1),
            "tbz" | "tbz2" => (ArchiveKind::TarBz2, 1),
            "gz" if tar => (ArchiveKind::TarGz, 2),
            "xz" if tar => (ArchiveKind::TarXz, 2),
            "bz2" if tar => (ArchiveKind::TarBz2, 2),
            "gz" => (ArchiveKind::Gzip, 1),
            "xz" => (ArchiveKind::Xz, 1),
            "bz2" => (ArchiveKind::Bzip2, 1),
            _ => return None,
        })
    }
}

/// How a split archive's parts are named (see the module's notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// `name.part1.rar`, `name.part2.rar`, ...
    Parts,
    /// One file (`name.zip`), or one with numbered companions (`name.z01`,
    /// `name.r00`).
    Classic,
    /// `name.7z.001`, `name.7z.002`, ...: pieces that joined make the
    /// archive.
    Pieces,
}

/// What a file's name says about the archive it is part of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// The archive's name without its endings, as written: `movie` for
    /// `movie.part2.rar`.
    pub base: String,
    pub kind: ArchiveKind,
    pub family: Family,
    /// Its place among the parts. Parts and pieces count as named (1, 2,
    /// ...); in the classic family the archive itself is 0, `.r00` is 1,
    /// `.r01` 2, and `.z01` is 1.
    pub number: u32,
}

/// What `file_name` says about the archive it belongs to, or `None` when it
/// is not an archive or part of one.
pub fn volume(file_name: &str) -> Option<Volume> {
    let lower = file_name.to_lowercase();
    let parts: Vec<&str> = lower.split('.').collect();
    // The name before the endings, as written, from the lowercase name's
    // byte length (lowercasing keeps ASCII endings' lengths).
    let base_of = |endings: usize| -> Option<String> {
        let tail: usize = parts[parts.len() - endings..].iter().map(|p| p.len() + 1).sum();
        let cut = file_name.len().checked_sub(tail)?;
        let base = file_name.get(..cut)?;
        (!base.is_empty()).then(|| base.to_string())
    };
    let last = *parts.last()?;
    if parts.len() < 2 {
        return None;
    }
    let before = parts.get(parts.len().wrapping_sub(2)).copied().filter(|_| parts.len() > 2);
    // Pieces: name.7z.001.
    if last.len() == 3 && last.bytes().all(|b| b.is_ascii_digit()) && parts.len() > 2 {
        let ext = parts[parts.len() - 2];
        let kind = match ext {
            "7z" => ArchiveKind::SevenZip,
            "zip" => ArchiveKind::Zip,
            "rar" => ArchiveKind::Rar,
            "tar" => ArchiveKind::Tar,
            _ => return None,
        };
        let number = last.parse().ok()?;
        return Some(Volume { base: base_of(2)?, kind, family: Family::Pieces, number });
    }
    // Classic companions: name.r00 .. name.r99, name.z01 .. name.z99.
    if last.len() == 3 && last[1..].bytes().all(|b| b.is_ascii_digit()) {
        let n: u32 = last[1..].parse().ok()?;
        let (kind, number) = match last.as_bytes()[0] {
            b'r' => (ArchiveKind::Rar, n + 1),
            b'z' if n > 0 => (ArchiveKind::Zip, n),
            _ => return None,
        };
        return Some(Volume { base: base_of(1)?, kind, family: Family::Classic, number });
    }
    let (kind, endings) = ArchiveKind::from_endings(last, before)?;
    // Parts: name.part1.rar.
    if kind == ArchiveKind::Rar {
        if let Some(n) = before.and_then(|b| b.strip_prefix("part")) {
            if !n.is_empty() && n.len() <= 4 && n.bytes().all(|b| b.is_ascii_digit()) {
                let number = n.parse().ok()?;
                return Some(Volume { base: base_of(2)?, kind, family: Family::Parts, number });
            }
        }
    }
    Some(Volume { base: base_of(endings)?, kind, family: Family::Classic, number: 0 })
}

/// One archive among a list of file names: its parts in order, the one to
/// open first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveSet {
    /// The archive's name without its endings, as its first part writes it.
    pub base: String,
    pub kind: ArchiveKind,
    pub family: Family,
    /// Indexes into the names given, in part order: the first is the part
    /// to open.
    pub parts: Vec<usize>,
    /// The numbers of parts that should be there but are not.
    pub missing: Vec<u32>,
}

impl ArchiveSet {
    /// UDM can unpack it itself: a built-in kind in one file, or cut
    /// into pieces it can join. A spanned zip and RAR need 7-Zip.
    pub fn built_in(&self) -> bool {
        self.kind.built_in() && (self.family == Family::Pieces || self.parts.len() == 1)
    }

    /// The name of the part opened first, for messages.
    pub fn first_name<'a>(&self, names: &[&'a str]) -> &'a str {
        self.parts.first().and_then(|&i| names.get(i)).copied().unwrap_or("")
    }
}

/// What tells one archive's parts from another's: the lowercase name
/// without endings, the kind and the family.
type SetKey = (String, ArchiveKind, Family);

/// The archives among `names` (file names, not paths), in the order their
/// first part appears. Names that are not archives are left out.
pub fn archive_sets(names: &[&str]) -> Vec<ArchiveSet> {
    let mut order: Vec<SetKey> = Vec::new();
    // Each set's parts: number, index into `names`, and its name as written.
    let mut found: HashMap<SetKey, Vec<(u32, usize, String)>> = HashMap::new();
    for (i, name) in names.iter().enumerate() {
        let Some(v) = volume(name) else { continue };
        let key = (v.base.to_lowercase(), v.kind, v.family);
        let list = found.entry(key.clone()).or_default();
        if list.is_empty() {
            order.push(key);
        }
        // A repeated part (the same file listed twice) counts once.
        if !list.iter().any(|(n, ..)| *n == v.number) {
            list.push((v.number, i, v.base));
        }
    }
    order
        .into_iter()
        .filter_map(|key| {
            let mut list = found.remove(&key)?;
            list.sort_by_key(|(n, ..)| *n);
            let numbers: Vec<u32> = list.iter().map(|(n, ..)| *n).collect();
            let (_, kind, family) = key;
            let first = match family {
                // The archive itself is 0; parts and pieces start at 1, or
                // at 0 when that is how they were numbered.
                Family::Classic => 0,
                Family::Parts | Family::Pieces => numbers.first().copied().unwrap_or(1).min(1),
            };
            let last = numbers.last().copied().unwrap_or(first);
            let missing = (first..=last).filter(|n| !numbers.contains(n)).collect();
            let base = list.first().map(|(_, _, b)| b.clone()).unwrap_or_default();
            Some(ArchiveSet { base, kind, family, parts: list.into_iter().map(|(_, i, _)| i).collect(), missing })
        })
        .collect()
}

/// Where, under the folder being unpacked to, an archive's entry `name` may
/// be written, as the folders and file name to join: `None` when it would
/// leave that folder (an absolute path, a drive or `..`) or holds a NUL.
/// Both `/` and `\` separate folders, empty and `.` parts are dropped, and
/// a part Windows would take for a device gets a `_` in front.
pub fn entry_path(name: &str) -> Option<Vec<FileName>> {
    if name.is_empty() || name.contains('\0') || name.starts_with(['/', '\\']) {
        return None;
    }
    let mut parts = Vec::new();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return None,
            // `C:`, `C:file` and NTFS streams (`file:stream`).
            p if p.contains(':') => return None,
            p => parts.push(FileName::parse(&not_a_device(p.to_string())).ok()?),
        }
    }
    (!parts.is_empty()).then_some(parts)
}

/// Whether unpacking `needed` bytes fits in `free`, leaving
/// [`SPACE_MARGIN`].
pub fn check_space(needed: ByteCount, free: ByteCount) -> Result<(), UnpackFailure> {
    if needed.saturating_add(SPACE_MARGIN) > free {
        return Err(UnpackFailure::NoSpace { needed, free });
    }
    Ok(())
}

/// The first of `name`, `name (1)`, `name (2)`, ... that `taken` says is
/// free, so an unpacked file never replaces one already there.
pub fn free_name(name: &FileName, taken: impl Fn(&FileName) -> bool) -> FileName {
    (0..u32::MAX).map(|n| name.numbered(n)).find(|n| !taken(n)).unwrap_or_else(|| name.numbered(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(name: &str) -> Option<(String, ArchiveKind, Family, u32)> {
        volume(name).map(|v| (v.base, v.kind, v.family, v.number))
    }

    fn some(base: &str, kind: ArchiveKind, family: Family, number: u32) -> Option<(String, ArchiveKind, Family, u32)> {
        Some((base.to_string(), kind, family, number))
    }

    #[test]
    fn single_archives_are_read_from_their_endings() {
        use ArchiveKind::*;
        assert_eq!(v("Holiday Photos.zip"), some("Holiday Photos", Zip, Family::Classic, 0));
        assert_eq!(v("x.7z"), some("x", SevenZip, Family::Classic, 0));
        assert_eq!(v("x.RAR"), some("x", Rar, Family::Classic, 0));
        assert_eq!(v("src-1.2.tar"), some("src-1.2", Tar, Family::Classic, 0));
        assert_eq!(v("src-1.2.tar.gz"), some("src-1.2", TarGz, Family::Classic, 0));
        assert_eq!(v("src.TGZ"), some("src", TarGz, Family::Classic, 0));
        assert_eq!(v("src.tar.xz"), some("src", TarXz, Family::Classic, 0));
        assert_eq!(v("src.txz"), some("src", TarXz, Family::Classic, 0));
        assert_eq!(v("src.tar.bz2"), some("src", TarBz2, Family::Classic, 0));
        assert_eq!(v("log.gz"), some("log", Gzip, Family::Classic, 0));
        assert_eq!(v("log.xz"), some("log", Xz, Family::Classic, 0));
        assert_eq!(v("log.bz2"), some("log", Bzip2, Family::Classic, 0));
        for not in ["movie.mkv", "zip", ".zip", "readme", "a.z00", "a.exe.001", "notes.r0"] {
            assert_eq!(v(not), None, "{not}");
        }
    }

    #[test]
    fn split_archives_are_read_in_every_family() {
        use ArchiveKind::*;
        assert_eq!(v("Movie.part1.rar"), some("Movie", Rar, Family::Parts, 1));
        assert_eq!(v("Movie.part01.rar"), some("Movie", Rar, Family::Parts, 1));
        assert_eq!(v("movie.PART012.rar"), some("movie", Rar, Family::Parts, 12));
        assert_eq!(v("movie.r00"), some("movie", Rar, Family::Classic, 1));
        assert_eq!(v("movie.r11"), some("movie", Rar, Family::Classic, 12));
        assert_eq!(v("site.z01"), some("site", Zip, Family::Classic, 1));
        assert_eq!(v("backup.7z.001"), some("backup", SevenZip, Family::Pieces, 1));
        assert_eq!(v("backup.zip.002"), some("backup", Zip, Family::Pieces, 2));
        assert_eq!(v("backup.tar.010"), some("backup", Tar, Family::Pieces, 10));
        // A dot in the name is kept.
        assert_eq!(v("v1.2.part3.rar"), some("v1.2", Rar, Family::Parts, 3));
        // `part` without a number is part of the name.
        assert_eq!(v("spare.part.rar"), some("spare.part", Rar, Family::Classic, 0));
    }

    fn sets(names: &[&str]) -> Vec<(String, Vec<usize>, Vec<u32>)> {
        archive_sets(names).into_iter().map(|s| (s.base, s.parts, s.missing)).collect()
    }

    #[test]
    fn parts_of_one_archive_group_in_order_with_the_first_to_open() {
        let names = ["m.part2.rar", "readme.txt", "m.part1.rar", "m.part3.rar"];
        assert_eq!(sets(&names), [("m".to_string(), vec![2, 0, 3], vec![])]);
        let old = ["m.r01", "m.rar", "m.r00"];
        assert_eq!(sets(&old), [("m".to_string(), vec![1, 2, 0], vec![])]);
        let spanned = ["s.z02", "s.zip", "s.z01"];
        let set = &archive_sets(&spanned)[0];
        assert_eq!((set.parts.clone(), set.family, set.kind), (vec![1, 2, 0], Family::Classic, ArchiveKind::Zip));
        assert!(!set.built_in(), "a spanned zip needs 7-Zip");
        let pieces = ["b.7z.002", "b.7z.001"];
        assert_eq!(sets(&pieces), [("b".to_string(), vec![1, 0], vec![])]);
        let zip_pieces = archive_sets(&["c.zip.001", "c.zip.002"]);
        assert!(zip_pieces[0].built_in(), "zip pieces are joined");
    }

    #[test]
    fn missing_parts_are_named() {
        assert_eq!(sets(&["m.part1.rar", "m.part3.rar"])[0].2, [2]);
        assert_eq!(sets(&["m.part2.rar"])[0].2, [1], "the first part is missing");
        assert_eq!(sets(&["m.r00", "m.r01"])[0].2, [0], "the .rar itself is missing");
        assert_eq!(sets(&["s.z01", "s.zip", "s.z03"])[0].2, [2]);
        assert_eq!(sets(&["b.7z.003", "b.7z.001"])[0].2, [2]);
        assert_eq!(sets(&["b.7z.000", "b.7z.001"])[0].2, Vec::<u32>::new(), "pieces may count from 0");
    }

    #[test]
    fn different_archives_and_families_stay_apart() {
        let names = ["a.part1.rar", "a.rar", "b.zip", "B.Z01", "a.part2.rar", "a.7z.001", "c.tar.gz", "a.part1.rar"];
        let got = sets(&names);
        assert_eq!(
            got,
            [
                ("a".to_string(), vec![0, 4], vec![]),
                ("a".to_string(), vec![1], vec![]),
                ("b".to_string(), vec![2, 3], vec![]),
                ("a".to_string(), vec![5], vec![]),
                ("c".to_string(), vec![6], vec![]),
            ]
        );
        assert!(archive_sets(&["c.tar.gz"])[0].built_in());
        assert!(!archive_sets(&["c.rar"])[0].built_in());
        assert!(!archive_sets(&["c.7z"])[0].built_in());
    }

    #[test]
    fn split_archives_become_packages_of_their_own() {
        let names = ["show.part1.rar", "show.part2.rar", "cover.jpg", "notes.zip", "set.7z.001", "set.7z.002"];
        let got: Vec<(String, Vec<usize>)> =
            plan_packages(&names, &Packaging::Archives).into_iter().map(|(n, i)| (n.to_string(), i)).collect();
        assert_eq!(got, [("show".to_string(), vec![0, 1]), ("set".to_string(), vec![4, 5])]);
        assert!(plan_packages(&names, &Packaging::None).is_empty());
        let one = Packaging::One { name: PackageName::parse("Season 1").unwrap(), folder: None };
        let got = plan_packages(&names, &one);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, (0..names.len()).collect::<Vec<_>>());
        assert!(plan_packages(&[], &one).is_empty());
    }

    #[test]
    fn a_package_is_complete_when_every_download_is_done() {
        let done = Some(Status::Done);
        assert!(complete(&[done, done]));
        assert!(!complete(&[done, Some(Status::Downloading)]));
        assert!(!complete(&[done, None]), "a removed download never finishes");
        assert!(!complete(&[]));
    }

    #[test]
    fn names_are_trimmed_and_give_a_folder() {
        let n = PackageName::parse("  Season 1: Pilot?  ").unwrap();
        assert_eq!(n.as_str(), "Season 1: Pilot?");
        assert_eq!(n.folder_name().as_str(), "Season 1_ Pilot_");
        assert_eq!(PackageName::parse(" "), Err(PackageError::EmptyName));
        assert_eq!(PackageName::parse(&"x".repeat(MAX_NAME + 1)), Err(PackageError::LongName));
        assert!(matches!(PackageName::parse("..."), Err(PackageError::NotAFolder(_))));
        assert_eq!(PackageName::parse("CON").unwrap().folder_name().as_str(), "_CON");
        assert_eq!(serde_json::to_string(&n).unwrap(), r#""Season 1: Pilot?""#);
    }

    #[test]
    fn entries_stay_inside_the_folder() {
        let parts = |name: &str| entry_path(name).map(|p| p.iter().map(|f| f.to_string()).collect::<Vec<_>>());
        assert_eq!(parts("docs/readme.txt").unwrap(), ["docs", "readme.txt"]);
        assert_eq!(parts("docs\\sub\\a.txt").unwrap(), ["docs", "sub", "a.txt"]);
        assert_eq!(parts("./a//b/./c").unwrap(), ["a", "b", "c"]);
        assert_eq!(parts("dir/CON.txt").unwrap(), ["dir", "_CON.txt"]);
        for bad in [
            "../evil",
            "a/../../evil",
            "a/..",
            "/etc/passwd",
            "\\Windows\\evil.dll",
            "C:\\evil",
            "C:evil",
            "file:stream",
            "a\0b",
            "",
            "./",
            "..\\..\\x",
        ] {
            assert_eq!(parts(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn space_is_checked_with_a_margin() {
        assert_eq!(check_space(ByteCount::mib(10), ByteCount::mib(100)), Ok(()));
        assert_eq!(
            check_space(ByteCount::mib(40), ByteCount::mib(100)),
            Err(UnpackFailure::NoSpace { needed: ByteCount::mib(40), free: ByteCount::mib(100) })
        );
    }

    #[test]
    fn a_taken_name_is_numbered() {
        let name = FileName::parse("a.txt").unwrap();
        let taken = ["a.txt", "a (1).txt"];
        assert_eq!(free_name(&name, |n| taken.contains(&n.as_str())).as_str(), "a (2).txt");
        assert_eq!(free_name(&name, |_| false).as_str(), "a.txt");
    }

    #[test]
    fn passwords_never_show_and_states_keep_their_wire_names() {
        let p = Password::new("hunter2".into()).unwrap();
        assert_eq!(format!("{p:?}"), "Password(..)");
        assert!(Password::new(String::new()).is_none());
        let failed = Unpack::Failed { reason: UnpackFailure::MissingPart { archive: "m.rar".into(), part: 2 } };
        let json = serde_json::to_string(&failed).unwrap();
        assert_eq!(json, r#"{"state":"failed","reason":{"kind":"missing_part","archive":"m.rar","part":2}}"#);
        assert_eq!(
            serde_json::from_str::<Unpack>(r#"{"state":"needs_password"}"#).unwrap(),
            Unpack::NeedsPassword { wrong: false }
        );
        assert_eq!(Unpack::Unpacking.after_restart(), Unpack::Waiting);
    }

    #[test]
    fn packages_load_without_the_optional_fields() {
        let p: Package = serde_json::from_str(r#"{"id":2,"name":"Show","downloads":[4,5]}"#).unwrap();
        assert_eq!(
            p,
            Package::new(
                PackageId::new(2),
                PackageName::parse("Show").unwrap(),
                None,
                vec![DownloadId::new(4), DownloadId::new(5)]
            )
        );
        assert!(p.can_unpack());
    }
}
