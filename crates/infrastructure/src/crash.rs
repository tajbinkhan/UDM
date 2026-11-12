//! Problem reports on disk: the panic hook each program installs at startup
//! ([`install`]), and [`ReportFiles`], the `ProblemReports` adapter the
//! window offers reports through and writes its freeze reports with. The
//! report's layout, the file names and the issue they become are
//! `udm_domain::report`'s.
//!
//! Reports go in [`crate::paths::reports_dir`], one text file each
//! (`udm-<program>-<kind>-<ms>.txt`), readable only by the user on
//! Unix. A program writes at most [`MAX_PER_RUN`] crash reports (a thread
//! that keeps panicking does not fill the disk), and the folder keeps the
//! newest [`KEEP`]. `offered.txt` beside them lists the ones the window has
//! offered.
//!
//! Nothing here sends anything: the window opens an issue in the browser
//! when the user asks, and the user attaches the file.
//!
//! What a report holds, and what it never does (AGENTS.md section 11):
//! this copy's version, system and install, the panic's message, place,
//! thread and backtrace, and the end of the program's log. Every text goes
//! through [`scrub`] first: the daemon's token is cut out wherever it
//! appears, addresses lose their passwords ([`logging::redact`]), and
//! cookies, authorization headers, passwords and tokens lose their values
//! (`udm_domain::report::scrub`). The logs hold no cookies, headers or
//! token to begin with (see `logging`); download addresses stay, so the
//! help page asks the user to look a report over before attaching it.

use std::backtrace::Backtrace;
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use udm_domain::ports::{FilesError, ProblemReports};
use udm_domain::report::{
    self, Install, InstallFacts, Kind, Os, Program, Report, SavedReport, Summary, System, ThisCopy, LOG_LINES,
};

use crate::logging;

/// The most crash reports one run of a program writes.
pub const MAX_PER_RUN: usize = 3;

/// How many reports the folder keeps, the newest.
pub const KEEP: usize = 20;

/// The list of reports the window has offered.
const OFFERED: &str = "offered.txt";

/// How much of the end of a log is read for its last lines.
const TAIL_BYTES: u64 = 256 * 1024;

/// How much of a report is read to offer it.
const SUMMARY_BYTES: u64 = 16 * 1024;

/// Write a crash report whenever this process panics, after the hooks
/// already set (`logging::init`'s, which logs the panic, and the standard
/// one), so the report's log ends with the panic. Panics the code expects
/// and catches (`udm_domain::report::expecting_panic`) get none.
pub fn install(program: logging::Program) {
    let program = report_program(program);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        if !report::panic_expected() {
            write_crash_report(program, info, &ReportFiles::new());
        }
    }));
}

static WRITTEN: AtomicUsize = AtomicUsize::new(0);

fn write_crash_report(program: Program, info: &PanicHookInfo<'_>, files: &ReportFiles) {
    if WRITTEN.fetch_add(1, Ordering::SeqCst) >= MAX_PER_RUN {
        return;
    }
    let report = crash_report(program, info, files);
    match files.save(&report) {
        Ok(saved) => tracing::error!(path = %saved.path.display(), "wrote a crash report"),
        Err(e) => tracing::warn!(error = %e, "could not write a crash report"),
    }
}

/// The report on the panic `info` describes.
fn crash_report(program: Program, info: &PanicHookInfo<'_>, files: &ReportFiles) -> Report {
    let payload = info.payload();
    let what = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(the panic's message is not text)".to_string());
    let thread = std::thread::current();
    let (log_name, log_tail) = files.log_tail(program);
    Report {
        program,
        kind: Kind::Crash,
        copy: files.this_copy(),
        at_ms: now_ms(),
        thread: Some(thread.name().map_or_else(|| format!("{:?}", thread.id()), str::to_string)),
        what,
        location: info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())),
        // Whatever RUST_BACKTRACE says.
        backtrace: Some(Backtrace::force_capture().to_string()),
        log_name,
        log_tail,
    }
}

fn report_program(program: logging::Program) -> Program {
    match program {
        logging::Program::Daemon => Program::Daemon,
        logging::Program::Host => Program::Host,
        logging::Program::Gui => Program::Window,
        logging::Program::Cli => Program::Cli,
        logging::Program::Mcp => Program::Mcp,
    }
}

