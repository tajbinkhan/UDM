//! Unpacking a package's archives (the `Unpacker` port): zip, tar, and gzip
//! and xz streams (`.tar.gz`, `.tar.xz` among them) in-process, and 7z, RAR,
//! bzip2 and spanned zips through an installed 7-Zip, found like ffmpeg is:
//! the program Options names, else `7z`, `7zz` or `7za` on `PATH`, else
//! where 7-Zip installs itself (`paths::program_dirs`).
//!
//! The same rules hold for every kind:
//!
//! - An archive is unpacked into a fresh hidden folder inside the folder it
//!   goes to, and its files are moved into place only once all of it came
//!   out. One that fails halfway leaves nothing behind.
//! - An entry whose name would land outside that folder (`..`, an absolute
//!   path, a drive, an NTFS stream; see `package::entry_path`) refuses the
//!   whole archive. No link is ever made: in-process, link entries are
//!   skipped; an archive 7-Zip lists with a link is refused before it runs,
//!   and one whose output holds a link anyway is refused after.
//! - Nothing already in the folder is replaced: an unpacked file or folder
//!   whose name is taken gets a numbered name (`name (1).ext`), as a
//!   download does.
//! - An archive of more than `MAX_FILES` files is refused, and so is one
//!   that would leave less than `SPACE_MARGIN` free on the disk. The sizes
//!   a zip or 7-Zip lists are checked before anything is written, and the
//!   bytes are counted as they are written in-process, so an archive that
//!   understates its sizes stops at the limit.
//! - A password goes to 7-Zip on its standard input, which it reads when it
//!   asks for one, never on its command line, where other users of the
//!   computer could read it. It is never logged or saved.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use pullbay_domain::bytes::ByteCount;
use pullbay_domain::filename::{sanitize_filename, FileName};
use pullbay_domain::package::{
    check_space, entry_path, free_name, ArchiveKind, Family, Password, UnpackError, UnpackFailure, MAX_FILES,
};
use pullbay_domain::ports::{UnpackJob, Unpacked, Unpacker};
use pullbay_domain::settings::Settings;

/// The `Unpacker` adapter.
pub struct Unpacking;

impl Unpacker for Unpacking {
    fn unpack(
        &self,
        job: &UnpackJob,
        password: Option<&Password>,
        settings: &Settings,
    ) -> Result<Unpacked, UnpackError> {
        unpack_with(job, password, || seven_zip(settings))
    }
}

/// 7-Zip's program: the one `settings` name, else one on `PATH`, else one
/// where 7-Zip installs itself.
pub fn seven_zip(settings: &Settings) -> Option<PathBuf> {
    let set = settings.seven_zip_path.trim();
    if !set.is_empty() {
        return Some(PathBuf::from(set)).filter(|p| p.is_file());
    }
    let names = ["7z", "7zz", "7za"].map(crate::platform::exe_name);
    let on_path = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
    let installed = crate::paths::program_dirs().into_iter().flat_map(|dir| [dir.join("7-Zip"), dir]);
    on_path.into_iter().chain(installed).flat_map(|dir| names.iter().map(move |n| dir.join(n))).find(|p| p.is_file())
}

/// Whether Pullbay unpacks `job` itself: a kind it knows, in one file or
/// cut into pieces it joins.
fn built_in(job: &UnpackJob) -> bool {
    job.kind.built_in() && (job.family == Family::Pieces || job.parts.len() == 1)
}

/// Unpack `job` (see `Unpacker::unpack`), with 7-Zip found by `find` when
/// it needs it.
pub fn unpack_with(
    job: &UnpackJob,
    password: Option<&Password>,
    find: impl FnOnce() -> Option<PathBuf>,
) -> Result<Unpacked, UnpackError> {
    let io = |e: io::Error| UnpackFailure::Io { archive: job.archive.clone(), detail: e.to_string() };
    fs::create_dir_all(&job.into).map_err(io)?;
    let staging = Staging::new(&job.into).map_err(io)?;
    let mut budget = Budget { archive: job.archive.clone(), free: free_bytes(&job.into), written: 0, files: 0 };
    if built_in(job) {
        unpack_built_in(job, password, staging.path(), &mut budget)?;
    } else {
        let program = find().ok_or_else(|| UnpackFailure::NeedsSevenZip { archive: job.archive.clone() })?;
        seven::unpack(&program, job, password, staging.path(), &mut budget)?;
    }
    // Whatever made them, the files are checked before they move.
    let files = check_staged(staging.path(), Path::new(""), &job.archive)?;
    if files > MAX_FILES {
        return Err(UnpackFailure::TooManyFiles { archive: job.archive.clone(), max: MAX_FILES }.into());
    }
    merge(staging.path(), &job.into).map_err(io)?;
    Ok(Unpacked { files, bytes: ByteCount::new(budget.written) })
}

/// The hidden folder an archive is unpacked into first, removed with
/// whatever is left in it when it goes.
struct Staging(PathBuf);

