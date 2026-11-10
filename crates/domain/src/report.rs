//! Problem reports: what Pullbay writes down when one of its programs
//! panics, when the window's frame stops, or when the daemon stops
//! answering, and the GitHub issue a report, or Help > Report a problem,
//! starts. These are the rules only: `pullbay_infrastructure::crash` finds
//! out the system and the install, reads the logs, scrubs them and writes
//! the files, and the window offers what it finds.
//!
//! Nothing is sent anywhere from here: an issue is an address the window
//! opens in the browser when the user asks, and the user attaches the
//! report file there.

use std::borrow::Cow;
use std::cell::Cell;
use std::fmt::Write as _;
use std::path::PathBuf;

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

use crate::queue::civil_from_days;

/// Where new issues are opened.
pub const NEW_ISSUE: &str = "https://github.com/tajbinkhan/pullbay/issues/new";

/// The issue form a crash, freeze or silent daemon goes to
/// (`.github/ISSUE_TEMPLATE/crash.yml`).
pub const CRASH_FORM: &str = "crash.yml";

/// The issue form Help > Report a problem opens (`bug.yml`).
pub const BUG_FORM: &str = "bug.yml";

/// How many characters of what happened an issue's address carries. GitHub
/// refuses addresses of about 8 KB; the report file has the rest.
pub const ISSUE_TEXT_CHARS: usize = 400;

/// How many lines of a program's log a report ends with.
pub const LOG_LINES: usize = 200;

/// The Pullbay program a report is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Program {
    Window,
    Daemon,
    /// The native-messaging host the browser starts.
    Host,
    /// `pullbay-cli`.
    Cli,
    /// `pullbay-mcp`, the AI assistants' server.
    Mcp,
}

impl Program {
    pub const ALL: [Program; 5] = [Program::Window, Program::Daemon, Program::Host, Program::Cli, Program::Mcp];

    /// The program's name in a report and its file name.
    pub fn name(self) -> &'static str {
        match self {
            Program::Window => "window",
            Program::Daemon => "daemon",
            Program::Host => "host",
            Program::Cli => "cli",
            Program::Mcp => "mcp",
        }
    }

    pub fn parse(name: &str) -> Option<Program> {
        Program::ALL.into_iter().find(|p| p.name() == name)
    }

    /// The crash form's answer to "What closed or froze".
    fn form_choice(self) -> &'static str {
        match self {
            Program::Window => "The window",
            Program::Daemon => "The downloads stopped (the background service)",
            Program::Host => "The browser lost its connection to Pullbay",
            Program::Cli | Program::Mcp => "Not sure",
        }
    }
}

/// What went wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// The program panicked.
    Crash,
    /// The window's frame did not finish.
    Freeze,
    /// The daemon kept its connection open but stopped answering.
    Silent,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Crash, Kind::Freeze, Kind::Silent];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Crash => "crash",
            Kind::Freeze => "freeze",
            Kind::Silent => "silent",
        }
    }

    pub fn parse(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// The operating system family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
    #[default]
    Other,
}

/// The system a copy runs on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct System {
    pub os: Os,
    /// The processor, as Rust names it (`x86_64`, `aarch64`).
    pub arch: String,
    /// The system's own version: `10.0.22631` on Windows, `14.5` on macOS,
    /// the distribution's name on Linux.
    pub version: Option<String>,
    /// Windows' release name (`23H2`).
    pub release: Option<String>,
    /// On Linux, the desktop and the session type (`GNOME`, `wayland`).
    pub desktop: Option<String>,
    pub session: Option<String>,
}