fn log_program(program: Program) -> logging::Program {
    match program {
        Program::Daemon => logging::Program::Daemon,
        Program::Host => logging::Program::Host,
        Program::Window => logging::Program::Gui,
        Program::Cli => logging::Program::Cli,
        Program::Mcp => logging::Program::Mcp,
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// `text` without the daemon's `token` (when it is long enough to be one),
/// the passwords in its addresses, or the values of its cookies,
/// authorization headers, passwords and tokens.
pub fn scrub(text: &str, token: Option<&str>) -> String {
    let text = match token.map(str::trim).filter(|t| t.len() >= 16) {
        Some(token) => text.replace(token, "***"),
        None => text.to_string(),
    };
    let text = logging::redact(&text);
    report::scrub(&text).into_owned()
}

/// The reports in their folder.
pub struct ReportFiles {
    dir: PathBuf,
}

impl Default for ReportFiles {
    fn default() -> Self {
        Self::new()
    }
}

impl ReportFiles {
    /// The reports in this copy's reports folder.
    pub fn new() -> Self {
        ReportFiles { dir: crate::paths::reports_dir() }
    }

    /// The reports in `dir`.
    pub fn in_dir(dir: PathBuf) -> Self {
        ReportFiles { dir }
    }

    fn token() -> Option<String> {
        let path = crate::ipc::token_path().ok()?;
        fs::read_to_string(path).ok()
    }

    fn scrubbed(report: &Report) -> Report {
        let token = Self::token();
        let token = token.as_deref();
        let s = |t: &str| scrub(t, token);
        Report {
            thread: report.thread.as_deref().map(s),
            what: s(&report.what),
            location: report.location.as_deref().map(s),
            backtrace: report.backtrace.as_deref().map(s),
            log_tail: report.log_tail.iter().map(|l| s(l)).collect(),
            ..report.clone()
        }
    }

    fn write(&self, report: &Report) -> io::Result<SavedReport> {
        create_private_dir(&self.dir)?;
        let mut report = Self::scrubbed(report);
        // Two threads panicking within a millisecond get two files.
        while self.dir.join(report.file_name()).exists() {
            report.at_ms += 1;
        }
        let text = report.to_text();
        let path = self.dir.join(report.file_name());
        let temp = path.with_extension("tmp");
        crate::platform::write_private(&temp, text.as_bytes())?;
        fs::rename(&temp, &path)?;
        self.prune();
        let summary = Summary::read(&report.file_name(), &text)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "the report does not read back"))?;
        Ok(SavedReport { path, summary })
    }

    /// The reports in the folder, oldest first.
    fn reports(&self) -> Vec<(String, u64)> {
        let mut found: Vec<(String, u64)> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let (_, _, at) = report::parse_file_name(&name)?;
                Some((name, at))
            })
            .collect();
        found.sort_by_key(|(_, at)| *at);
        found
    }

    /// Delete all but the newest [`KEEP`] reports.
    fn prune(&self) {
        let found = self.reports();
        for (name, _) in found.iter().take(found.len().saturating_sub(KEEP)) {
            let _ = fs::remove_file(self.dir.join(name));
        }
    }

    fn offered(&self) -> BTreeSet<String> {
        fs::read_to_string(self.dir.join(OFFERED))
            .map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect())
            .unwrap_or_default()
    }

    fn write_offered(&self, names: &BTreeSet<String>) -> io::Result<()> {
        create_private_dir(&self.dir)?;
        let text: String = names.iter().map(|n| format!("{n}\n")).collect();
        let path = self.dir.join(OFFERED);
        let temp = path.with_extension("tmp");
        crate::platform::write_private(&temp, text.as_bytes())?;
        fs::rename(&temp, &path)
    }
}

/// Make `dir`, readable only by the user on Unix.
fn create_private_dir(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    Ok(())
}

/// The first `max` bytes of the file at `path`, as text.
fn read_head(path: &Path, max: u64) -> io::Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(max).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The last `lines` lines of the file at `path`, reading at most
/// [`TAIL_BYTES`] of it.
fn read_tail(path: &Path, lines: usize) -> io::Result<Vec<String>> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let from = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(from))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut all: Vec<&str> = text.lines().collect();
    // Reading from the middle of the file starts in the middle of a line.
    if from > 0 && !all.is_empty() {
        all.remove(0);
    }
    Ok(all[all.len().saturating_sub(lines)..].iter().map(|l| l.to_string()).collect())
}

