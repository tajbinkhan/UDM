//! Moving what UDM left under its earlier names over to UDM. UDM was
//! called Pullbay in 0.4 and UIDM up to 0.3.3 ([`crate::paths::OLD_NAMES`]);
//! an update keeps the user's settings, download list and choices by moving
//! them over when a UDM program first starts:
//!
//! - the settings and data folders ([`crate::paths::old_folders`]), with
//!   the logs in them renamed and the old names' stale browser manifests and
//!   notification icons deleted. Pullbay's go first, so where both names
//!   left a file, the newer one is kept;
//! - the browsers' `com.pullbay.host` and `com.uidm.host` registrations,
//!   replaced by UDM's;
//! - the entry that started Pullbay or UIDM at sign-in, replaced by UDM's;
//! - on Windows, the names and icons the old names' notifications were
//!   shown with.
//!
//! Half-finished downloads keep their `.pullbaypart` and `.uidmpart`
//! sidecars and `.pullbayparts` and `.uidmparts` folders until the engine
//! next looks for them (`engine::resume`).
//!
//! Each step finds nothing to do once it has run. Every program runs this
//! before its log opens, since opening the log creates UDM's data folder.
//! A copy named by [`crate::paths::INSTANCE_VAR`] moves only its own
//! folders: the browsers and the sign-in entry belong to the usual copy. A
//! portable copy moves nothing, since everything the old names left belongs
//! to an installed copy.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::paths::OLD_NAMES;