impl System {
    /// The issue forms' answer to "System".
    pub fn form_choice(&self) -> &'static str {
        match self.os {
            Os::Windows => match self.windows_build() {
                Some(build) if build >= 22_000 => "Windows 11",
                Some(_) => "Windows 10",
                None => "Other",
            },
            Os::MacOs if self.arch == "aarch64" => "macOS (Apple Silicon)",
            Os::MacOs => "macOS (Intel)",
            Os::Linux => "Linux",
            Os::Other => "Other",
        }
    }

    /// Windows' build number, the last part of its version.
    fn windows_build(&self) -> Option<u32> {
        self.version.as_deref()?.rsplit('.').next()?.parse().ok()
    }

    /// The system in one line: its name and version, the desktop and
    /// session on Linux, and the processor.
    pub fn details(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let name = match self.os {
            Os::Windows => "Windows",
            Os::MacOs => "macOS",
            Os::Linux => "Linux",
            Os::Other => "Other",
        };
        let release = self.release.as_deref().map(|r| format!(" ({r})")).unwrap_or_default();
        parts.push(match (&self.version, self.os) {
            // A distribution's name says it is Linux.
            (Some(v), Os::Linux) => v.clone(),
            (Some(v), _) => format!("{name} {v}{release}"),
            (None, _) => name.to_string(),
        });
        parts.extend(self.desktop.clone());
        parts.extend(self.session.as_deref().map(session_name));
        if !self.arch.is_empty() {
            parts.push(self.arch.clone());
        }
        parts.join(", ")
    }
}

/// `x11` and `wayland` as people write them.
fn session_name(session: &str) -> String {
    match session.to_ascii_lowercase().as_str() {
        "x11" => "X11".to_string(),
        "wayland" => "Wayland".to_string(),
        _ => session.to_string(),
    }
}

/// How this copy of Pullbay was installed, as the issue forms ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Install {
    /// `Pullbay-Setup.exe`.
    Setup,
    /// The Windows portable zip.
    PortableZip,
    /// The macOS `.dmg`'s app.
    Dmg,
    Deb,
    Rpm,
    AppImage,
    /// The `.tar.gz` with `install.sh`.
    TarGz,
    /// `cargo build`.
    Source,
    #[default]
    Unknown,
}

impl Install {
    /// The issue forms' answer to "How you installed Pullbay", when one
    /// fits.
    pub fn form_choice(self) -> Option<&'static str> {
        Some(match self {
            Install::Setup => "Setup (Pullbay-Setup .exe)",
            Install::PortableZip => "Windows portable zip",
            Install::Dmg => "macOS .dmg",
            Install::Deb => ".deb package",
            Install::Rpm => ".rpm package",
            Install::AppImage => "AppImage",
            Install::TarGz => ".tar.gz with install.sh",
            Install::Source => "Built from source",
            Install::Unknown => return None,
        })
    }

    /// The install in a report.
    pub fn describe(self) -> &'static str {
        self.form_choice().unwrap_or("unknown")
    }

    /// The install the facts about the running programs point to.
    pub fn detect(f: &InstallFacts) -> Install {
        // A portable copy may sit in a `target\release` folder, as the zip
        // has it, so the marker comes before the build folder.
        if f.portable {
            return if f.os == Os::Windows { Install::PortableZip } else { Install::Unknown };
        }
        if f.appimage {
            return Install::AppImage;
        }
        if f.in_cargo_target {
            return Install::Source;
        }
        match f.os {
            Os::Windows if f.setup_uninstaller => Install::Setup,
            Os::MacOs if f.in_app_bundle => Install::Dmg,
            Os::Linux if f.in_system_lib => {
                if f.dpkg_owns {
                    Install::Deb
                } else {
                    Install::Rpm
                }
            }
            Os::Linux if f.in_user_lib => Install::TarGz,
            _ => Install::Unknown,
        }
    }
}

/// What the adapter found out about where the programs are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InstallFacts {
    pub os: Os,
    /// A portable copy (`portable.txt` beside the programs).
    pub portable: bool,
    /// Running from an AppImage.
    pub appimage: bool,
    /// The programs are in a Cargo `target/debug` or `target/release`.
    pub in_cargo_target: bool,
    /// Windows: Setup's uninstaller is beside the programs.
    pub setup_uninstaller: bool,
    /// macOS: the programs are inside an `.app`.
    pub in_app_bundle: bool,
    /// Linux: the programs are in `/usr/lib/pullbay`, as the packages put
    /// them.
    pub in_system_lib: bool,
    /// Linux: dpkg lists Pullbay's package.
    pub dpkg_owns: bool,
    /// Linux: the programs are in `~/.local/lib/pullbay`, as `install.sh`
    /// puts them.
    pub in_user_lib: bool,
}