impl Staging {
    fn new(into: &Path) -> io::Result<Staging> {
        for n in 0..1000 {
            let path = into.join(format!(".pullbay-unpacking-{}-{n}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Staging(path)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(io::ErrorKind::AlreadyExists, "no free name for a folder to unpack into"))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        // remove_dir_all does not follow links, so a link 7-Zip made cannot
        // take anything outside with it.
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// What unpacking one archive may still write.
struct Budget {
    archive: String,
    /// The bytes free on the disk when it began, when the system says.
    free: Option<u64>,
    written: u64,
    files: u64,
}

impl Budget {
    /// Whether `more` bytes still fit.
    fn fits(&self, more: u64) -> Result<(), UnpackFailure> {
        match self.free {
            Some(free) => check_space(ByteCount::new(self.written.saturating_add(more)), ByteCount::new(free)),
            None => Ok(()),
        }
    }

    /// Count one more file.
    fn file(&mut self) -> Result<(), UnpackFailure> {
        self.files += 1;
        if self.files > MAX_FILES {
            return Err(UnpackFailure::TooManyFiles { archive: self.archive.clone(), max: MAX_FILES });
        }
        Ok(())
    }

    fn damaged(&self, detail: impl std::fmt::Display) -> UnpackFailure {
        UnpackFailure::Damaged { archive: self.archive.clone(), detail: detail.to_string() }
    }

    fn io(&self, e: io::Error) -> UnpackFailure {
        UnpackFailure::Io { archive: self.archive.clone(), detail: e.to_string() }
    }

    fn unsafe_entry(&self, entry: &str) -> UnpackFailure {
        UnpackFailure::Unsafe { archive: self.archive.clone(), entry: entry.to_string() }
    }
}

/// Copy `reader` into `out`, counting the bytes against `budget`. A read
/// that fails is a damaged archive, or with `decrypting` a wrong password.
fn copy_counted(
    reader: &mut impl Read,
    out: &mut File,
    budget: &mut Budget,
    decrypting: bool,
) -> Result<(), UnpackError> {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) if decrypting => return Err(UnpackError::WrongPassword),
            Err(e) => return Err(budget.damaged(e).into()),
        };
        budget.fits(n as u64)?;
        budget.written += n as u64;
        out.write_all(&buf[..n]).map_err(|e| budget.io(e))?;
    }
}

/// A name that is only the archive's own folder (`./`, `/`), which makes
/// nothing.
fn is_root(name: &str) -> bool {
    name.split(['/', '\\']).all(|p| p.is_empty() || p == ".")
}

/// Create the file `parts` names under `root`, with its folders. A name
/// already taken (an archive may list one twice) gets a numbered one.
fn create_in(root: &Path, parts: &[FileName]) -> io::Result<File> {
    let Some((name, dirs)) = parts.split_last() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "an entry without a name"));
    };
    let dir = dirs.iter().fold(root.to_path_buf(), |dir, d| dir.join(d.as_str()));
    fs::create_dir_all(&dir)?;
    for n in 0..10_000 {
        match OpenOptions::new().write(true).create_new(true).open(dir.join(name.numbered(n).as_str())) {
            Ok(f) => return Ok(f),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{name} is taken")))
}

fn unpack_built_in(
    job: &UnpackJob,
    password: Option<&Password>,
    staging: &Path,
    budget: &mut Budget,
) -> Result<(), UnpackError> {
    let joined = Joined::open(&job.parts).map_err(|e| budget.io(e))?;
    match job.kind {
        ArchiveKind::Zip => unzip(joined, password, staging, budget),
        ArchiveKind::Tar => untar(BufReader::new(joined), staging, budget),
        ArchiveKind::TarGz => untar(flate2::read::MultiGzDecoder::new(BufReader::new(joined)), staging, budget),
        ArchiveKind::TarXz => untar(lzma_rust2::XzReader::new(BufReader::new(joined), true), staging, budget),
        ArchiveKind::Gzip => {
            let mut reader = flate2::read::MultiGzDecoder::new(BufReader::new(joined));
            single(&mut reader, &job.archive, staging, budget)
        }
        ArchiveKind::Xz => {
            let mut reader = lzma_rust2::XzReader::new(BufReader::new(joined), true);
            single(&mut reader, &job.archive, staging, budget)
        }
        ArchiveKind::SevenZip | ArchiveKind::Rar | ArchiveKind::TarBz2 | ArchiveKind::Bzip2 => {
            Err(UnpackFailure::NeedsSevenZip { archive: job.archive.clone() }.into())
        }
    }
}

fn unzip<R: Read + Seek>(
    reader: R,
    password: Option<&Password>,
    staging: &Path,
    budget: &mut Budget,
) -> Result<(), UnpackError> {
    let mut zip = ::zip::ZipArchive::new(reader).map_err(|e| budget.damaged(e))?;
    if zip.len() as u64 > MAX_FILES {
        return Err(UnpackFailure::TooManyFiles { archive: budget.archive.clone(), max: MAX_FILES }.into());
    }
    // Look at every entry first: where it goes, how big it says it is and
    // whether it is encrypted.
    struct Entry {
        parts: Vec<FileName>,
        dir: bool,
        encrypted: bool,
    }
    let mut entries = Vec::with_capacity(zip.len());
    let mut total = 0u64;
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i).map_err(|e| budget.damaged(e))?;
        let name = f.name().map_err(|e| budget.damaged(e))?.into_owned();
        if f.is_symlink() || is_root(&name) {
            entries.push(None);
            continue;
        }
        let parts = entry_path(&name).ok_or_else(|| budget.unsafe_entry(&name))?;
        total = total.saturating_add(f.size());
        entries.push(Some(Entry { parts, dir: f.is_dir(), encrypted: f.encrypted() }));
    }
    budget.fits(total)?;
    if password.is_none() && entries.iter().flatten().any(|e| e.encrypted) {
        return Err(UnpackError::NeedsPassword);
    }
    for (i, entry) in entries.into_iter().enumerate() {
        let Some(entry) = entry else { continue };
        if entry.dir {
            let dir = entry.parts.iter().fold(staging.to_path_buf(), |dir, d| dir.join(d.as_str()));
            fs::create_dir_all(dir).map_err(|e| budget.io(e))?;
            continue;
        }
        budget.file()?;
        let decrypt = password.filter(|_| entry.encrypted);
        let opened = match decrypt {
            Some(p) => zip.by_index_decrypt(i, p.expose().as_bytes()),
            None => zip.by_index(i),
        };
        let mut file = opened.map_err(|e| match e {
            ::zip::result::ZipError::InvalidPassword => UnpackError::WrongPassword,
            ::zip::result::ZipError::UnsupportedArchive(why) if why == ::zip::result::ZipError::PASSWORD_REQUIRED => {
                UnpackError::NeedsPassword
            }
            e => budget.damaged(e).into(),
        })?;
        let mut out = create_in(staging, &entry.parts).map_err(|e| budget.io(e))?;
        copy_counted(&mut file, &mut out, budget, decrypt.is_some())?;
    }
    Ok(())
}

