//! Saving into folders that need administrator rights (`C:\Program Files`).
//!
//! UIDM checks a folder before a download starts. When it cannot write
//! there, the user can save there with administrator rights: the download
//! itself still runs as the user, in a staging folder, and only the finished
//! file is moved into place by `uidm-daemon --move-elevated`, which Windows
//! starts after a UAC prompt. IDM does the same with its elevated helper,
//! `idmfsa.dll`.

use std::fs;
use std::io;
use std::path::Path;

/// The argument that makes `uidm-daemon` move one file and exit, instead of
/// running as the daemon: `uidm-daemon --move-elevated <from> <to>`.
pub const MOVE_ARG: &str = "--move-elevated";

/// Whether this system can ask for administrator rights to save a file.
pub fn supported() -> bool {
    cfg!(windows)
}

/// Whether UIDM can create files in `dir`. A missing folder is judged by
/// the nearest folder above it that exists, where UIDM would have to make
/// it; nothing is left behind. Only a refusal counts: any other error (a
/// missing drive, a full disk) is left for the download to report.
pub fn can_write(dir: &Path) -> bool {
    let Some(existing) = dir.ancestors().find(|d| d.is_dir()) else {
        return true;
    };
    match try_write(existing) {
        Ok(()) => true,
        Err(e) => e.kind() != io::ErrorKind::PermissionDenied,
    }
}

fn try_write(dir: &Path) -> io::Result<()> {
    let probe = dir.join(format!(".uidm-write-check-{}", std::process::id()));
    fs::OpenOptions::new().write(true).create_new(true).open(&probe)?;
    fs::remove_file(&probe)
}

/// The helper's work: copy `from` to `to` and delete `from`. A copy, not a
/// rename, so the file takes the permissions of the folder it lands in
/// rather than keeping those of the user's temporary folder.
pub fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::copy(from, to)?;
    // The helper runs as an administrator, but the source is the user's own
    // file, so it can always go; a leftover copy is harmless.
    let _ = fs::remove_file(from);
    Ok(())
}

/// Why a move with administrator rights did not happen.
#[derive(Debug, PartialEq)]
pub enum MoveError {
    /// The user said No to the UAC prompt.
    Refused,
    /// The helper did not start, or failed; the text says why.
    Failed(String),
}

/// Move `from` to `to` through the helper, which Windows starts with
/// administrator rights after asking the user. Blocks until it ends.
pub fn move_as_admin(from: &Path, to: &Path) -> Result<(), MoveError> {
    #[cfg(windows)]
    return win::move_as_admin(from, to);
    #[cfg(not(windows))]
    {
        let _ = (from, to);
        Err(MoveError::Failed("this system has no way to ask for administrator rights".into()))
    }
}

/// `arg` quoted for a Windows command line, as `CommandLineToArgvW` reads it.
#[cfg_attr(not(windows), allow(dead_code))]
fn quote(arg: &str) -> String {
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(slashes));
                out.push(c);
                slashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use super::{quote, MoveError, MOVE_ARG};

    #[repr(C)]
    struct ShellExecuteInfo {
        size: u32,
        mask: u32,
        hwnd: isize,
        verb: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
        inst_app: isize,
        id_list: *mut c_void,
        class: *const u16,
        hkey_class: isize,
        hot_key: u32,
        icon_or_monitor: isize,
        process: isize,
    }

    const SEE_MASK_NOCLOSEPROCESS: u32 = 0x0000_0040;
    const SEE_MASK_NOASYNC: u32 = 0x0000_0100;
    const SW_HIDE: i32 = 0;
    const INFINITE: u32 = 0xFFFF_FFFF;
    const ERROR_CANCELLED: i32 = 1223;

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteExW(info: *mut ShellExecuteInfo) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn WaitForSingleObject(handle: isize, ms: u32) -> u32;
        fn GetExitCodeProcess(process: isize, code: *mut u32) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(Some(0)).collect()
    }

    pub fn move_as_admin(from: &Path, to: &Path) -> Result<(), MoveError> {
        let exe = crate::platform::sibling_exe("uidm-daemon").map_err(|e| MoveError::Failed(e.to_string()))?;
        let params = format!("{MOVE_ARG} {} {}", quote(&from.to_string_lossy()), quote(&to.to_string_lossy()));
        let (verb, file, params) = (wide("runas".as_ref()), wide(exe.as_os_str()), wide(params.as_ref()));
        let mut info = ShellExecuteInfo {
            size: std::mem::size_of::<ShellExecuteInfo>() as u32,
            mask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
            hwnd: 0,
            verb: verb.as_ptr(),
            file: file.as_ptr(),
            parameters: params.as_ptr(),
            directory: std::ptr::null(),
            show: SW_HIDE,
            inst_app: 0,
            id_list: std::ptr::null_mut(),
            class: std::ptr::null(),
            hkey_class: 0,
            hot_key: 0,
            icon_or_monitor: 0,
            process: 0,
        };
        // SAFETY: `info` and the strings it points to outlive the call.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            let e = std::io::Error::last_os_error();
            return Err(match e.raw_os_error() {
                Some(ERROR_CANCELLED) => MoveError::Refused,
                _ => MoveError::Failed(e.to_string()),
            });
        }
        if info.process == 0 {
            return Err(MoveError::Failed("the helper did not start".into()));
        }
        let mut code = 1u32;
        // SAFETY: `process` is a live handle ShellExecuteExW gave us; it is
        // closed once, here.
        unsafe {
            WaitForSingleObject(info.process, INFINITE);
            GetExitCodeProcess(info.process, &mut code);
            CloseHandle(info.process);
        }
        match code {
            0 => Ok(()),
            n => Err(MoveError::Failed(format!("the helper ended with code {n}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_like_windows_reads_them() {
        assert_eq!(quote(r"C:\Program Files\a.zip"), r#""C:\Program Files\a.zip""#);
        assert_eq!(quote(r"C:\dir\"), r#""C:\dir\\""#);
        assert_eq!(quote(r#"a"b"#), r#""a\"b""#);
    }

    #[test]
    fn a_writable_folder_passes_and_is_left_clean() {
        let dir = std::env::temp_dir().join(format!("uidm-elevate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(can_write(&dir.join("new").join("deeper")));
        assert!(!dir.join("new").exists());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let (from, to) = (dir.join("a.bin"), dir.join("sub").join("b.bin"));
        fs::write(&from, b"data").unwrap();
        move_file(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(fs::read(&to).unwrap(), b"data");
        let _ = fs::remove_dir_all(&dir);
    }
}