/// This copy of Pullbay: its version, the system and the install.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThisCopy {
    pub version: String,
    pub system: System,
    pub install: Install,
}

/// A report, as written to its file. Its texts are scrubbed by whoever
/// makes it (see [`scrub`]); this only lays them out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub program: Program,
    pub kind: Kind,
    pub copy: ThisCopy,
    /// When, in milliseconds since 1970.
    pub at_ms: u64,
    /// The thread that panicked.
    pub thread: Option<String>,
    /// What happened: the panic's message, or what stopped.
    pub what: String,
    /// The file, line and column of the panic.
    pub location: Option<String>,
    pub backtrace: Option<String>,
    /// The log the report ends with, and its last lines.
    pub log_name: String,
    pub log_tail: Vec<String>,
}

/// The first line of every report file.
const FIRST_LINE: &str = "Pullbay problem report";
const WHAT_HEADING: &str = "What happened:";

impl Report {
    /// The report's file name: `pullbay-<program>-<kind>-<ms>.txt`.
    pub fn file_name(&self) -> String {
        file_name(self.program, self.kind, self.at_ms)
    }

    /// The report as text.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "{FIRST_LINE}");
        let _ = writeln!(out, "Program: {}", self.program.name());
        let _ = writeln!(out, "Kind: {}", self.kind.name());
        let _ = writeln!(out, "Version: {}", self.copy.version);
        let _ = writeln!(out, "System: {}", self.copy.system.details());
        let _ = writeln!(out, "Install: {}", self.copy.install.describe());
        let _ = writeln!(out, "Time: {}", utc(self.at_ms));
        if let Some(thread) = &self.thread {
            let _ = writeln!(out, "Thread: {thread}");
        }
        if let Some(location) = &self.location {
            let _ = writeln!(out, "Where: {location}");
        }
        let _ = writeln!(out, "\n{WHAT_HEADING}\n{}", self.what.trim_end());
        if let Some(backtrace) = &self.backtrace {
            let _ = writeln!(out, "\nBacktrace:\n{}", backtrace.trim_end());
        }
        let _ = writeln!(out, "\nThe end of {}:", self.log_name);
        if self.log_tail.is_empty() {
            let _ = writeln!(out, "(empty, or it could not be read)");
        }
        for line in &self.log_tail {
            let _ = writeln!(out, "{line}");
        }
        out
    }
}

/// `pullbay-<program>-<kind>-<ms>.txt`.
pub fn file_name(program: Program, kind: Kind, at_ms: u64) -> String {
    format!("pullbay-{}-{}-{at_ms}.txt", program.name(), kind.name())
}

/// The program, kind and time a report's file name gives, or `None` for a
/// file that is not a report.
pub fn parse_file_name(name: &str) -> Option<(Program, Kind, u64)> {
    let rest = name.strip_prefix("pullbay-")?.strip_suffix(".txt")?;
    let mut parts = rest.splitn(3, '-');
    let program = Program::parse(parts.next()?)?;
    let kind = Kind::parse(parts.next()?)?;
    let at = parts.next()?;
    (!at.is_empty() && at.bytes().all(|b| b.is_ascii_digit())).then_some(())?;
    Some((program, kind, at.parse().ok()?))
}

/// `at_ms` as `2026-11-10 12:34:56 UTC`.
pub fn utc(at_ms: u64) -> String {
    let secs = at_ms / 1000;
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let s = secs % 86_400;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", s / 3600, s / 60 % 60, s % 60)
}

/// A report file found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedReport {
    pub path: PathBuf,
    pub summary: Summary,
}

impl SavedReport {
    /// The file's name, which the user attaches to the issue.
    pub fn file_name(&self) -> String {
        file_name(self.summary.program, self.summary.kind, self.summary.at_ms)
    }
}

/// What the window needs of a report to offer it and to open its issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub program: Program,
    pub kind: Kind,
    pub at_ms: u64,
    /// The version that wrote it.
    pub version: Option<String>,
    /// The first line of what happened.
    pub what: String,
}