fn untar(reader: impl Read, staging: &Path, budget: &mut Budget) -> Result<(), UnpackError> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(|e| budget.damaged(e))? {
        let mut entry = entry.map_err(|e| budget.damaged(e))?;
        let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        let kind = entry.header().entry_type();
        // Links, devices and the like are never made.
        if !(kind.is_dir() || kind.is_file()) || is_root(&name) {
            continue;
        }
        let parts = entry_path(&name).ok_or_else(|| budget.unsafe_entry(&name))?;
        if kind.is_dir() {
            let dir = parts.iter().fold(staging.to_path_buf(), |dir, d| dir.join(d.as_str()));
            fs::create_dir_all(dir).map_err(|e| budget.io(e))?;
            continue;
        }
        budget.file()?;
        budget.fits(entry.size())?;
        let mut out = create_in(staging, &parts).map_err(|e| budget.io(e))?;
        copy_counted(&mut entry, &mut out, budget, false)?;
    }
    Ok(())
}

/// A single compressed file (`notes.txt.gz`), unpacked under its name
/// without the ending.
fn single(reader: &mut impl Read, archive: &str, staging: &Path, budget: &mut Budget) -> Result<(), UnpackError> {
    let stem = Path::new(archive).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = sanitize_filename(&stem)
        .or_else(|| sanitize_filename("unpacked"))
        .ok_or_else(|| budget.unsafe_entry(archive))?;
    budget.file()?;
    let mut out = create_in(staging, &[name]).map_err(|e| budget.io(e))?;
    copy_counted(reader, &mut out, budget, false)
}

/// Check what was unpacked into `dir` (`rel` under the staging folder):
/// no links anywhere. Returns how many files it holds.
fn check_staged(dir: &Path, rel: &Path, archive: &str) -> Result<u64, UnpackError> {
    let io = |e: io::Error| UnpackFailure::Io { archive: archive.to_string(), detail: e.to_string() };
    let mut files = 0;
    for entry in fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        let kind = entry.file_type().map_err(io)?;
        let rel = rel.join(entry.file_name());
        if kind.is_symlink() {
            return Err(UnpackFailure::Unsafe { archive: archive.to_string(), entry: rel.display().to_string() }.into());
        }
        if kind.is_dir() {
            files += check_staged(&entry.path(), &rel, archive)?;
        } else {
            files += 1;
        }
    }
    Ok(files)
}

