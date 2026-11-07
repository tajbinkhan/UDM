//! `WindowDir`, the `WindowFiles` adapter: the window's files live in its
//! folder (`paths::window_dir`) and are written atomically, in order, by
//! one writer thread.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;

use tracing::warn;
use uidm_application::window::WindowFiles;

/// A save for the writer thread: the file and its new contents.
type Write = (PathBuf, Vec<u8>);

/// The window's folder and its writer.
pub struct WindowDir {
    dir: Option<PathBuf>,
    /// Hands each save to the writer thread.
    writes: Mutex<Option<Sender<Write>>>,
}

/// Lock `m`. It holds a channel's sender, which a panicking thread cannot
/// leave half-changed.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl WindowDir {
    /// The window's files in their usual folder. Without one (outside
    /// Windows today) nothing is read or saved.
    pub fn new() -> Self {
        Self::at(crate::paths::window_dir())
    }

    /// The window's files in `dir`.
    pub fn at(dir: Option<PathBuf>) -> Self {
        WindowDir { dir, writes: Mutex::new(None) }
    }

    /// The writer thread's sender, started on first use.
    fn writer(&self) -> Sender<Write> {
        let mut writes = lock(&self.writes);
        writes
            .get_or_insert_with(|| {
                let (tx, rx) = channel::<Write>();
                thread::spawn(move || {
                    while let Ok((path, bytes)) = rx.recv() {
                        let written = path
                            .parent()
                            .map_or(Ok(()), std::fs::create_dir_all)
                            .and_then(|()| crate::settings::write_atomic(&path, &bytes));
                        if let Err(e) = written {
                            warn!(path = %path.display(), error = %e, "could not save a window file");
                        }
                    }
                });
                tx
            })
            .clone()
    }
}

impl Default for WindowDir {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowFiles for WindowDir {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.dir.as_ref()?.join(name)).ok()
    }

    fn save(&self, name: &str, bytes: Vec<u8>) {
        if let Some(dir) = &self.dir {
            let _ = self.writer().send((dir.join(name), bytes));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_save_is_read_back_and_the_later_one_wins() {
        let dir = std::env::temp_dir().join(format!("uidm_window_files_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let files = WindowDir::at(Some(dir.clone()));
        assert_eq!(files.read("window.json"), None);
        files.save("window.json", b"one".to_vec());
        files.save("window.json", b"two".to_vec());
        let until = Instant::now() + Duration::from_secs(5);
        while files.read("window.json").as_deref() != Some(b"two") {
            assert!(Instant::now() < until, "never saved: {:?}", files.read("window.json"));
            thread::sleep(Duration::from_millis(10));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_folder_nothing_is_kept() {
        let files = WindowDir::at(None);
        files.save("window.json", b"x".to_vec());
        assert_eq!(files.read("window.json"), None);
    }
}