/// What [`move_from_old_names`] did, for the log, which opens after it.
#[derive(Debug, Default)]
pub struct Moved {
    /// The folders moved, `(old, new)`.
    pub folders: Vec<(PathBuf, PathBuf)>,
    /// The browsers whose registration moved.
    pub browsers: Vec<&'static str>,
    /// Whether the sign-in entry moved.
    pub autostart: bool,
    /// The steps that failed, by name.
    pub failed: Vec<(&'static str, io::Error)>,
}

impl Moved {
    /// Log what moved and what could not.
    pub fn log(&self) {
        for (old, new) in &self.folders {
            tracing::info!(from = %old.display(), to = %new.display(), "moved an old name's folder");
        }
        if !self.browsers.is_empty() {
            tracing::info!(browsers = ?self.browsers, "moved the old names' browser registrations");
        }
        if self.autostart {
            tracing::info!("moved an old name's sign-in entry");
        }
        for (step, e) in &self.failed {
            tracing::warn!(step, error = %e, "could not move what an old name left");
        }
    }
}

/// Move everything Pullbay and UIDM left over to UDM.
pub fn move_from_old_names() -> Moved {
    let mut moved = Moved::default();
    for old_name in OLD_NAMES {
        match crate::paths::old_folders(old_name) {
            Ok(pairs) => {
                for (old, new) in pairs {
                    match move_folder(&old, &new, old_name) {
                        Ok(true) => moved.folders.push((old, new)),
                        Ok(false) => {}
                        Err(e) => moved.failed.push(("folder", e)),
                    }
                }
            }
            Err(e) => moved.failed.push(("folders", e)),
        }
    }
    // The old names' registrations and sign-in entries belong to an
    // installed copy.
    if crate::paths::instance().is_some() || crate::paths::is_portable() {
        return moved;
    }
    match crate::browsers::move_old_registrations() {
        Ok(browsers) => moved.browsers = browsers,
        Err(e) => moved.failed.push(("browsers", e)),
    }
    match crate::autostart::move_old_entries() {
        Ok(entry) => moved.autostart = entry,
        Err(e) => moved.failed.push(("autostart", e)),
    }
    #[cfg(windows)]
    for old_name in OLD_NAMES {
        let key = format!(r"Software\Classes\AppUserModelId\{old_name}.DownloadManager");
        if let Err(e) = crate::registry::delete_key(&key) {
            moved.failed.push(("notifications", e));
        }
    }
    moved
}

/// Move folder `old`, which UDM had under the earlier name `old_name`, to
/// `new`. When `new` is already there (a program that started first made
/// it, or an older name's folder moved in), move in each of `old`'s entries
/// that `new` lacks, then remove `old` if nothing is left in it. True when
/// anything moved.
fn move_folder(old: &Path, new: &Path, old_name: &str) -> io::Result<bool> {
    if !old.is_dir() {
        return Ok(false);
    }
    if !new.exists() {
        if let Some(parent) = new.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(old, new)?;
        tidy(new, old_name);
        return Ok(true);
    }
    let mut any = false;
    for entry in fs::read_dir(old)? {
        let entry = entry?;
        let to = new.join(entry.file_name());
        if !to.exists() {
            fs::rename(entry.path(), to)?;
            any = true;
        }
    }
    // Not empty means both had an entry of that name: UDM's is kept, and
    // the old name's stays where it was.
    let _ = fs::remove_dir(old);
    if any {
        tidy(new, old_name);
    }
    Ok(any)
}

/// Rename the logs `old_name`'s programs wrote in `dir` to UDM's names, and
/// delete the files UDM writes again under its own.
fn tidy(dir: &Path, old_name: &str) {
    let prefix = old_name.to_ascii_lowercase();
    for program in ["daemon", "host", "gui", "cli", "mcp"] {
        for end in [".log", ".log.old"] {
            let from = dir.join(format!("{prefix}-{program}{end}"));
            let to = dir.join(format!("udm-{program}{end}"));
            if from.is_file() && !to.exists() {
                let _ = fs::rename(from, to);
            }
        }
    }
    for stale in [format!("com.{prefix}.host.json"), format!("com.{prefix}.host.firefox.json"), format!("{prefix}.png")]
    {
        let _ = fs::remove_file(dir.join(stale));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("udm_from_old_names_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn pullbays_folder_becomes_udms() {
        let base = scratch("whole");
        let (old, new) = (base.join("Pullbay"), base.join("UDM"));
        fs::create_dir_all(old.join("grabber")).unwrap();
        fs::write(old.join("history.json"), b"[]").unwrap();
        fs::write(old.join("pullbay-daemon.log"), b"log").unwrap();
        fs::write(old.join("pullbay-daemon.log.old"), b"older").unwrap();
        fs::write(old.join("pullbay-mcp.log"), b"mcp").unwrap();
        fs::write(old.join("com.pullbay.host.json"), b"{}").unwrap();
        fs::write(old.join("pullbay.png"), b"png").unwrap();

        assert!(move_folder(&old, &new, "Pullbay").unwrap());
        assert!(!old.exists());
        assert_eq!(fs::read(new.join("history.json")).unwrap(), b"[]");
        assert!(new.join("grabber").is_dir());
        assert_eq!(fs::read(new.join("udm-daemon.log")).unwrap(), b"log");
        assert_eq!(fs::read(new.join("udm-daemon.log.old")).unwrap(), b"older");
        assert_eq!(fs::read(new.join("udm-mcp.log")).unwrap(), b"mcp");
        assert!(!new.join("com.pullbay.host.json").exists() && !new.join("pullbay.png").exists());
        assert!(!move_folder(&old, &new, "Pullbay").unwrap(), "nothing is left to move");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn uidms_folder_becomes_udms() {
        let base = scratch("uidm");
        let (old, new) = (base.join("UIDM"), base.join("UDM"));
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("history.json"), b"[]").unwrap();
        fs::write(old.join("uidm-gui.log"), b"log").unwrap();
        fs::write(old.join("com.uidm.host.firefox.json"), b"{}").unwrap();

        assert!(move_folder(&old, &new, "UIDM").unwrap());
        assert_eq!(fs::read(new.join("udm-gui.log")).unwrap(), b"log");
        assert!(!new.join("com.uidm.host.firefox.json").exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn an_old_names_entries_join_a_udm_folder_that_is_already_there() {
        let base = scratch("merge");
        let (old, new) = (base.join("Pullbay"), base.join("UDM"));
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("history.json"), b"old list").unwrap();
        fs::write(old.join("settings.json"), b"old settings").unwrap();
        fs::write(new.join("settings.json"), b"new settings").unwrap();
        fs::write(new.join("udm-host.log"), b"host").unwrap();

        assert!(move_folder(&old, &new, "Pullbay").unwrap());
        assert_eq!(fs::read(new.join("history.json")).unwrap(), b"old list");
        assert_eq!(fs::read(new.join("settings.json")).unwrap(), b"new settings", "UDM's own wins");
        assert_eq!(fs::read(old.join("settings.json")).unwrap(), b"old settings", "Pullbay's stays put");
        assert!(new.join("udm-host.log").is_file());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn pullbays_files_win_over_uidms() {
        let base = scratch("both");
        let (pullbay, uidm, new) = (base.join("Pullbay"), base.join("UIDM"), base.join("UDM"));
        fs::create_dir_all(&pullbay).unwrap();
        fs::create_dir_all(&uidm).unwrap();
        fs::write(pullbay.join("settings.json"), b"pullbay").unwrap();
        fs::write(uidm.join("settings.json"), b"uidm").unwrap();
        fs::write(uidm.join("window.json"), b"uidm window").unwrap();

        assert!(move_folder(&pullbay, &new, "Pullbay").unwrap());
        assert!(move_folder(&uidm, &new, "UIDM").unwrap());
        assert_eq!(fs::read(new.join("settings.json")).unwrap(), b"pullbay");
        assert_eq!(fs::read(new.join("window.json")).unwrap(), b"uidm window");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn no_old_folder_moves_nothing() {
        let base = scratch("none");
        assert!(!move_folder(&base.join("Pullbay"), &base.join("UDM"), "Pullbay").unwrap());
        assert!(!base.join("UDM").exists());
        fs::remove_dir_all(&base).unwrap();
    }
}