impl Summary {
    /// The summary of the report file `name` holding `text`, or `None` when
    /// it is not a report.
    pub fn read(name: &str, text: &str) -> Option<Summary> {
        let (program, kind, at_ms) = parse_file_name(name)?;
        let mut lines = text.lines();
        (lines.next()? == FIRST_LINE).then_some(())?;
        let mut version = None;
        let mut what = String::new();
        while let Some(line) = lines.next() {
            if let Some(v) = line.strip_prefix("Version: ") {
                version = Some(v.trim().to_string());
            } else if line == WHAT_HEADING {
                what = lines.next().unwrap_or_default().trim().to_string();
                break;
            }
        }
        Some(Summary { program, kind, at_ms, version, what })
    }
}

/// `text` cut to at most `max` characters, with `...` when cut.
fn cut(text: &str, max: usize) -> Cow<'_, str> {
    match text.char_indices().nth(max) {
        Some((end, _)) => Cow::Owned(format!("{}...", &text[..end])),
        None => Cow::Borrowed(text),
    }
}

/// The address of a new issue, its form filled in with `copy`'s version,
/// system and install, and for a report with what it says and a request
/// to attach its file. Only a short summary goes in the address; the
/// report's file holds the backtrace and the log.
pub fn issue_url(copy: &ThisCopy, report: Option<&SavedReport>) -> String {
    let mut fields: Vec<(&str, String)> = Vec::new();
    let version = report.and_then(|r| r.summary.version.clone()).unwrap_or_else(|| copy.version.clone());
    fields.push(("version", version));
    fields.push(("system", copy.system.form_choice().to_string()));
    fields.push(("system-details", copy.system.details()));
    if let Some(install) = copy.install.form_choice() {
        fields.push(("install", install.to_string()));
    }
    let form = match report {
        Some(r) => {
            let s = &r.summary;
            let what = cut(&s.what, ISSUE_TEXT_CHARS);
            let title = match s.kind {
                Kind::Crash => format!("The {} crashed: {}", s.program.name(), cut(&s.what, 80)),
                Kind::Freeze => "The window stopped responding".to_string(),
                Kind::Silent => "The daemon stopped answering".to_string(),
            };
            let program = match s.kind {
                Kind::Silent => Program::Daemon,
                _ => s.program,
            };
            fields.push(("title", title));
            fields.push(("program", program.form_choice().to_string()));
            fields.push((
                "what-happened",
                format!("<!-- What were you doing just before? -->\n\nPullbay's report says: {what}"),
            ));
            fields.push((
                "logs",
                format!(
                    "Pullbay saved a report, {}, written {}. Please attach it below, after looking it over.",
                    r.file_name(),
                    utc(s.at_ms)
                ),
            ));
            CRASH_FORM
        }
        None => BUG_FORM,
    };
    let mut url = format!("{NEW_ISSUE}?template={form}");
    for (name, value) in fields {
        let _ = write!(url, "&{name}={}", utf8_percent_encode(&value, NON_ALPHANUMERIC));
    }
    url
}

/// Names whose value is a secret wherever they appear as `name: value`,
/// `name=value` or `"name": "value"`. The cookie and authorization ones run
/// to the end of the line, as a header's value does.
const LINE_SECRETS: [&str; 5] = ["proxy-authorization", "authorization", "set-cookie", "cookies", "cookie"];
const WORD_SECRETS: [&str; 7] = ["password", "passwd", "token", "secret", "api_key", "apikey", "bearer"];