impl ProblemReports for ReportFiles {
    fn this_copy(&self) -> ThisCopy {
        this_copy()
    }

    fn log_tail(&self, program: Program) -> (String, Vec<String>) {
        let path = log_program(program).log_path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let token = Self::token();
        let lines = read_tail(&path, LOG_LINES).unwrap_or_default();
        (name, lines.iter().map(|l| scrub(l, token.as_deref())).collect())
    }

    fn save(&self, report: &Report) -> Result<SavedReport, FilesError> {
        self.write(report).map_err(|e| FilesError(format!("could not save the report in {}: {e}", self.dir.display())))
    }

    fn delete(&self, report: &SavedReport) -> Result<(), FilesError> {
        // Only a report in this folder, by its own name.
        let path = self.dir.join(report.file_name());
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                Err(FilesError(format!("could not delete {}: {e}", path.display())))
            }
            _ => Ok(()),
        }
    }

    fn unoffered(&self) -> Vec<SavedReport> {
        let offered = self.offered();
        self.reports()
            .into_iter()
            .filter(|(name, _)| !offered.contains(name))
            .filter_map(|(name, _)| {
                let path = self.dir.join(&name);
                let summary = Summary::read(&name, &read_head(&path, SUMMARY_BYTES).ok()?)?;
                Some(SavedReport { path, summary })
            })
            .collect()
    }

    fn mark_offered(&self, reports: &[SavedReport]) {
        let present: BTreeSet<String> = self.reports().into_iter().map(|(name, _)| name).collect();
        let mut offered = self.offered();
        offered.extend(reports.iter().map(SavedReport::file_name));
        // Forget the ones that were deleted or pruned.
        offered.retain(|name| present.contains(name));
        if let Err(e) = self.write_offered(&offered) {
            tracing::warn!(error = %e, "could not note the problem reports offered");
        }
    }
}

/// This copy of UDM: its version, the system and the install.
pub fn this_copy() -> ThisCopy {
    ThisCopy { version: env!("CARGO_PKG_VERSION").to_string(), system: system(), install: install_kind() }
}

fn system() -> System {
    let (desktop, session) = crate::paths::desktop_session();
    #[cfg_attr(not(any(windows, target_os = "macos", target_os = "linux")), allow(unused_mut))]
    let mut system =
        System { os: system_os(), arch: std::env::consts::ARCH.to_string(), desktop, session, ..System::default() };
    #[cfg(windows)]
    {
        const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        let read = |name| crate::registry::get_machine_string(KEY, name).ok().flatten();
        system.version = read("CurrentBuild").map(|build| format!("10.0.{build}"));
        system.release = read("DisplayVersion");
    }
    #[cfg(target_os = "macos")]
    {
        system.version = fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist")
            .ok()
            .and_then(|plist| plist_string(&plist, "ProductVersion"));
    }
    #[cfg(target_os = "linux")]
    {
        system.version = ["/etc/os-release", "/usr/lib/os-release"]
            .iter()
            .find_map(|p| fs::read_to_string(p).ok())
            .and_then(|text| os_release_name(&text));
    }
    system
}

/// The distribution's name in an `os-release` file: `PRETTY_NAME`, or
/// `NAME` and `VERSION_ID`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn os_release_name(text: &str) -> Option<String> {
    let value = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
            .map(|v| v.trim().trim_matches('"').to_string())
    };
    value("PRETTY_NAME").filter(|v| !v.is_empty()).or_else(|| {
        let name = value("NAME")?;
        Some(match value("VERSION_ID") {
            Some(v) => format!("{name} {v}"),
            None => name,
        })
    })
}

/// The `<string>` after `<key>key</key>` in a property list.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn plist_string(plist: &str, key: &str) -> Option<String> {
    let after = &plist[plist.find(&format!("<key>{key}</key>"))?..];
    let start = after.find("<string>")? + "<string>".len();
    let end = after[start..].find("</string>")?;
    Some(after[start..start + end].trim().to_string())
}