/// `name` in `dir`, numbered when it is taken there.
fn free_path(dir: &Path, name: &OsStr) -> PathBuf {
    let taken = |p: &Path| p.symlink_metadata().is_ok();
    let Some(name) = name.to_str().and_then(|n| FileName::parse(n).ok()) else {
        // A name that is not Unicode keeps its bytes, with a number after
        // it when taken.
        let mut path = dir.join(name);
        let mut n = 1;
        while taken(&path) {
            let mut numbered = OsString::from(name);
            numbered.push(format!(" ({n})"));
            path = dir.join(numbered);
            n += 1;
        }
        return path;
    };
    dir.join(free_name(&name, |n| taken(&dir.join(n.as_str()))).as_str())
}

/// Move what `from` holds into `to`: a folder already there gets the new
/// files inside it, and a name taken by anything else is numbered.
fn merge(from: &Path, to: &Path) -> io::Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let source = entry.path();
        let there = to.join(&name);
        if entry.file_type()?.is_dir() && there.symlink_metadata().is_ok_and(|m| m.is_dir()) {
            merge(&source, &there)?;
            continue;
        }
        fs::rename(&source, free_path(to, &name))?;
    }
    Ok(())
}

/// The parts of an archive cut into pieces, read as one file.
struct Joined {
    parts: Vec<(File, u64)>,
    pos: u64,
    len: u64,
}

impl Joined {
    fn open(paths: &[PathBuf]) -> io::Result<Joined> {
        let mut parts = Vec::with_capacity(paths.len());
        for p in paths {
            let file = File::open(p)?;
            let len = file.metadata()?.len();
            parts.push((file, len));
        }
        let len = parts.iter().map(|(_, l)| *l).sum();
        Ok(Joined { parts, pos: 0, len })
    }
}

impl Read for Joined {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut start = 0;
        for (file, len) in &mut self.parts {
            if self.pos < start + *len {
                let within = self.pos - start;
                file.seek(SeekFrom::Start(within))?;
                let room = usize::try_from(*len - within).unwrap_or(usize::MAX).min(buf.len());
                let n = file.read(&mut buf[..room])?;
                self.pos += n as u64;
                return Ok(n);
            }
            start += *len;
        }
        Ok(0)
    }
}

impl Seek for Joined {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = pos.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start"))?;
        Ok(self.pos)
    }
}

/// The bytes free for this user on the disk `dir` is on.
#[cfg(unix)]
// The counts are u32 on some systems and u64 on others.
#[allow(clippy::useless_conversion)]
fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs only writes the struct it is given, which is plain
    // integers, so a zeroed one is a valid value to start from.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `path` is a NUL-terminated string and `stat` outlives the call.
    let ok = unsafe { libc::statvfs(path.as_ptr(), &mut stat) } == 0;
    ok.then(|| u64::from(stat.f_bavail).saturating_mul(u64::from(stat.f_frsize)))
}

#[cfg(windows)]
fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(dir: *const u16, free_to_caller: *mut u64, total: *mut u64, total_free: *mut u64)
            -> i32;
    }
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free = 0u64;
    // SAFETY: `wide` is NUL-terminated; the counts it does not ask for are
    // null, which the call allows.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

#[cfg(not(any(unix, windows)))]
fn free_bytes(_: &Path) -> Option<u64> {
    None
}

/// 7-Zip, for what Pullbay does not unpack itself.
mod seven {
    use super::*;

    /// One entry 7-Zip lists.
    #[derive(Debug, Default, PartialEq, Eq)]
    pub struct Listed {
        pub path: String,
        pub size: u64,
        pub folder: bool,
        pub encrypted: bool,
        pub link: bool,
    }

    /// The entries in `7z l -slt`'s output: blocks of `Key = value` lines
    /// after the line of dashes that ends the archive's own block.
    pub fn parse_listing(text: &str) -> Vec<Listed> {
        let Some((_, body)) = text.split_once("\n----------") else {
            return Vec::new();
        };
        let mut list = Vec::new();
        let mut cur: Option<Listed> = None;
        for line in body.lines() {
            let line = line.trim_end_matches('\r');
            let Some((key, value)) = line.split_once(" = ").or_else(|| line.strip_suffix(" =").map(|k| (k, ""))) else {
                continue;
            };
            match key {
                "Path" => {
                    list.extend(cur.take());
                    cur = Some(Listed { path: value.to_string(), ..Listed::default() });
                }
                _ if cur.is_none() => {}
                "Size" => cur.iter_mut().for_each(|c| c.size = value.parse().unwrap_or(0)),
                "Folder" => cur.iter_mut().for_each(|c| c.folder = value == "+"),
                "Encrypted" => cur.iter_mut().for_each(|c| c.encrypted = value == "+"),
                "Symbolic Link" | "Hard Link" | "Link" if !value.is_empty() => {
                    cur.iter_mut().for_each(|c| c.link = true)
                }
                // Unix modes: `A_ lrwxrwxrwx` is a symbolic link.
                "Attributes" if value.split_whitespace().any(|w| w.len() == 10 && w.starts_with('l')) => {
                    cur.iter_mut().for_each(|c| c.link = true)
                }
                _ => {}
            }
        }
        list.extend(cur);
        list
    }