/// `text` with the values of cookies, authorization headers, passwords,
/// tokens and secrets replaced by `***`, for a report the user may post.
/// What it finds: `Cookie: a=b`, `cookies=...`, `"password":"..."`,
/// `?token=...&`, `Bearer abc`. A name joined to another word by `_` or `-`
/// (`login_cookies`, `access_token`) counts too; one inside a word
/// (`passwords`, `tokenize`) does not. Addresses' passwords are
/// `pullbay_infrastructure::logging::redact`'s, which reports go through
/// as well.
pub fn scrub(text: &str) -> Cow<'_, str> {
    // ASCII lowercasing keeps every byte where it was.
    let lower = text.to_ascii_lowercase();
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    for (name, to_line_end) in LINE_SECRETS.iter().map(|n| (n, true)).chain(WORD_SECRETS.iter().map(|n| (n, false))) {
        let mut from = 0;
        while let Some(at) = lower[from..].find(name) {
            let start = from + at;
            let end = start + name.len();
            from = end;
            let before = lower[..start].bytes().next_back();
            let after = lower.as_bytes().get(end).copied();
            if before.is_some_and(|b| b.is_ascii_alphanumeric())
                || after.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                continue;
            }
            if let Some(value) = value_after(text, end, to_line_end, *name == "bearer") {
                cuts.push(value);
            }
        }
    }
    if cuts.is_empty() {
        return Cow::Borrowed(text);
    }
    cuts.sort_unstable();
    let mut out = String::with_capacity(text.len());
    let mut done = 0;
    for (start, end) in cuts {
        if end <= done {
            continue;
        }
        let start = start.max(done);
        out.push_str(&text[done..start]);
        out.push_str("***");
        done = end;
    }
    out.push_str(&text[done..]);
    Cow::Owned(out)
}

/// Where the value of a secret named just before `at` lies in `text`, or
/// `None` when no value follows. `to_line_end`: an unquoted value runs to
/// the end of the line; else to the next space or separator. `spaced`: the
/// value follows a space instead of `:` or `=` (`Bearer abc`).
fn value_after(text: &str, at: usize, to_line_end: bool, spaced: bool) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut i = at;
    if spaced {
        if bytes.get(i) != Some(&b' ') {
            return None;
        }
    } else {
        // The closing quote of a quoted name.
        if matches!(bytes.get(i), Some(b'"' | b'\'')) {
            i += 1;
        }
        while bytes.get(i) == Some(&b' ') {
            i += 1;
        }
        if !matches!(bytes.get(i), Some(b':' | b'=')) {
            return None;
        }
        i += 1;
    }
    while matches!(bytes.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    let start;
    let mut end;
    if let Some(quote @ (b'"' | b'\'')) = bytes.get(i).copied() {
        start = i + 1;
        end = start;
        while let Some(&b) = bytes.get(end) {
            match b {
                b'\\' => end += 2,
                b'\n' | b'\r' => break,
                _ if b == quote => break,
                _ => end += 1,
            }
        }
        end = end.min(bytes.len());
    } else {
        start = i;
        end = start;
        while let Some(&b) = bytes.get(end) {
            let stop = if to_line_end {
                matches!(b, b'\n' | b'\r')
            } else {
                b.is_ascii_whitespace() || matches!(b, b'&' | b',' | b';' | b')' | b'"' | b'\'' | b'}' | b']')
            };
            if stop {
                break;
            }
            end += 1;
        }
    }
    // Keep the value's cut on a character boundary; the stops are ASCII,
    // so only a skipped escape can land inside one.
    while !text.is_char_boundary(end) {
        end += 1;
    }
    (end > start).then_some((start, end))
}

/// A watch on something that should finish or answer within `limit_ms`:
/// the window's frame, or the daemon's answer to a ping. The watcher calls
/// [`Watch::check`] now and then with the time and when the work under
/// watch began (`None` while there is none), and hears when it stalls and
/// when the stall ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    limit_ms: u64,
    /// When the stalled work began, while it is stalled.
    stalled: Option<u64>,
}

/// A change [`Watch::check`] saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stall {
    /// The work that began at `since` has gone on for at least the limit.
    Began { since: u64 },
    /// The stalled work ended after `lasted_ms`.
    Ended { lasted_ms: u64 },
}

impl Watch {
    pub fn new(limit_ms: u64) -> Self {
        Watch { limit_ms, stalled: None }
    }

    /// Whether the watched work is stalled now.
    pub fn stalled(&self) -> bool {
        self.stalled.is_some()
    }

