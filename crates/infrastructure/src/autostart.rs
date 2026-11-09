//! Starting Pullbay's window, hidden in the tray, when the user signs in:
//!
//! - Windows: the `Pullbay` value under `HKCU\...\CurrentVersion\Run`;
//! - macOS: `~/Library/LaunchAgents/com.pullbay.gui.plist`, a launchd agent;
//! - Linux: `~/.config/autostart/pullbay.desktop`, the freedesktop.org way.

use std::io;
#[cfg(unix)]
use std::path::{Path, PathBuf};

/// Add or remove the entry that starts Pullbay's window at sign-in.
pub fn set_autostart(enabled: bool) -> io::Result<()> {
    #[cfg(windows)]
    return crate::registry::set_autostart(enabled, &crate::platform::sibling_exe("pullbay-gui")?);
    #[cfg(unix)]
    {
        let (gui, first) = crate::platform::program("pullbay-gui")?;
        let path = entry_path()?;
        if !enabled {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, entry(&gui, &first))
    }
}

/// The sign-in entry UIDM, as Pullbay was called up to 0.3.3, wrote: a
/// Run value on Windows, a file in [`crate::paths::autostart_dir`] elsewhere.
#[cfg(windows)]
const UIDM_ENTRY: &str = "UIDM";
#[cfg(target_os = "macos")]
const UIDM_ENTRY: &str = "com.uidm.gui.plist";
#[cfg(all(unix, not(target_os = "macos")))]
const UIDM_ENTRY: &str = "uidm.desktop";

/// Replace the entry that started UIDM at sign-in with Pullbay's. True
/// when there was one.
pub fn move_uidm_entry() -> io::Result<bool> {
    #[cfg(windows)]
    {
        let run = crate::registry::RUN_KEY;
        if crate::registry::get_string(run, UIDM_ENTRY)?.is_none() {
            return Ok(false);
        }
        crate::registry::delete_value(run, UIDM_ENTRY)?;
    }
    #[cfg(unix)]
    {
        match std::fs::remove_file(crate::paths::autostart_dir()?.join(UIDM_ENTRY)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            other => other?,
        }
    }
    set_autostart(true)?;
    Ok(true)
}

/// Where the sign-in entry lives.
#[cfg(unix)]
fn entry_path() -> io::Result<PathBuf> {
    let dir = crate::paths::autostart_dir()?;
    #[cfg(target_os = "macos")]
    return Ok(dir.join("com.pullbay.gui.plist"));
    #[cfg(not(target_os = "macos"))]
    return Ok(dir.join("pullbay.desktop"));
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
             \t<key>Label</key>\n\t<string>com.pullbay.gui</string>\n\
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
            "[Desktop Entry]\nType=Application\nName=Pullbay\nComment=Download manager\nIcon=pullbay\n\
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
        let text = entry(Path::new("/opt/Pullbay app/pullbay-gui"), &[]);
        assert!(text.contains("--minimized"));
        if cfg!(target_os = "macos") {
            assert!(text.contains("<string>/opt/Pullbay app/pullbay-gui</string>"));
        } else {
            assert!(text.contains("Exec=\"/opt/Pullbay app/pullbay-gui\" --minimized"));
            let image = entry(Path::new("/home/me/Pullbay.AppImage"), &["pullbay-gui".to_string()]);
            assert!(image.contains("Exec=\"/home/me/Pullbay.AppImage\" \"pullbay-gui\" --minimized"), "{image}");
        }
    }
}