    /// Whether 7-Zip's complaint is about a password.
    fn about_password(output: &str) -> bool {
        let lower = output.to_lowercase();
        lower.contains("wrong password") || lower.contains("encrypted archive") || lower.contains("enter password")
    }

    /// What 7-Zip said last, for a message: its error lines, or its last
    /// lines.
    fn complaint(output: &str) -> String {
        let lines: Vec<&str> = output.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let errors: Vec<&str> = lines.iter().copied().filter(|l| l.to_lowercase().contains("error")).collect();
        let chosen = if errors.is_empty() { &lines[lines.len().saturating_sub(2)..] } else { &errors[..] };
        chosen.join(" ").chars().take(300).collect()
    }

    /// Run 7-Zip with `args`, giving it `password` on its standard input.
    /// Returns whether it succeeded and what it said.
    fn run(program: &Path, args: &[OsString], password: Option<&Password>) -> io::Result<(bool, String)> {
        let mut cmd = Command::new(program);
        cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = crate::platform::no_window(&mut cmd).spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            // 7-Zip reads a line when it needs a password. Closing its input
            // right after means it never waits for one that is not coming.
            if let Some(p) = password {
                let _ = stdin.write_all(p.expose().as_bytes());
                let _ = stdin.write_all(b"\n");
            }
        }
        let out = child.wait_with_output()?;
        let mut said = String::from_utf8_lossy(&out.stdout).into_owned();
        said.push('\n');
        said.push_str(&String::from_utf8_lossy(&out.stderr));
        Ok((out.status.success(), said))
    }

    /// Switches every call has: UTF-8 output on Windows, where 7-Zip would
    /// otherwise write the console's code page.
    fn common() -> Vec<OsString> {
        if cfg!(windows) {
            vec!["-sccUTF-8".into()]
        } else {
            Vec::new()
        }
    }

    pub fn unpack(
        program: &Path,
        job: &UnpackJob,
        password: Option<&Password>,
        staging: &Path,
        budget: &mut Budget,
    ) -> Result<(), UnpackError> {
        let Some(first) = job.parts.first() else {
            return Err(budget.damaged("no parts").into());
        };
        let failed = |said: &str| -> UnpackError {
            match password {
                _ if !about_password(said) => budget.damaged(complaint(said)).into(),
                Some(_) => UnpackError::WrongPassword,
                None => UnpackError::NeedsPassword,
            }
        };

        let mut list: Vec<OsString> = vec!["l".into(), "-slt".into()];
        list.extend(common());
        list.extend(["--".into(), first.into()]);
        let (ok, said) = run(program, &list, password).map_err(|e| budget.io(e))?;
        if !ok {
            return Err(failed(&said));
        }
        let entries = parse_listing(&said);
        let mut total = 0u64;
        let mut count = 0u64;
        for e in &entries {
            if e.link {
                return Err(budget.unsafe_entry(&e.path).into());
            }
            if !is_root(&e.path) && entry_path(&e.path).is_none() {
                return Err(budget.unsafe_entry(&e.path).into());
            }
            if !e.folder {
                count += 1;
                total = total.saturating_add(e.size);
            }
        }
        if count > MAX_FILES {
            return Err(UnpackFailure::TooManyFiles { archive: budget.archive.clone(), max: MAX_FILES }.into());
        }
        budget.fits(total)?;
        if password.is_none() && entries.iter().any(|e| e.encrypted) {
            return Err(UnpackError::NeedsPassword);
        }

        // Extract with full paths, renaming rather than replacing a name
        // the archive lists twice.
        let mut out = OsString::from("-o");
        out.push(staging.as_os_str());
        let mut extract: Vec<OsString> = vec!["x".into(), "-y".into(), "-aou".into(), "-bd".into(), out];
        extract.extend(common());
        extract.extend(["--".into(), first.into()]);
        let (ok, said) = run(program, &extract, password).map_err(|e| budget.io(e))?;
        if !ok {
            return Err(failed(&said));
        }
        budget.written = budget.written.saturating_add(total);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A new empty folder for one test.
    fn scratch(name: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("pullbay_unpack_{}_{name}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn job(kind: ArchiveKind, family: Family, parts: Vec<PathBuf>, into: PathBuf) -> UnpackJob {
        let archive = parts[0].file_name().unwrap().to_string_lossy().into_owned();
        UnpackJob { archive, kind, family, parts, into }
    }

    fn no_7z() -> Option<PathBuf> {
        None
    }

    /// A zip of `files` (name, text), each encrypted with `password` when
    /// one is given.
    fn zip_of(path: &Path, files: &[(&str, &str)], password: Option<&str>) {
        use ::zip::write::SimpleFileOptions;
        let mut w = ::zip::ZipWriter::new(File::create(path).unwrap());
        for (name, text) in files {
            let mut options = SimpleFileOptions::default().compression_method(::zip::CompressionMethod::Deflated);
            if let Some(p) = password {
                options = options.with_aes_encryption(::zip::AesMode::Aes256, p);
            }
            if name.ends_with('/') {
                w.add_directory(*name, options).unwrap();
            } else {
                w.start_file(*name, options).unwrap();
                w.write_all(text.as_bytes()).unwrap();
            }
        }
        w.finish().unwrap();
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    /// The names in `dir`, sorted, without the staging folder.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn a_zip_unpacks_with_its_folders() {
        let dir = scratch("zip");
        let archive = dir.join("photos.zip");
        zip_of(&archive, &[("docs/", ""), ("docs/a.txt", "alpha"), ("b.txt", "beta")], None);
        let into = dir.join("Photos");
        let done = unpack_with(&job(ArchiveKind::Zip, Family::Classic, vec![archive], into.clone()), None, no_7z);
        assert_eq!(done.unwrap(), Unpacked { files: 2, bytes: ByteCount::new(9) });
        assert_eq!(read(&into.join("docs").join("a.txt")), "alpha");
        assert_eq!(read(&into.join("b.txt")), "beta");
        assert_eq!(names(&into), ["b.txt", "docs"], "the staging folder is gone");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn files_already_there_are_never_replaced() {
        let dir = scratch("taken");
        let archive = dir.join("a.zip");
        zip_of(&archive, &[("b.txt", "new"), ("sub/c.txt", "new c")], None);
        let into = dir.join("out");
        fs::create_dir_all(into.join("sub")).unwrap();
        fs::write(into.join("b.txt"), "old").unwrap();
        fs::write(into.join("sub").join("c.txt"), "old c").unwrap();
        unpack_with(&job(ArchiveKind::Zip, Family::Classic, vec![archive], into.clone()), None, no_7z).unwrap();
        assert_eq!(read(&into.join("b.txt")), "old");
        assert_eq!(read(&into.join("b (1).txt")), "new");
        assert_eq!(read(&into.join("sub").join("c.txt")), "old c", "a folder already there takes the new files in");
        assert_eq!(read(&into.join("sub").join("c (1).txt")), "new c");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_entry_leaving_the_folder_refuses_the_whole_archive() {
        for evil in ["../evil.txt", "/tmp/evil.txt", "ok/../../evil.txt", "C:\\evil.txt"] {
            let dir = scratch("slip");
            let archive = dir.join("slip.zip");
            zip_of(&archive, &[("fine.txt", "fine"), (evil, "evil")], None);
            let into = dir.join("out");
            let got = unpack_with(&job(ArchiveKind::Zip, Family::Classic, vec![archive], into.clone()), None, no_7z);
            assert_eq!(
                got,
                Err(UnpackFailure::Unsafe { archive: "slip.zip".into(), entry: evil.into() }.into()),
                "{evil}"
            );
            assert!(names(&into).is_empty(), "nothing is unpacked: {:?}", names(&into));
            assert!(!dir.join("evil.txt").exists());
            fs::remove_dir_all(&dir).ok();
        }
    }

    #[test]
    fn an_encrypted_zip_needs_the_right_password() {
        let dir = scratch("aes");
        let archive = dir.join("secret.zip");
        zip_of(&archive, &[("s.txt", "secret")], Some("open sesame"));
        let into = dir.join("out");
        let j = job(ArchiveKind::Zip, Family::Classic, vec![archive], into.clone());
        assert_eq!(unpack_with(&j, None, no_7z), Err(UnpackError::NeedsPassword));
        let wrong = Password::new("guess".into());
        assert_eq!(unpack_with(&j, wrong.as_ref(), no_7z), Err(UnpackError::WrongPassword));
        assert!(names(&into).is_empty());
        let right = Password::new("open sesame".into());
        assert_eq!(unpack_with(&j, right.as_ref(), no_7z).unwrap().files, 1);
        assert_eq!(read(&into.join("s.txt")), "secret");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_zip_bigger_than_the_disk_is_refused_before_writing() {
        let dir = scratch("bomb");
        let archive = dir.join("bomb.zip");
        {
            use ::zip::write::SimpleFileOptions;
            let mut w = ::zip::ZipWriter::new(File::create(&archive).unwrap());
            let options = SimpleFileOptions::default().compression_method(::zip::CompressionMethod::Deflated);
            w.start_file("zeros.bin", options).unwrap();
            let chunk = vec![0u8; 1024 * 1024];
            for _ in 0..8 {
                w.write_all(&chunk).unwrap();
            }
            w.finish().unwrap();
        }
        assert!(fs::metadata(&archive).unwrap().len() < 64 * 1024, "eight megabytes of zeros pack small");
        let into = dir.join("out");
        fs::create_dir_all(&into).unwrap();
        let staging = Staging::new(&into).unwrap();
        let mut budget = Budget { archive: "bomb.zip".into(), free: Some(32 * 1024 * 1024), written: 0, files: 0 };
        let got = unzip(File::open(&archive).unwrap(), None, staging.path(), &mut budget);
        assert!(matches!(got, Err(UnpackError::Failed(UnpackFailure::NoSpace { .. }))), "{got:?}");
        assert_eq!(budget.written, 0, "nothing was written");
        drop(staging);
        assert!(names(&into).is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    /// A tar of `files`, and one entry named `raw` (written into the header
    /// as given, as a hostile archive would).
    fn tar_bytes(files: &[(&str, &str)], raw: Option<&str>) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        for (name, text) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(text.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, name, text.as_bytes()).unwrap();
        }
        if let Some(raw) = raw {
            let mut h = tar::Header::new_old();
            h.as_old_mut().name[..raw.len()].copy_from_slice(raw.as_bytes());
            h.set_size(4);
            h.set_mode(0o644);
            h.set_cksum();
            b.append(&h, &b"evil"[..]).unwrap();
        }
        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_size(0);
        b.append_link(&mut link, "escape", "/etc").unwrap();
        b.into_inner().unwrap()
    }

    #[test]
    fn tar_gz_and_tar_xz_unpack_without_their_links() {
        let tar = tar_bytes(&[("dir/a.txt", "alpha"), ("b.txt", "beta")], None);
        let dir = scratch("tar");
        let gz = dir.join("src.tar.gz");
        let mut enc = flate2::write::GzEncoder::new(File::create(&gz).unwrap(), flate2::Compression::fast());
        enc.write_all(&tar).unwrap();
        enc.finish().unwrap();
        let xz = dir.join("src.tar.xz");
        let mut enc = lzma_rust2::XzWriter::new(File::create(&xz).unwrap(), lzma_rust2::XzOptions::default()).unwrap();
        enc.write_all(&tar).unwrap();
        enc.finish().unwrap();
        for (path, kind) in [(gz, ArchiveKind::TarGz), (xz, ArchiveKind::TarXz)] {
            let into = dir.join(format!("{kind:?}"));
            let done = unpack_with(&job(kind, Family::Classic, vec![path], into.clone()), None, no_7z).unwrap();
            assert_eq!(done.files, 2, "{kind:?}");
            assert_eq!(read(&into.join("dir").join("a.txt")), "alpha");
            assert_eq!(names(&into), ["b.txt", "dir"], "no link is made");
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_tar_entry_leaving_the_folder_refuses_it() {
        let dir = scratch("tar_slip");
        let path = dir.join("evil.tar");
        fs::write(&path, tar_bytes(&[("fine.txt", "fine")], Some("../../evil.txt"))).unwrap();
        let into = dir.join("out");
        let got = unpack_with(&job(ArchiveKind::Tar, Family::Classic, vec![path], into.clone()), None, no_7z);
        assert_eq!(
            got,
            Err(UnpackFailure::Unsafe { archive: "evil.tar".into(), entry: "../../evil.txt".into() }.into())
        );
        assert!(names(&into).is_empty());
        assert!(!dir.parent().unwrap().join("evil.txt").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_compressed_file_unpacks_under_its_own_name_and_pieces_are_joined() {
        let dir = scratch("single");
        let gz = dir.join("notes.txt.gz");
        let mut enc = flate2::write::GzEncoder::new(File::create(&gz).unwrap(), flate2::Compression::fast());
        enc.write_all(b"hello").unwrap();
        enc.finish().unwrap();
        let into = dir.join("out");
        unpack_with(&job(ArchiveKind::Gzip, Family::Classic, vec![gz], into.clone()), None, no_7z).unwrap();
        assert_eq!(read(&into.join("notes.txt")), "hello");

        // A zip cut in three pieces, as a file splitter leaves it.
        let whole = dir.join("whole.zip");
        zip_of(&whole, &[("p.txt", "pieces")], None);
        let bytes = fs::read(&whole).unwrap();
        let third = bytes.len() / 3;
        let pieces: Vec<PathBuf> = [&bytes[..third], &bytes[third..2 * third], &bytes[2 * third..]]
            .iter()
            .enumerate()
            .map(|(i, part)| {
                let p = dir.join(format!("cut.zip.00{}", i + 1));
                fs::write(&p, part).unwrap();
                p
            })
            .collect();
        let into = dir.join("pieces");
        unpack_with(&job(ArchiveKind::Zip, Family::Pieces, pieces, into.clone()), None, no_7z).unwrap();
        assert_eq!(read(&into.join("p.txt")), "pieces");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rar_and_7z_need_7_zip() {
        let dir = scratch("no7z");
        let rar = dir.join("a.part1.rar");
        fs::write(&rar, b"Rar!").unwrap();
        let got = unpack_with(&job(ArchiveKind::Rar, Family::Parts, vec![rar], dir.join("out")), None, no_7z);
        assert_eq!(got, Err(UnpackFailure::NeedsSevenZip { archive: "a.part1.rar".into() }.into()));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn seven_zip_listings_are_read() {
        let text = "7-Zip 23.01\n\nListing archive: a.7z\n\n--\nPath = a.7z\nType = 7z\n\n----------\n\
                    Path = docs\nFolder = +\nSize = 0\nAttributes = D_ drwxr-xr-x\n\n\
                    Path = docs/a.txt\nFolder = -\nSize = 12\nAttributes = A_ -rw-r--r--\nEncrypted = +\n\n\
                    Path = link\nFolder = -\nSize = 4\nAttributes = A_ lrwxrwxrwx\n\n\
                    Path = win\nSymbolic Link =\nSize = 1\n";
        let got = seven::parse_listing(text);
        assert_eq!(got.len(), 4);
        assert!(got[0].folder && !got[0].link);
        assert_eq!((got[1].path.as_str(), got[1].size, got[1].encrypted), ("docs/a.txt", 12, true));
        assert!(got[2].link, "a unix symbolic link");
        assert!(!got[3].link, "an empty link field is no link");
        assert!(seven::parse_listing("no dashes here").is_empty());
    }

    /// A stand-in for 7-Zip: a shell script that lists `listing` and, for
    /// `x`, makes `out.txt` in the `-o` folder holding the line it read on
    /// its standard input, so a test sees the password arrive there and not
    /// on the command line, which it writes to `args.txt`.
    #[cfg(unix)]
    fn fake_7z(dir: &Path, listing: &str, extract: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("7z");
        let body = format!(
            "#!/bin/sh\necho \"$@\" >> '{args}'\nif [ \"$1\" = l ]; then\ncat <<'EOF'\n{listing}\nEOF\nexit 0\nfi\n\
             read -r pw\nfor a in \"$@\"; do case \"$a\" in -o*) out=\"${{a#-o}}\";; esac; done\n{extract}\n",
            args = dir.join("args.txt").display(),
        );
        fs::write(&script, body).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[cfg(unix)]
    #[test]
    fn seven_zip_gets_the_password_on_its_input_and_its_files_move_into_place() {
        let dir = scratch("fake7z");
        let listing = "--\nPath = a.rar\n\n----------\nPath = out.txt\nFolder = -\nSize = 6\nEncrypted = +\n";
        let program = fake_7z(&dir, listing, "printf '%s' \"$pw\" > \"$out/out.txt\"");
        let rar = dir.join("a.rar");
        fs::write(&rar, b"Rar!").unwrap();
        let into = dir.join("out");
        let j = job(ArchiveKind::Rar, Family::Classic, vec![rar], into.clone());
        let find = || Some(program.clone());
        assert_eq!(unpack_with(&j, None, find), Err(UnpackError::NeedsPassword), "an encrypted entry is listed");
        let pw = Password::new("s3cret".into());
        assert_eq!(unpack_with(&j, pw.as_ref(), find).unwrap().files, 1);
        assert_eq!(read(&into.join("out.txt")), "s3cret");
        let args = read(&dir.join("args.txt"));
        assert!(!args.contains("s3cret"), "the password is not an argument: {args}");
        assert!(args.contains("x -y -aou -bd -o"), "{args}");
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn seven_zip_output_with_a_link_or_a_listed_escape_is_refused() {
        let dir = scratch("fake7z_link");
        let rar = dir.join("a.rar");
        fs::write(&rar, b"Rar!").unwrap();
        let listing = "--\nPath = a.rar\n\n----------\nPath = ok.txt\nFolder = -\nSize = 1\n";
        let program = fake_7z(&dir, listing, "ln -s /etc \"$out/etc\"");
        let into = dir.join("out");
        let j = job(ArchiveKind::Rar, Family::Classic, vec![rar.clone()], into.clone());
        let got = unpack_with(&j, None, || Some(program.clone()));
        assert_eq!(got, Err(UnpackFailure::Unsafe { archive: "a.rar".into(), entry: "etc".into() }.into()));
        assert!(names(&into).is_empty(), "nothing moved");

        let listing = "--\nPath = a.rar\n\n----------\nPath = ../../x\nFolder = -\nSize = 1\n";
        let program = fake_7z(&dir, listing, "touch \"$out/never\"");
        let got = unpack_with(&j, None, || Some(program.clone()));
        assert_eq!(got, Err(UnpackFailure::Unsafe { archive: "a.rar".into(), entry: "../../x".into() }.into()));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn seven_zip_is_found_where_the_settings_say() {
        let dir = scratch("find7z");
        let program = dir.join(crate::platform::exe_name("7z"));
        fs::write(&program, b"").unwrap();
        let settings = Settings { seven_zip_path: program.display().to_string(), ..Settings::default() };
        assert_eq!(seven_zip(&settings), Some(program.clone()));
        let missing = Settings { seven_zip_path: dir.join("none.exe").display().to_string(), ..Settings::default() };
        assert_eq!(seven_zip(&missing), None);
        fs::remove_dir_all(&dir).ok();
    }
}
