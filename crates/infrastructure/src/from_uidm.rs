//! Moving what UIDM left to Pullbay. Pullbay was called UIDM up to version
//! 0.3.3; an update keeps the user's settings, download list and choices by
//! moving them over when a Pullbay program first starts:
//!
//! - the settings and data folders ([`crate::paths::uidm_folders`]), with
//!   the logs in them renamed and UIDM's stale browser manifests and
//!   notification icon deleted;
//! - the browsers' `com.uidm.host` registrations, replaced by Pullbay's;
//! - the entry that started UIDM at sign-in, replaced by Pullbay's;
//! - on Windows, the name and icon UIDM's notifications were shown with.
//!
//! Half-finished downloads keep their `.uidmpart` sidecars and `.uidmparts`
//! folders until the engine next looks for them (`engine::resume`).
//!
//! Each step finds nothing to do once it has run. Every program runs this
//! before its log opens, since opening the log creates Pullbay's data
//! folder. A copy named by [`crate::paths::INSTANCE_VAR`] moves only its own
//! folders: the browsers and the sign-in entry belong to the usual copy.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// What [`move_from_uidm`] did, for the log, which opens after it.
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
            tracing::info!(from = %old.display(), to = %new.display(), "moved UIDM's folder");
        }
        if !self.browsers.is_empty() {
            tracing::info!(browsers = ?self.browsers, "moved UIDM's browser registrations");
        }
        if self.autostart {
            tracing::info!("moved UIDM's sign-in entry");
        }
        for (step, e) in &self.failed {
            tracing::warn!(step, error = %e, "could not move what UIDM left");
        }
    }
}

/// Move everything UIDM left over to Pullbay.
pub fn move_from_uidm() -> Moved {
    let mut moved = Moved::default();
    match crate::paths::uidm_folders() {
        Ok(pairs) => {
            for (old, new) in pairs {
                match move_folder(&old, &new) {
                    Ok(true) => moved.folders.push((old, new)),
                    Ok(false) => {}
                    Err(e) => moved.failed.push(("folder", e)),
                }
            }
        }
        Err(e) => moved.failed.push(("folders", e)),
    }
    if crate::paths::instance().is_some() {
        return moved;
    }
    match crate::browsers::move_uidm_registrations() {
        Ok(browsers) => moved.browsers = browsers,
        Err(e) => moved.failed.push(("browsers", e)),
    }
    match crate::autostart::move_uidm_entry() {
        Ok(entry) => moved.autostart = entry,
        Err(e) => moved.failed.push(("autostart", e)),
    }
    #[cfg(windows)]
    if let Err(e) = crate::registry::delete_key(r"Software\Classes\AppUserModelId\UIDM.DownloadManager") {
        moved.failed.push(("notifications", e));
    }
    moved
}

/// Move folder `old` to `new`. When `new` is already there (a program that
/// started first made it), move in each of `old`'s entries that `new` lacks,
/// then remove `old` if nothing is left in it. True when anything moved.
fn move_folder(old: &Path, new: &Path) -> io::Result<bool> {
    if !old.is_dir() {
        return Ok(false);
    }
    if !new.exists() {
        if let Some(parent) = new.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(old, new)?;
        tidy(new);
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
    // Not empty means both had an entry of that name: Pullbay's is kept,
    // and UIDM's stays where it was.
    let _ = fs::remove_dir(old);
    if any {
        tidy(new);
    }
    Ok(any)
}

/// Rename UIDM's logs in `dir` to Pullbay's names, and delete the files
/// Pullbay writes again under its own.
fn tidy(dir: &Path) {
    for program in ["daemon", "host", "gui"] {
        for end in [".log", ".log.old"] {
            let from = dir.join(format!("uidm-{program}{end}"));
            let to = dir.join(format!("pullbay-{program}{end}"));
            if from.is_file() && !to.exists() {
                let _ = fs::rename(from, to);
            }
        }
    }
    for stale in ["com.uidm.host.json", "com.uidm.host.firefox.json", "uidm.png"] {
        let _ = fs::remove_file(dir.join(stale));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pullbay_from_uidm_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn uidms_folder_becomes_pullbays() {
        let base = scratch("whole");
        let (old, new) = (base.join("UIDM"), base.join("Pullbay"));
        fs::create_dir_all(old.join("grabber")).unwrap();
        fs::write(old.join("history.json"), b"[]").unwrap();
        fs::write(old.join("uidm-daemon.log"), b"log").unwrap();
        fs::write(old.join("uidm-daemon.log.old"), b"older").unwrap();
        fs::write(old.join("com.uidm.host.json"), b"{}").unwrap();
        fs::write(old.join("uidm.png"), b"png").unwrap();

        assert!(move_folder(&old, &new).unwrap());
        assert!(!old.exists());
        assert_eq!(fs::read(new.join("history.json")).unwrap(), b"[]");
        assert!(new.join("grabber").is_dir());
        assert_eq!(fs::read(new.join("pullbay-daemon.log")).unwrap(), b"log");
        assert_eq!(fs::read(new.join("pullbay-daemon.log.old")).unwrap(), b"older");
        assert!(!new.join("com.uidm.host.json").exists() && !new.join("uidm.png").exists());
        assert!(!move_folder(&old, &new).unwrap(), "nothing is left to move");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn uidms_entries_join_a_pullbay_folder_that_is_already_there() {
        let base = scratch("merge");
        let (old, new) = (base.join("UIDM"), base.join("Pullbay"));
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("history.json"), b"old list").unwrap();
        fs::write(old.join("settings.json"), b"old settings").unwrap();
        fs::write(new.join("settings.json"), b"new settings").unwrap();
        fs::write(new.join("pullbay-host.log"), b"host").unwrap();

        assert!(move_folder(&old, &new).unwrap());
        assert_eq!(fs::read(new.join("history.json")).unwrap(), b"old list");
        assert_eq!(fs::read(new.join("settings.json")).unwrap(), b"new settings", "Pullbay's own wins");
        assert_eq!(fs::read(old.join("settings.json")).unwrap(), b"old settings", "UIDM's stays put");
        assert!(new.join("pullbay-host.log").is_file());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn no_uidm_folder_moves_nothing() {
        let base = scratch("none");
        assert!(!move_folder(&base.join("UIDM"), &base.join("Pullbay")).unwrap());
        assert!(!base.join("Pullbay").exists());
        fs::remove_dir_all(&base).unwrap();
    }
}
