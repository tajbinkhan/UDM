//! Starting UDM's window, hidden in the tray, when the user signs in:
//!
//! - Windows: the `UDM` value under `HKCU\...\CurrentVersion\Run`;
//! - macOS: `~/Library/LaunchAgents/com.udm.gui.plist`, a launchd agent;
//! - Linux: `~/.config/autostart/udm.desktop`, the freedesktop.org way.
//!
//! A portable copy (`paths::portable_root`) has none, so it leaves the
//! computer as it found it.

use std::io;
#[cfg(unix)]
use std::path::{Path, PathBuf};

/// Add or remove the entry that starts UDM's window at sign-in. A
/// portable copy refuses to add one and removes none, since an entry there
/// would be the installed copy's.
pub fn set_autostart(enabled: bool) -> io::Result<()> {
    if crate::paths::is_portable() {
        return match enabled {
            true => Err(io::Error::new(io::ErrorKind::Unsupported, "a portable copy does not start when you sign in")),
            false => Ok(()),
        };
    }
    #[cfg(windows)]
    return crate::registry::set_autostart(enabled, &crate::platform::sibling_exe("udm-gui")?);
    #[cfg(unix)]
    {
        let (gui, first) = crate::platform::program("udm-gui")?;
        let path = entry_path()?;
        if !enabled {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, entry(&gui, &first))
    }
}

/// The sign-in entries UDM wrote under its earlier names, Pullbay (0.4) and
/// UIDM (up to 0.3.3): Run values on Windows, files in
/// [`crate::paths::autostart_dir`] elsewhere.
#[cfg(windows)]
const OLD_ENTRIES: [&str; 2] = ["Pullbay", "UIDM"];
#[cfg(target_os = "macos")]
const OLD_ENTRIES: [&str; 2] = ["com.pullbay.gui.plist", "com.uidm.gui.plist"];
#[cfg(all(unix, not(target_os = "macos")))]
const OLD_ENTRIES: [&str; 2] = ["pullbay.desktop", "uidm.desktop"];

/// Replace the entries that started Pullbay or UIDM at sign-in with UDM's.
/// True when there was one.
pub fn move_old_entries() -> io::Result<bool> {
    let mut any = false;
    for old in OLD_ENTRIES {
        any |= drop_old_entry(old)?;
    }
    if any {
        set_autostart(true)?;
    }
    Ok(any)
}

/// Remove the old sign-in entry `old`. True when there was one.
fn drop_old_entry(old: &str) -> io::Result<bool> {
    #[cfg(windows)]
    {
        let run = crate::registry::RUN_KEY;
        if crate::registry::get_string(run, old)?.is_none() {
            return Ok(false);
        }
        crate::registry::delete_value(run, old)?;
        Ok(true)
    }
    #[cfg(unix)]
    {
        match std::fs::remove_file(crate::paths::autostart_dir()?.join(old)) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}

/// Where the sign-in entry lives.
#[cfg(unix)]
fn entry_path() -> io::Result<PathBuf> {
    let dir = crate::paths::autostart_dir()?;
    #[cfg(target_os = "macos")]
    return Ok(dir.join("com.udm.gui.plist"));
    #[cfg(not(target_os = "macos"))]
    return Ok(dir.join("udm.desktop"));
}

/// The sign-in entry that starts `gui` with `first` (the program's name
/// when `gui` is an AppImage), then `--minimized`.
#[cfg(unix)]
fn entry(gui: &Path, first: &[String]) -> String {
    #[cfg(target_os = "macos")]
    {
        let escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n<dict>\n\
             \t<key>Label</key>\n\t<string>com.udm.gui</string>\n\
             \t<key>ProgramArguments</key>\n\t<array>\n{}\t</array>\n\
             \t<key>RunAtLoad</key>\n\t<true/>\n\
             </dict>\n</plist>\n",
            std::iter::once(gui.display().to_string())
                .chain(first.iter().cloned())
                .chain(["--minimized".to_string()])
                .map(|a| format!("\t\t<string>{}</string>\n", escape(&a)))
                .collect::<String>()
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Exec quotes each argument: a space or quote in it stays part of it.
        let quote = |s: &str| {
            format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('`', "\\`").replace('$', "\\$"))
        };
        let exec: Vec<String> =
            std::iter::once(quote(&gui.display().to_string())).chain(first.iter().map(|a| quote(a))).collect();
        format!(
            "[Desktop Entry]\nType=Application\nName=UDM\nComment=Download manager\nIcon=udm\n\
             Exec={} --minimized\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            exec.join(" ")
        )
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn the_entry_starts_the_window_minimized() {
        let text = entry(Path::new("/opt/UDM app/udm-gui"), &[]);
        assert!(text.contains("--minimized"));
        if cfg!(target_os = "macos") {
            assert!(text.contains("<string>/opt/UDM app/udm-gui</string>"));
        } else {
            assert!(text.contains("Exec=\"/opt/UDM app/udm-gui\" --minimized"));
            let image = entry(Path::new("/home/me/UDM.AppImage"), &["udm-gui".to_string()]);
            assert!(image.contains("Exec=\"/home/me/UDM.AppImage\" \"udm-gui\" --minimized"), "{image}");
        }
    }
}
