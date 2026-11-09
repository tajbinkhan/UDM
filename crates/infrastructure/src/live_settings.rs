//! The settings a running daemon works with, and the file they are kept in.
//!
//! `LiveSettings` holds the current `Settings` behind a lock that every
//! adapter reading them shares, and remembers `settings.json`'s path. It is
//! the adapter for `UpdateChecks`. `QueueChimes` plays the queue sounds the
//! settings ask for (`QueueSounds`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use uidm_domain::sync::LockExt;

use tracing::warn;
use uidm_domain::ports::{QueueSounds, UpdateChecks};

use crate::notify;
use crate::settings;
use uidm_domain::settings::Settings;
use uidm_domain::settings::Sound;

/// The current settings, and where they are saved. With no path they live
/// in memory only.
pub struct LiveSettings {
    current: Mutex<Settings>,
    path: Option<PathBuf>,
}

impl LiveSettings {
    pub fn new(settings: Settings, path: Option<PathBuf>) -> Self {
        LiveSettings { current: Mutex::new(settings), path }
    }

    /// The settings, locked. A thread that panicked while holding them left
    /// whole `Settings` values behind (every change replaces a field or the
    /// whole value), so a poisoned lock is still read.
    pub fn lock(&self) -> MutexGuard<'_, Settings> {
        self.current.lock_or_recover()
    }

    /// A copy of the settings as they are now.
    pub fn get(&self) -> Settings {
        self.lock().clone()
    }

    /// `settings.json`, when the settings are kept in a file.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Write `settings` to the file, if there is one.
    pub fn save(&self, settings: &Settings) -> std::io::Result<()> {
        match &self.path {
            Some(path) => settings::save(settings, path),
            None => Ok(()),
        }
    }
}

impl UpdateChecks for LiveSettings {
    fn weekly(&self) -> (bool, u64) {
        let s = self.lock();
        (s.check_updates, s.last_update_check)
    }

    fn note_checked(&self, at: u64, save: bool) {
        let mut s = self.lock();
        s.last_update_check = at;
        if save {
            if let Err(e) = self.save(&s) {
                warn!(error = %e, "could not save when updates were checked");
            }
        }
    }
}

/// The queue sounds, as the settings choose them.
pub struct QueueChimes(pub Arc<LiveSettings>);

impl QueueSounds for QueueChimes {
    fn queue_started(&self) {
        let settings = self.0.get();
        if settings.sound_on_queue_started {
            notify::play(Sound::QueueStarted, settings.sound_file(Sound::QueueStarted).as_deref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("uidm_live_settings_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_noted_check_is_saved_only_when_asked() {
        let dir = temp_dir("noted");
        let path = dir.join("settings.json");
        let live = LiveSettings::new(Settings::default(), Some(path.clone()));
        live.note_checked(100, false);
        assert_eq!(live.weekly().1, 100);
        assert!(!path.exists(), "a check not asked to be saved writes nothing");
        live.note_checked(200, true);
        assert_eq!(settings::load(&path).unwrap().last_update_check, 200);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_a_file_the_settings_live_in_memory() {
        let live = LiveSettings::new(Settings { check_updates: false, ..Settings::default() }, None);
        live.note_checked(5, true);
        assert_eq!(live.weekly(), (false, 5));
        assert!(live.path().is_none());
        assert!(live.save(&live.get()).is_ok());
    }
}