fn install_kind() -> Install {
    let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) else {
        return Install::Unknown;
    };
    let home = crate::paths::home_dir();
    let os = system_os();
    Install::detect(&facts(
        os,
        &dir,
        home.as_deref(),
        crate::paths::is_portable(),
        crate::platform::appimage().is_some(),
    ))
}

fn system_os() -> Os {
    if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::MacOs
    } else if cfg!(target_os = "linux") {
        Os::Linux
    } else {
        Os::Other
    }
}

/// What the programs' folder `dir` says about the install.
fn facts(os: Os, dir: &Path, home: Option<&Path>, portable: bool, appimage: bool) -> InstallFacts {
    let named = |p: &Path, name: &str| p.file_name().is_some_and(|n| n == name);
    let in_cargo_target =
        (named(dir, "debug") || named(dir, "release")) && dir.parent().is_some_and(|t| named(t, "target"));
    let in_app_bundle = named(dir, "MacOS")
        && dir.parent().is_some_and(|c| {
            named(c, "Contents") && c.parent().is_some_and(|app| app.extension().is_some_and(|e| e == "app"))
        });
    InstallFacts {
        os,
        portable,
        appimage,
        in_cargo_target,
        setup_uninstaller: os == Os::Windows && dir.join("unins000.exe").is_file(),
        in_app_bundle,
        in_system_lib: os == Os::Linux && dir == Path::new("/usr/lib/udm"),
        dpkg_owns: os == Os::Linux && Path::new("/var/lib/dpkg/info/udm.list").is_file(),
        in_user_lib: os == Os::Linux && home.is_some_and(|h| dir == h.join(".local/lib/udm")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("udm-crash-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn report(at_ms: u64, what: &str, log: Vec<String>) -> Report {
        Report {
            program: Program::Daemon,
            kind: Kind::Crash,
            copy: ThisCopy { version: "0.4.0".into(), ..ThisCopy::default() },
            at_ms,
            thread: Some("engine".into()),
            what: what.into(),
            location: Some("crates/x.rs:1:2".into()),
            backtrace: Some("   0: x\n".into()),
            log_name: "udm-daemon.log".into(),
            log_tail: log,
        }
    }

    #[test]
    fn scrub_hides_the_token_passwords_and_cookies() {
        let token = "0123456789abcdef0123456789abcdef";
        let text = format!(
            "token {token} in a message\nfetch ftp://bob:hunter2@h/f failed\nCookie: SESSION=s3cret\nAuthorization: Basic dXNlcjpwdw==\nok"
        );
        let out = scrub(&text, Some(&format!("{token}\n")));
        for secret in [token, "hunter2", "s3cret", "dXNlcjpwdw=="] {
            assert!(!out.contains(secret), "{secret} in {out}");
        }
        assert!(out.contains("ftp://bob:***@h/f") && out.ends_with("\nok"), "{out}");
        assert_eq!(scrub("short ab", Some("ab")), "short ab", "too short to be the token");
    }

    #[test]
    fn a_saved_report_is_scrubbed_offered_once_and_deleted() {
        let dir = temp_dir("save");
        let files = ReportFiles::in_dir(dir.clone());
        let saved = files
            .save(&report(
                1_000,
                "request with Cookie: a=b failed for ftp://u:pw1@h/x",
                vec!["password=pw2 was refused".into()],
            ))
            .unwrap();
        assert_eq!(saved.path, dir.join("udm-daemon-crash-1000.txt"));
        let text = fs::read_to_string(&saved.path).unwrap();
        for secret in ["a=b", "pw1", "pw2"] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
        assert!(text.contains("The end of udm-daemon.log:\npassword=*** was refused\n"), "{text}");
        assert_eq!(saved.summary.what, "request with Cookie: ***");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&saved.path).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
        // The same millisecond again gets a file of its own.
        let second = files.save(&report(1_000, "again", Vec::new())).unwrap();
        assert_eq!(second.summary.at_ms, 1_001);
        fs::write(dir.join("notes.txt"), "not a report").unwrap();
        let found = files.unoffered();
        assert_eq!(found, [saved.clone(), second.clone()]);
        files.mark_offered(&found[..1]);
        assert_eq!(files.unoffered(), std::slice::from_ref(&second));
        files.delete(&second).unwrap();
        assert!(!second.path.exists());
        files.delete(&second).unwrap();
        files.mark_offered(&[]);
        assert_eq!(fs::read_to_string(dir.join(OFFERED)).unwrap(), "udm-daemon-crash-1000.txt\n");
        assert!(files.unoffered().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_folder_keeps_the_newest_reports() {
        let dir = temp_dir("prune");
        let files = ReportFiles::in_dir(dir.clone());
        for at in 0..(KEEP as u64 + 3) {
            files.save(&report(at, "x", Vec::new())).unwrap();
        }
        let kept: Vec<u64> = files.reports().into_iter().map(|(_, at)| at).collect();
        assert_eq!(kept, (3..KEEP as u64 + 3).collect::<Vec<_>>());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_logs_tail_is_its_last_whole_lines() {
        let dir = temp_dir("tail");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.log");
        fs::write(&path, "one\ntwo\nthree\n").unwrap();
        assert_eq!(read_tail(&path, 2).unwrap(), ["two", "three"]);
        assert_eq!(read_tail(&path, 10).unwrap(), ["one", "two", "three"]);
        let long: String = (0..100_000).map(|i| format!("line {i}\n")).collect();
        fs::write(&path, long).unwrap();
        let tail = read_tail(&path, LOG_LINES).unwrap();
        assert_eq!(tail.len(), LOG_LINES);
        assert_eq!(tail.last().unwrap(), "line 99999");
        assert!(read_tail(&dir.join("missing.log"), 5).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_panic_hook_writes_a_report_with_the_panic() {
        let dir = temp_dir("hook");
        let files = ReportFiles::in_dir(dir.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Only this test's thread: other tests may panic on purpose.
            if std::thread::current().name() == Some("crash-hook-test") {
                let _ = tx.send(crash_report(Program::Window, info, &files));
            }
        }));
        let joined = std::thread::Builder::new()
            .name("crash-hook-test".into())
            .spawn(|| panic!("boom with password=hunter2"))
            .unwrap()
            .join();
        std::panic::set_hook(previous);
        assert!(joined.is_err());
        let report = rx.recv().unwrap();
        assert_eq!((report.program, report.kind), (Program::Window, Kind::Crash));
        assert_eq!(report.thread.as_deref(), Some("crash-hook-test"));
        assert!(report.location.as_deref().unwrap().contains("crash.rs"));
        assert!(report.backtrace.is_some());
        assert_eq!(report.copy.version, env!("CARGO_PKG_VERSION"));
        let saved = ReportFiles::in_dir(dir.clone()).save(&report).unwrap();
        let text = fs::read_to_string(&saved.path).unwrap();
        assert!(text.contains("boom with password=***") && !text.contains("hunter2"), "{text}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_system_is_read_from_its_files() {
        let ubuntu = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\n";
        assert_eq!(os_release_name(ubuntu).as_deref(), Some("Ubuntu 24.04.1 LTS"));
        assert_eq!(os_release_name("NAME=Arch\nVERSION_ID=1\n").as_deref(), Some("Arch 1"));
        assert_eq!(os_release_name("ID=x\n"), None);
        let plist = "<dict>\n\t<key>ProductName</key>\n\t<string>macOS</string>\n\t<key>ProductVersion</key>\n\t<string>14.5</string>\n</dict>";
        assert_eq!(plist_string(plist, "ProductVersion").as_deref(), Some("14.5"));
        assert_eq!(plist_string(plist, "Missing"), None);
        let copy = this_copy();
        assert_eq!(copy.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(copy.system.arch, std::env::consts::ARCH);
    }

    #[test]
    fn the_install_is_read_from_the_programs_folder() {
        let home = Path::new("/home/u");
        let at = |os, dir: &str| Install::detect(&facts(os, Path::new(dir), Some(home), false, false));
        assert_eq!(at(Os::Linux, "/src/udm/target/release"), Install::Source);
        assert_eq!(at(Os::Linux, "/home/u/.local/lib/udm"), Install::TarGz);
        assert_eq!(at(Os::MacOs, "/Applications/UDM.app/Contents/MacOS"), Install::Dmg);
        assert_eq!(at(Os::MacOs, "/opt/MacOS"), Install::Unknown);
        assert!(facts(Os::Linux, Path::new("/usr/lib/udm"), None, false, false).in_system_lib);
        assert_eq!(Install::detect(&facts(Os::Linux, Path::new("/x"), None, false, true)), Install::AppImage);
    }
}
