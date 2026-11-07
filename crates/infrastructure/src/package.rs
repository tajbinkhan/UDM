//! Running where Windows keeps a private copy of AppData: inside an app
//! package (MSIX), or as a process a packaged app started. The Microsoft
//! Store's Firefox starting the native host is one; a packaged desktop app
//! starting the window is another, and its children can have no package
//! identity of their own yet still get the copy.
//!
//! There, new files written under `%LOCALAPPDATA%` and `%APPDATA%` go to
//! `%LOCALAPPDATA%\Packages\<family>\LocalCache`, and reads find that copy
//! before the real file. A daemon started there would keep its token, list
//! and settings apart from everyone else's, and a stale token left there
//! hides the real one from every client in that package. So:
//!
//! - [`data_copy`] finds out whether this process writes to such a copy, by
//!   writing a file and looking for it in each package's copy;
//! - [`spawn_outside`] starts the daemon through WMI (`Win32_Process.Create`),
//!   whose processes run in the user's session without the copy;
//! - the client deletes a stale token from the copy when a handshake fails.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;

/// The folder this process's writes to UIDM's data folder really go to, when
/// Windows sends them to an app package's copy; `None` when they reach the
/// real folder.
pub fn data_copy() -> Option<PathBuf> {
    let data = crate::paths::data_dir().ok()?;
    fs::create_dir_all(&data).ok()?;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let name = format!("copy-probe-{}-{nanos}.tmp", std::process::id());
    let probe = data.join(&name);
    fs::write(&probe, b"").ok()?;
    let packages = crate::paths::packages_dir()?;
    let found = fs::read_dir(packages)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path().join(r"LocalCache\Local\UIDM"))
        .find(|copy| copy.join(&name).is_file());
    let _ = fs::remove_file(&probe);
    found
}

/// Start `program` with `args` through WMI, so it runs without the package's
/// copy of AppData, with its console window hidden.
///
/// PowerShell makes the WMI call. Its script travels base64-encoded, so the
/// program's path needs no quoting for PowerShell's own parser.
pub fn spawn_outside(program: &Path, args: &[String]) -> io::Result<()> {
    let mut line = quote(&program.to_string_lossy());
    for arg in args {
        line.push(' ');
        line.push_str(&quote(arg));
    }
    let script = format!(
        "$s = New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{{ ShowWindow = [uint16]0 }}\n\
         $r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{{ CommandLine = {}; ProcessStartupInformation = $s }}\n\
         exit $r.ReturnValue",
        powershell_string(&line)
    );
    let encoded: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
        .arg(base64::engine::general_purpose::STANDARD.encode(encoded));
    let status = crate::platform::no_window(&mut cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| io::Error::new(e.kind(), format!("starting PowerShell to start {}: {e}", program.display())))?;
    if !status.success() {
        return Err(io::Error::other(format!("WMI could not start {} ({status})", program.display())));
    }
    Ok(())
}

/// `arg` quoted for a Windows command line, so the C runtime's parser gives
/// it back unchanged.
fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from('"');
    let mut slashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            slashes += 1;
        } else {
            // Backslashes before a quote are doubled, and the quote escaped.
            if c == '"' {
                out.extend(std::iter::repeat_n('\\', slashes + 1));
            }
            slashes = 0;
        }
        out.push(c);
    }
    // Backslashes before the closing quote are doubled too.
    out.extend(std::iter::repeat_n('\\', slashes));
    out.push('"');
    out
}

/// `s` as a single-quoted PowerShell string, which expands nothing.
/// PowerShell also reads the typographic single quotes as quotes, so each
/// is doubled like the plain one.
fn powershell_string(s: &str) -> String {
    let mut out = String::from('\'');
    for c in s.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::{powershell_string, quote};

    #[test]
    fn quotes_like_the_c_runtime_reads() {
        assert_eq!(quote(r"C:\UIDM\uidm-daemon.exe"), r"C:\UIDM\uidm-daemon.exe");
        assert_eq!(quote(r"C:\Program Files\UIDM\uidm-daemon.exe"), r#""C:\Program Files\UIDM\uidm-daemon.exe""#);
        assert_eq!(quote(""), r#""""#);
        assert_eq!(quote(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quote(r"a dir\"), r#""a dir\\""#);
        assert_eq!(quote(r#"a\"b c"#), r#""a\\\"b c""#);
    }

    #[test]
    fn powershell_strings_expand_nothing() {
        assert_eq!(powershell_string(r#""C:\a b\x.exe" --log"#), r#"'"C:\a b\x.exe" --log'"#);
        assert_eq!(powershell_string("it's $HOME"), "'it''s $HOME'");
        assert_eq!(powershell_string("O\u{2019}Brien"), "'O\u{2019}\u{2019}Brien'");
    }

    #[test]
    fn data_copy_cleans_up_its_probe() {
        // Either answer is right, depending on where the tests run; the
        // probe must be gone either way.
        let _ = super::data_copy();
        let data = crate::paths::data_dir().unwrap();
        let prefix = format!("copy-probe-{}-", std::process::id());
        let left = std::fs::read_dir(data)
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with(&prefix));
        assert!(!left);
    }
}
