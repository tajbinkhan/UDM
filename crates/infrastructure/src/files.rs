//! The files a download leaves on disk: moving them when the user renames
//! or moves a download, recycling them when it is removed with its files,
//! and the resume records kept beside partial files.

use std::path::{Path, PathBuf};

use udm_domain::ports::{DownloadFiles, FilesError};

use crate::engine::{files_of, partial_files, sidecar_path};
use crate::recycle::recycle;

/// The [`DownloadFiles`] port over the disk and the Recycle Bin (or Trash).
pub struct DiskFiles;

impl DownloadFiles for DiskFiles {
    fn recycle(&self, path: Option<&str>, partial: Option<&str>) -> Result<(), FilesError> {
        let mut files = path.map(|p| files_of(Path::new(p))).unwrap_or_default();
        files.extend(partial.map(|p| partial_files(Path::new(p))).unwrap_or_default());
        if files.is_empty() {
            return Ok(());
        }
        recycle(&files).map_err(|e| FilesError(e.to_string()))
    }

    fn forget_resume(&self, paths: &[&str]) {
        for p in paths {
            // A record that is not there is already forgotten.
            let _ = std::fs::remove_file(sidecar_path(Path::new(p)));
        }
    }

    fn move_finished(&self, from: &Path, to: &Path) -> Result<(), FilesError> {
        move_file(from, to)
    }

    fn move_partial(&self, from: &Path, to: &Path) -> Result<(), FilesError> {
        move_partial(from, to)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn can_write(&self, dir: &Path) -> bool {
        crate::elevate::can_write(dir)
    }
}

/// Move a finished file to `to`, which must not exist yet. Across drives
/// this copies the file, then deletes the original.
pub fn move_file(from: &Path, to: &Path) -> Result<(), FilesError> {
    let fail = |msg: String| Err(FilesError(msg));
    if !from.is_file() {
        return fail(format!("The file is no longer at {}.", from.display()));
    }
    if to.exists() {
        return fail(format!("{} already exists.", to.display()));
    }
    if let Some(dir) = to.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return fail(format!("Could not create {}: {e}", dir.display()));
        }
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if let Err(e) = std::fs::copy(from, to) {
        return fail(format!("Could not move the file to {}: {e}", to.display()));
    }
    std::fs::remove_file(from)
        .map_err(|e| FilesError(format!("Copied the file, but could not delete {}: {e}", from.display())))
}

/// Move an unfinished download's file `from` and its resume record to
/// `to`, refusing to land on any file already there. Either may be missing:
/// a download that never started has no file yet, and one writing to the
/// temporary folder keeps its data there.
pub fn move_partial(from: &Path, to: &Path) -> Result<(), FilesError> {
    let (sidecar, new_sidecar) = (sidecar_path(from), sidecar_path(to));
    for taken in [to, new_sidecar.as_path()] {
        if taken.exists() {
            return Err(FilesError(format!("{} already exists.", taken.display())));
        }
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| FilesError(format!("Could not create {}: {e}", dir.display())))?;
    }
    if from.is_file() {
        move_file(from, to)?;
    }
    if sidecar.is_file() {
        move_file(&sidecar, &new_sidecar)?;
    }
    Ok(())
}

impl udm_domain::ports::SaveFolders for DiskFiles {
    fn downloads_dir(&self) -> PathBuf {
        crate::settings::default_downloads_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("udm_files_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_finished_file_moves_but_never_over_another() {
        let dir = temp_dir("finished");
        let (a, b) = (dir.join("a.zip"), dir.join("sub").join("b.zip"));
        fs::write(&a, b"x").unwrap();
        DiskFiles.move_finished(&a, &b).unwrap();
        assert!(!a.exists() && b.is_file());
        fs::write(&a, b"y").unwrap();
        assert!(DiskFiles.move_finished(&a, &b).unwrap_err().0.ends_with("already exists."));
        assert!(DiskFiles.move_finished(&dir.join("gone.zip"), &dir.join("c.zip")).is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_partial_file_moves_with_its_resume_record() {
        let dir = temp_dir("partial");
        let (from, to) = (dir.join("a.zip"), dir.join("new").join("a.zip"));
        fs::write(&from, b"part").unwrap();
        fs::write(sidecar_path(&from), b"{}").unwrap();
        DiskFiles.move_partial(&from, &to).unwrap();
        assert!(to.is_file() && sidecar_path(&to).is_file());
        assert!(!from.exists() && !sidecar_path(&from).exists());
        // One that never started has nothing to move.
        DiskFiles.move_partial(&dir.join("none.zip"), &dir.join("new").join("none.zip")).unwrap();
        fs::remove_dir_all(&dir).ok();
    }
}