    /// Look at the work at `now`: `busy_since` is when the current work
    /// began, or `None` when nothing is under way. Times are in
    /// milliseconds on any clock that does not go back.
    pub fn check(&mut self, now: u64, busy_since: Option<u64>) -> Option<Stall> {
        match (self.stalled, busy_since) {
            // The same work is still stuck.
            (Some(since), Some(busy)) if busy == since => None,
            // It finished (and other work may have begun since).
            (Some(since), _) => {
                self.stalled = None;
                Some(Stall::Ended { lasted_ms: now.saturating_sub(since) })
            }
            (None, Some(busy)) if now.saturating_sub(busy) >= self.limit_ms => {
                self.stalled = Some(busy);
                Some(Stall::Began { since: busy })
            }
            (None, _) => None,
        }
    }
}

thread_local! {
    /// How many [`expecting_panic`] calls this thread is inside.
    static EXPECTED: Cell<u32> = const { Cell::new(0) };
}

/// Run `f`, a call that may panic on purpose and whose caller catches it
/// (such as a library that panics when a system library is missing), so
/// the panic hook writes no report for it.
pub fn expecting_panic<T>(f: impl FnOnce() -> T) -> T {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            EXPECTED.with(|e| e.set(e.get().saturating_sub(1)));
        }
    }
    EXPECTED.with(|e| e.set(e.get() + 1));
    let _guard = Guard;
    f()
}

