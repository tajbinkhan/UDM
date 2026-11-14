//! Installing a checked update file (`update::GitHub::run_setup`). Windows'
//! Setup replaces the files itself once UDM closes. On macOS and Linux UDM
//! puts the new copy in place of the running one, which keeps running from
//! the files it opened, then leaves a small shell script behind that stops
//! the old daemon once the windows have closed and starts the new copy.
//!
//! How this copy was installed (`this_install`) decides which release file
//! is used: the disk image for `UDM.app`, the AppImage for an AppImage, the
//! `.tar.gz` for a copy `install.sh` put in `~/.local/lib/udm`. A `.deb` or
//! `.rpm` copy in `/usr` is the package manager's to update.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use udm_domain::update::{Install, Installer};

/// Why an update could not be put in place.
#[derive(Debug, thiserror::Error)]
pub enum InPlaceError {
    /// What was being done, and the system's error.
    #[error("{0}: {1}")]
    Io(String, #[source] io::Error),
    /// A tool UDM ran (`tar`, `hdiutil`, `install.sh`) failed.
    #[error("{0} failed ({1})")]
    Tool(String, String),
    /// The update holds no `UDM.app`, or no `install.sh`.
    #[error("The update holds no {0}.")]
    Missing(&'static str),
    /// This copy is not installed the way the update file installs.
    #[error("This copy of UDM was not installed from {0}.")]
    NotInstalledSo(&'static str),
}

fn io_err(what: impl Into<String>) -> impl FnOnce(io::Error) -> InPlaceError {
    let what = what.into();
    move |e| InPlaceError::Io(what, e)
}

/// How this copy was installed, from where its programs are.
pub fn this_install() -> Install {
    if cfg!(windows) {
        return Install::WindowsSetup;
    }
    let exe = std::env::current_exe().ok();
    if cfg!(target_os = "macos") {
        return if exe.as_deref().and_then(app_bundle).is_some() { Install::MacApp } else { Install::Other };
    }
    if crate::paths::appimage().is_some() {
        return Install::AppImage;
    }
    let dir = exe.as_deref().and_then(Path::parent).map(Path::to_path_buf);
    linux_install(dir.as_deref(), crate::paths::user_lib_dir().as_deref())
}

/// A Linux copy's install, from its programs' folder `dir` and
/// `install.sh`'s folder `lib`.
fn linux_install(dir: Option<&Path>, lib: Option<&Path>) -> Install {
    match dir {
        Some(d) if lib.is_some_and(|l| l == d) => Install::LinuxTar,
        Some(d) if d.starts_with("/usr") => Install::LinuxPackage,
        _ => Install::Other,
    }
}

/// The `.app` bundle `exe` runs in (`.../UDM.app/Contents/MacOS/udm-gui`).
fn app_bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let is_app = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"));
    is_app.then(|| bundle.to_path_buf())
}

/// Put the update at `path` (an `installer` file) in place of this copy and
/// arrange for the new copy to start once UDM has closed.
pub fn install(installer: Installer, path: &Path) -> Result<(), InPlaceError> {
    match installer {
        Installer::WindowsSetup => run_windows_setup(path),
        Installer::AppImage => {
            let target = crate::paths::appimage().ok_or(InPlaceError::NotInstalledSo("an AppImage"))?;
            replace_appimage(path, &target)?;
            restart_after_exit(&[target.as_os_str()])
        }
        Installer::LinuxTar => {
            let lib = crate::paths::user_lib_dir().ok_or(InPlaceError::NotInstalledSo("the .tar.gz"))?;
            install_tar(path)?;
            restart_after_exit(&[lib.join("udm-gui").as_os_str()])
        }
        Installer::MacDiskImage => {
            let exe = std::env::current_exe().map_err(io_err("finding this program"))?;
            let bundle = app_bundle(&exe).ok_or(InPlaceError::NotInstalledSo("the disk image"))?;
            install_dmg(path, &bundle)?;
            restart_after_exit(&["open".as_ref(), "-n".as_ref(), bundle.as_os_str()])
        }
    }
}

fn run_windows_setup(path: &Path) -> Result<(), InPlaceError> {
    // /fromudm closes UDM without asking: the user asked for this.
    Command::new(path).arg("/fromudm=yes").spawn().map(|_| ()).map_err(io_err(format!("starting {}", path.display())))
}

/// Run `cmd`, failing with its standard error when it fails.
fn run(cmd: &mut Command, what: &str) -> Result<(), InPlaceError> {
    let out = cmd.stdin(Stdio::null()).output().map_err(io_err(format!("starting {what}")))?;
    if out.status.success() {
        return Ok(());
    }
    let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(InPlaceError::Tool(what.to_string(), if why.is_empty() { out.status.to_string() } else { why }))
}

/// A name beside `target` for a file or folder UDM is putting in place.
fn beside(target: &Path, tag: &str) -> PathBuf {
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    target.with_file_name(format!(".{name}.{tag}-{}", std::process::id()))
}

/// Copy the new AppImage next to the running one, make it runnable, and
/// rename it over the old, which the running programs keep open.
fn replace_appimage(new: &Path, target: &Path) -> Result<(), InPlaceError> {
    let temp = beside(target, "update");
    std::fs::copy(new, &temp).map_err(io_err(format!("copying the update to {}", temp.display())))?;
    let done = make_runnable(&temp).and_then(|()| std::fs::rename(&temp, target));
    if let Err(e) = done {
        let _ = std::fs::remove_file(&temp);
        return Err(InPlaceError::Io(format!("replacing {}", target.display()), e));
    }
    Ok(())
}

#[cfg(unix)]
fn make_runnable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_runnable(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Unpack the `.tar.gz` with the system's `tar` and run the `install.sh` it
/// holds, which copies the programs over the ones in `~/.local/lib/udm`.
fn install_tar(archive: &Path) -> Result<(), InPlaceError> {
    let work = archive.with_extension("unpacked");
    let _ = std::fs::remove_dir_all(&work);
    // Made fresh (failing if something put it back) and private.
    private_dir(&work).map_err(io_err(format!("making {}", work.display())))?;
    let result = (|| {
        run(Command::new("tar").arg("-xzf").arg(archive).arg("-C").arg(&work), "tar")?;
        let script = find_install_script(&work)?;
        run(Command::new("sh").arg(&script), "install.sh")
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Make the folder `dir`, which must not exist yet, readable and writable
/// by this user only.
fn private_dir(dir: &Path) -> io::Result<()> {
    let builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    builder.create(dir)
}

/// Save a checked update's `bytes` at `path` in the folder `dir`: the
/// folder is emptied and made again, private to this user, and the file
/// written as a new file there, so no one else can plant or swap it before
/// it is installed. `dir` is in the data folder (`paths::update_dir`), not
/// a shared temporary folder.
pub fn save_private(dir: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write as _;
    if path.parent() != Some(dir) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the update must be saved in its own folder"));
    }
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    private_dir(dir)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// `install.sh` in the one folder the archive unpacked to.
fn find_install_script(work: &Path) -> Result<PathBuf, InPlaceError> {
    let entries = std::fs::read_dir(work).map_err(io_err(format!("reading {}", work.display())))?;
    entries
        .flatten()
        .map(|e| e.path().join("install.sh"))
        .find(|p| p.is_file())
        .ok_or(InPlaceError::Missing("install.sh"))
}

/// Mount the disk image, copy its `UDM.app` beside `bundle`, then swap the
/// two and remove the old one.
fn install_dmg(image: &Path, bundle: &Path) -> Result<(), InPlaceError> {
    let mount = image.with_extension("mount");
    std::fs::create_dir_all(&mount).map_err(io_err(format!("making {}", mount.display())))?;
    run(
        Command::new("hdiutil")
            .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"])
            .arg(&mount)
            .arg(image),
        "hdiutil attach",
    )?;
    let fresh = beside(bundle, "update");
    let copied = (|| {
        let app = std::fs::read_dir(&mount)
            .map_err(io_err(format!("reading {}", mount.display())))?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")) && p.is_dir())
            .ok_or(InPlaceError::Missing("UDM.app"))?;
        run(Command::new("ditto").arg(&app).arg(&fresh), "ditto")
    })();
    let _ = run(Command::new("hdiutil").arg("detach").arg(&mount).arg("-quiet"), "hdiutil detach");
    let _ = std::fs::remove_dir(&mount);
    if let Err(e) = copied {
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(e);
    }
    let old = beside(bundle, "old");
    std::fs::rename(bundle, &old).map_err(io_err(format!("moving {} aside", bundle.display())))?;
    if let Err(e) = std::fs::rename(&fresh, bundle) {
        // Put the running copy back, so UDM is still there.
        let _ = std::fs::rename(&old, bundle);
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(InPlaceError::Io(format!("putting the update at {}", bundle.display()), e));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// The script left running when UDM closes for an update: it gives the
/// windows a moment to close (the list is saved already), stops this
/// daemon, waits up to ten seconds for it to go, and starts the new copy.
const RESTART: &str = r#"sleep 3
kill "$1" 2>/dev/null
i=0
while kill -0 "$1" 2>/dev/null && [ "$i" -lt 50 ]; do sleep 0.2; i=$((i + 1)); done
shift
exec "$@""#;

/// Start `program` (with its arguments) once this daemon has stopped.
fn restart_after_exit(program: &[&std::ffi::OsStr]) -> Result<(), InPlaceError> {
    Command::new("/bin/sh")
        .arg("-c")
        .arg(RESTART)
        .arg("sh")
        .arg(std::process::id().to_string())
        .args(program)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(io_err("starting the script that restarts UDM"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A new empty folder for one test.
    fn scratch(name: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("udm_in_place_{}_{name}_{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_copy_in_an_app_bundle_is_the_mac_app() {
        assert_eq!(
            app_bundle(Path::new("/Applications/UDM.app/Contents/MacOS/udm-daemon")),
            Some(PathBuf::from("/Applications/UDM.app"))
        );
        assert_eq!(app_bundle(Path::new("/Users/u/udm/target/release/udm-daemon")), None);
        assert_eq!(app_bundle(Path::new("/Applications/UDM/Contents/MacOS/udm-daemon")), None);
    }

    #[test]
    fn linux_copies_are_told_apart_by_their_folder() {
        let lib = Path::new("/home/u/.local/lib/udm");
        assert_eq!(linux_install(Some(lib), Some(lib)), Install::LinuxTar);
        assert_eq!(linux_install(Some(Path::new("/usr/bin")), Some(lib)), Install::LinuxPackage);
        assert_eq!(linux_install(Some(Path::new("/usr/lib/udm")), Some(lib)), Install::LinuxPackage);
        assert_eq!(linux_install(Some(Path::new("/home/u/src/udm/target/release")), Some(lib)), Install::Other);
        assert_eq!(linux_install(None, Some(lib)), Install::Other);
    }

    #[test]
    fn an_appimage_is_replaced_in_one_rename() {
        let dir = scratch("appimage");
        let (new, target) = (dir.join("download"), dir.join("UDM.AppImage"));
        std::fs::write(&new, b"new copy").unwrap();
        std::fs::write(&target, b"old copy").unwrap();
        replace_appimage(&new, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new copy");
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left.len(), 2, "no temporary file left: {left:?}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o755);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_update_is_saved_fresh_in_a_private_folder() {
        let base = scratch("save");
        let dir = base.join("update");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("planted"), b"left by someone").unwrap();
        let path = dir.join("UDM-Setup-0.6.0.exe");
        save_private(&dir, &path, b"MZ new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"MZ new");
        assert!(!dir.join("planted").exists(), "the folder is made again");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert!(save_private(&dir, &base.join("elsewhere.exe"), b"x").is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_install_script_is_found_in_the_unpacked_folder() {
        let dir = scratch("tar");
        assert!(matches!(find_install_script(&dir), Err(InPlaceError::Missing("install.sh"))));
        let folder = dir.join("udm-0.6.0-linux-x86_64");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("install.sh"), b"#!/bin/sh\n").unwrap();
        assert_eq!(find_install_script(&dir).unwrap(), folder.join("install.sh"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
