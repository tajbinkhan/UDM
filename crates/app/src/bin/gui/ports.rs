//! The ports the window's views reach beyond the daemon connection: the
//! clock, the save folders, opening files and the window's own files.
//!
//! `main` installs them once, before the window opens, with the adapters
//! from `uidm_infrastructure`. Views read them through [`ports`]. This is a
//! step on the way to #199, where views get what they need through
//! `AppState` instead.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use uidm_application::window::WindowFiles;
use uidm_domain::ports::{Clock, DownloadFiles, FileOpener, SaveFolders, Timestamps};
use uidm_domain::queue::Moment;

/// What the views reach through ports.
pub struct Ports {
    /// The local date and time.
    pub clock: Arc<dyn Clock>,
    /// The time as seconds since 1970.
    pub times: Arc<dyn Timestamps>,
    /// The system's Downloads folder.
    pub folders: Arc<dyn SaveFolders>,
    /// Whether UIDM may write to a folder.
    pub files: Arc<dyn DownloadFiles>,
    /// Opening and showing files.
    pub opener: Arc<dyn FileOpener>,
    /// The window's layout and drop target files.
    pub window_files: Arc<dyn WindowFiles>,
}

impl Ports {
    /// The local date and time now.
    pub fn now(&self) -> Moment {
        self.clock.now()
    }

    /// Seconds since 1970 now.
    pub fn unix_now(&self) -> u64 {
        self.times.unix_now()
    }

    /// The system's Downloads folder.
    pub fn downloads_dir(&self) -> PathBuf {
        self.folders.downloads_dir()
    }

    /// Whether UIDM may write to `folder` without administrator rights.
    pub fn can_write(&self, folder: &Path) -> bool {
        self.files.can_write(folder)
    }
}

static PORTS: OnceLock<Ports> = OnceLock::new();

/// Install the ports. Only the first call counts.
pub fn install(ports: Ports) {
    let _ = PORTS.set(ports);
}

/// The ports `main` installed.
pub fn ports() -> &'static Ports {
    #[cfg(test)]
    return PORTS.get_or_init(fakes::ports);
    // `main` installs the ports before it opens the window, and only the
    // window's frames and the threads it starts call this.
    #[cfg(not(test))]
    #[allow(clippy::expect_used)]
    PORTS.get().expect("the ports are installed before the window opens")
}

/// Stand-ins for the tests: UTC as the local time, a Downloads folder in
/// the temporary folder, every folder writable, and no files opened or
/// saved.
#[cfg(test)]
mod fakes {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use uidm_domain::ports::FilesError;

    struct Utc;

    fn unix() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
    }

    impl Clock for Utc {
        fn now(&self) -> Moment {
            let minutes = unix() / 60;
            Moment { day: (minutes / 1440) as i64, minute: (minutes % 1440) as u32 }
        }
    }

    impl Timestamps for Utc {
        fn unix_now(&self) -> u64 {
            unix()
        }
    }

    struct Temp;

    impl SaveFolders for Temp {
        fn downloads_dir(&self) -> PathBuf {
            std::env::temp_dir().join("uidm-test-downloads")
        }
    }

    impl DownloadFiles for Temp {
        fn recycle(&self, _: Option<&str>, _: Option<&str>) -> Result<(), FilesError> {
            Ok(())
        }
        fn forget_resume(&self, _: &[&str]) {}
        fn move_finished(&self, _: &Path, _: &Path) -> Result<(), FilesError> {
            Ok(())
        }
        fn move_partial(&self, _: &Path, _: &Path) -> Result<(), FilesError> {
            Ok(())
        }
        fn exists(&self, path: &Path) -> bool {
            path.exists()
        }
        fn is_file(&self, path: &Path) -> bool {
            path.is_file()
        }
        fn can_write(&self, _: &Path) -> bool {
            true
        }
    }

    impl FileOpener for Temp {
        fn open(&self, _: &str) {}
        fn reveal(&self, _: &str) {}
    }

    impl WindowFiles for Temp {
        fn read(&self, _: &str) -> Option<Vec<u8>> {
            None
        }
        fn save(&self, _: &str, _: Vec<u8>) {}
    }

    pub fn ports() -> Ports {
        Ports {
            clock: Arc::new(Utc),
            times: Arc::new(Utc),
            folders: Arc::new(Temp),
            files: Arc::new(Temp),
            opener: Arc::new(Temp),
            window_files: Arc::new(Temp),
        }
    }
}