/// Whether a panic on this thread now was expected (see
/// [`expecting_panic`]).
pub fn panic_expected() -> bool {
    EXPECTED.with(|e| e.get() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linux() -> ThisCopy {
        ThisCopy {
            version: "0.4.0".into(),
            system: System {
                os: Os::Linux,
                arch: "x86_64".into(),
                version: Some("Ubuntu 24.04.1 LTS".into()),
                release: None,
                desktop: Some("GNOME".into()),
                session: Some("wayland".into()),
            },
            install: Install::Deb,
        }
    }

    fn report() -> Report {
        Report {
            program: Program::Window,
            kind: Kind::Crash,
            copy: linux(),
            at_ms: 1_762_778_096_123,
            thread: Some("main".into()),
            what: "index out of bounds: the len is 3 but the index is 7\nmore".into(),
            location: Some("crates/ui/src/lib.rs:12:5".into()),
            backtrace: Some("   0: pullbay_ui::run\n".into()),
            log_name: "pullbay-gui.log".into(),
            log_tail: vec!["line one".into(), "line two".into()],
        }
    }

    #[test]
    fn a_report_reads_back_as_its_summary() {
        let r = report();
        let name = r.file_name();
        assert_eq!(name, "pullbay-window-crash-1762778096123.txt");
        let text = r.to_text();
        assert!(text.starts_with("Pullbay problem report\nProgram: window\nKind: crash\nVersion: 0.4.0\n"), "{text}");
        assert!(text.contains("System: Ubuntu 24.04.1 LTS, GNOME, Wayland, x86_64\n"), "{text}");
        assert!(text.contains("Install: .deb package\nTime: 2025-11-10 12:34:56 UTC\nThread: main\n"), "{text}");
        assert!(text.contains("\nBacktrace:\n   0: pullbay_ui::run\n"), "{text}");
        assert!(text.ends_with("The end of pullbay-gui.log:\nline one\nline two\n"), "{text}");
        let summary = Summary::read(&name, &text).unwrap();
        assert_eq!(
            summary,
            Summary {
                program: Program::Window,
                kind: Kind::Crash,
                at_ms: 1_762_778_096_123,
                version: Some("0.4.0".into()),
                what: "index out of bounds: the len is 3 but the index is 7".into(),
            }
        );
        assert_eq!(Summary::read("notes.txt", &text), None);
        assert_eq!(Summary::read(&name, "something else"), None);
    }

    #[test]
    fn file_names_parse_only_when_they_are_reports() {
        for program in Program::ALL {
            for kind in Kind::ALL {
                assert_eq!(parse_file_name(&file_name(program, kind, 42)), Some((program, kind, 42)));
            }
        }
        for name in ["pullbay-window-crash-.txt", "pullbay-window-crash-1a.txt", "pullbay-gui-crash-1.txt", "x.txt"] {
            assert_eq!(parse_file_name(name), None, "{name}");
        }
    }

    #[test]
    fn the_system_maps_to_the_forms_choices() {
        let windows = |v: &str| System { os: Os::Windows, version: Some(v.into()), ..System::default() };
        assert_eq!(windows("10.0.22631").form_choice(), "Windows 11");
        assert_eq!(windows("10.0.19045").form_choice(), "Windows 10");
        assert_eq!(System { os: Os::Windows, ..System::default() }.form_choice(), "Other");
        let mac = |arch: &str| System { os: Os::MacOs, arch: arch.into(), ..System::default() };
        assert_eq!(mac("aarch64").form_choice(), "macOS (Apple Silicon)");
        assert_eq!(mac("x86_64").form_choice(), "macOS (Intel)");
        assert_eq!(linux().system.form_choice(), "Linux");
        let w = System { release: Some("23H2".into()), arch: "x86_64".into(), ..windows("10.0.22631") };
        assert_eq!(w.details(), "Windows 10.0.22631 (23H2), x86_64");
        assert_eq!(System::default().details(), "Other");
    }

    #[test]
    fn the_install_follows_where_the_programs_are() {
        let facts = |os| InstallFacts { os, ..InstallFacts::default() };
        assert_eq!(Install::detect(&InstallFacts { portable: true, ..facts(Os::Windows) }), Install::PortableZip);
        let zip = InstallFacts { portable: true, in_cargo_target: true, ..facts(Os::Windows) };
        assert_eq!(Install::detect(&zip), Install::PortableZip, "the zip keeps its programs in target\\release");
        assert_eq!(Install::detect(&InstallFacts { setup_uninstaller: true, ..facts(Os::Windows) }), Install::Setup);
        assert_eq!(Install::detect(&InstallFacts { in_cargo_target: true, ..facts(Os::Linux) }), Install::Source);
        assert_eq!(Install::detect(&InstallFacts { in_app_bundle: true, ..facts(Os::MacOs) }), Install::Dmg);
        assert_eq!(Install::detect(&InstallFacts { appimage: true, ..facts(Os::Linux) }), Install::AppImage);
        let system = InstallFacts { in_system_lib: true, ..facts(Os::Linux) };
        assert_eq!(Install::detect(&InstallFacts { dpkg_owns: true, ..system }), Install::Deb);
        assert_eq!(Install::detect(&system), Install::Rpm);
        assert_eq!(Install::detect(&InstallFacts { in_user_lib: true, ..facts(Os::Linux) }), Install::TarGz);
        assert_eq!(Install::detect(&facts(Os::Linux)), Install::Unknown);
        assert_eq!(Install::Unknown.form_choice(), None);
    }

    fn query(url: &str) -> Vec<(String, String)> {
        let (_, query) = url.split_once('?').unwrap();
        query
            .split('&')
            .map(|pair| {
                let (k, v) = pair.split_once('=').unwrap();
                (k.to_string(), percent_encoding::percent_decode_str(v).decode_utf8().unwrap().into_owned())
            })
            .collect()
    }

    #[test]
    fn report_a_problem_fills_in_the_bug_form() {
        let url = issue_url(&linux(), None);
        assert!(url.starts_with("https://github.com/tajbinkhan/pullbay/issues/new?template=bug.yml&"), "{url}");
        let q = query(&url);
        let get = |k: &str| q.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("version"), Some("0.4.0"));
        assert_eq!(get("system"), Some("Linux"));
        assert_eq!(get("system-details"), Some("Ubuntu 24.04.1 LTS, GNOME, Wayland, x86_64"));
        assert_eq!(get("install"), Some(".deb package"));
        let unknown = ThisCopy { install: Install::Unknown, ..linux() };
        assert!(!issue_url(&unknown, None).contains("install="));
    }

    #[test]
    fn a_reports_issue_is_short_and_asks_for_the_file() {
        let long = "x".repeat(20_000);
        let saved = SavedReport {
            path: PathBuf::from("/data/reports/pullbay-daemon-crash-1.txt"),
            summary: Summary {
                program: Program::Daemon,
                kind: Kind::Crash,
                at_ms: 1,
                version: Some("0.3.9".into()),
                what: long,
            },
        };
        let url = issue_url(&linux(), Some(&saved));
        assert!(url.contains("?template=crash.yml&"), "{url}");
        assert!(url.len() < 4_000, "{} bytes", url.len());
        let q = query(&url);
        let get = |k: &str| q.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap_or_default();
        assert_eq!(get("version"), "0.3.9", "the version that crashed");
        assert_eq!(get("program"), "The downloads stopped (the background service)");
        assert!(get("logs").contains("pullbay-daemon-crash-1.txt"), "{}", get("logs"));
        assert!(get("title").starts_with("The daemon crashed: xxx"));
        let silent = SavedReport {
            summary: Summary { program: Program::Window, kind: Kind::Silent, ..saved.summary.clone() },
            ..saved.clone()
        };
        let q = query(&issue_url(&linux(), Some(&silent)));
        assert!(q.contains(&("program".into(), "The downloads stopped (the background service)".into())));
    }

    #[test]
    fn scrub_hides_cookies_authorization_and_secrets() {
        let cases = [
            ("Cookie: SESSION=abc; theme=dark\nnext", "Cookie: ***\nnext"),
            ("set-cookie: id=1; Path=/", "set-cookie: ***"),
            ("Authorization: Basic dXNlcjpwdw==", "Authorization: ***"),
            ("proxy-authorization=Negotiate abc def", "proxy-authorization=***"),
            (r#"{"cookies":"a=b; c=d","url":"x"}"#, r#"{"cookies":"***","url":"x"}"#),
            ("login_cookies=a=b; c=d", "login_cookies=***"),
            ("password=hunter2 and more", "password=*** and more"),
            (r#"Download { password: "pw\"x", user: "bob" }"#, r#"Download { password: "***", user: "bob" }"#),
            ("https://x.example/f?access_token=abc123&n=1", "https://x.example/f?access_token=***&n=1"),
            ("sent Bearer eyJhbGciOi.x.y then", "sent Bearer *** then"),
            ("TOKEN = abc", "TOKEN = ***"),
        ];
        for (text, want) in cases {
            assert_eq!(scrub(text), want, "{text}");
        }
    }

    #[test]
    fn scrub_leaves_other_words_alone() {
        for text in [
            "two passwords forgotten",
            "the token was refused",
            "tokenize=1",
            "cookie_count=3",
            "secretary: Ann",
            "a cookie",
            "token:",
            "ünïcode cookie",
            "",
        ] {
            assert!(matches!(scrub(text), Cow::Borrowed(t) if t == text), "{text}");
        }
        // Not ASCII after the name, and an escape at the very end.
        assert_eq!(scrub("token=ä€ rest"), "token=*** rest");
        assert_eq!(scrub(r#"token="ab\"#), r#"token="***"#);
        assert_eq!(scrub("token=\"ä\\€\" rest"), "token=\"***\" rest");
    }

    #[test]
    fn a_watch_hears_a_stall_once_and_its_end() {
        let mut w = Watch::new(10_000);
        assert_eq!(w.check(5_000, None), None);
        assert_eq!(w.check(9_000, Some(1_000)), None, "not yet the limit");
        assert_eq!(w.check(11_000, Some(1_000)), Some(Stall::Began { since: 1_000 }));
        assert!(w.stalled());
        assert_eq!(w.check(30_000, Some(1_000)), None, "said once");
        assert_eq!(w.check(31_000, None), Some(Stall::Ended { lasted_ms: 30_000 }));
        assert_eq!(w.check(32_000, None), None);
        assert_eq!(w.check(50_000, Some(40_000)), Some(Stall::Began { since: 40_000 }));
        assert_eq!(w.check(51_000, Some(50_500)), Some(Stall::Ended { lasted_ms: 11_000 }), "new work began");
        assert_eq!(w.check(52_000, Some(50_500)), None);
    }

    #[test]
    fn an_expected_panic_is_known_on_its_thread_only() {
        assert!(!panic_expected());
        let inside = expecting_panic(|| (panic_expected(), std::thread::spawn(panic_expected).join().unwrap()));
        assert_eq!(inside, (true, false));
        let caught = std::panic::catch_unwind(|| expecting_panic(|| panic!("on purpose")));
        assert!(caught.is_err());
        assert!(!panic_expected(), "the count goes back down when it unwinds");
    }
}
