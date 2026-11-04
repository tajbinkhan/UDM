//! Win32 calls the window needs outside egui: showing and hiding it while egui
//! is idle (a hidden window gets no frames, so viewport commands would wait),
//! reading the clipboard for the tray menu and the clipboard offer, the
//! Windows `Open with` picker, and the display languages.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};

/// The main window's HWND, once eframe has created it.
static WINDOW: AtomicIsize = AtomicIsize::new(0);

const SW_HIDE: i32 = 0;
const SW_SHOW: i32 = 5;
const SW_RESTORE: i32 = 9;
const CF_UNICODETEXT: u32 = 13;

const HWND_TOPMOST: isize = -1;
const HWND_NOTOPMOST: isize = -2;
const SWP_NOSIZE: u32 = 0x1;
const SWP_NOMOVE: u32 = 0x2;
const SWP_SHOWWINDOW: u32 = 0x40;

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn GetCurrentProcessId() -> u32;
}

#[link(name = "user32")]
extern "system" {
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn AttachThreadInput(attach: u32, to: u32, on: i32) -> i32;
    fn BringWindowToTop(hwnd: isize) -> i32;
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn EnumWindows(callback: extern "system" fn(isize, isize) -> i32, param: isize) -> i32;
    fn GetWindowTextW(hwnd: isize, text: *mut u16, max: i32) -> i32;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn OpenClipboard(owner: isize) -> i32;
    fn CloseClipboard() -> i32;
    fn GetClipboardData(format: u32) -> *mut c_void;
    fn GetClipboardSequenceNumber() -> u32;
}

/// `OPENASINFO`.
#[repr(C)]
struct OpenAsInfo {
    file: *const u16,
    class: *const u16,
    flags: u32,
}

const OAIF_ALLOW_REGISTRATION: u32 = 0x1;
const OAIF_REGISTER_EXT: u32 = 0x2;
const OAIF_EXEC: u32 = 0x4;

/// `SHFILEINFOW`.
#[repr(C)]
struct ShFileInfo {
    icon: isize,
    icon_index: i32,
    attributes: u32,
    display_name: [u16; 260],
    type_name: [u16; 80],
}

const SHGFI_USEFILEATTRIBUTES: u32 = 0x10;
const SHGFI_TYPENAME: u32 = 0x400;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;

#[link(name = "shell32")]
extern "system" {
    fn SHOpenWithDialog(parent: isize, info: *const OpenAsInfo) -> i32;
    fn SHGetFileInfoW(path: *const u16, attributes: u32, info: *mut ShFileInfo, size: u32, flags: u32) -> usize;
}

/// What Windows calls files with extension `ext` (no dot), such as "PDF
/// Document", from the file types registered on this computer.
pub fn file_type_name(ext: &str) -> Option<String> {
    if ext.is_empty() || ext.contains(['\\', '/', '\0']) {
        return None;
    }
    let name: Vec<u16> = format!("file.{ext}").encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: an all-zero SHFILEINFOW is valid, and the call writes within
    // the size it is given.
    let mut info: ShFileInfo = unsafe { std::mem::zeroed() };
    let found = unsafe {
        SHGetFileInfoW(
            name.as_ptr(),
            FILE_ATTRIBUTE_NORMAL,
            &mut info,
            std::mem::size_of::<ShFileInfo>() as u32,
            SHGFI_TYPENAME | SHGFI_USEFILEATTRIBUTES,
        )
    };
    let len = info.type_name.iter().position(|c| *c == 0).unwrap_or(info.type_name.len());
    (found != 0 && len > 0).then(|| String::from_utf16_lossy(&info.type_name[..len]))
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalLock(mem: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(mem: *mut c_void) -> i32;
    fn GlobalSize(mem: *mut c_void) -> usize;
    fn GetUserPreferredUILanguages(flags: u32, count: *mut u32, buffer: *mut u16, size: *mut u32) -> i32;
}

/// Languages by name (`de-DE`) rather than by number.
const MUI_LANGUAGE_NAME: u32 = 0x8;

pub fn set_window(hwnd: isize) {
    WINDOW.store(hwnd, Ordering::Relaxed);
}

/// Show the window, restore it if minimized, and bring it to the front.
/// Safe to call from any thread.
pub fn show() {
    let hwnd = WINDOW.load(Ordering::Relaxed);
    if hwnd == 0 {
        return;
    }
    // SAFETY: plain calls on our own window handle.
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
    }
    raise(hwnd);
}

/// Bring `hwnd` in front of the other programs' windows and give it the
/// focus. Windows lets only the program the user is in take the foreground,
/// so a download caught from the browser left the window, and its save-as
/// dialog, behind the browser. Joining the foreground window's input for
/// the call is allowed; should the focus still be refused, a pass through
/// "topmost" at least puts the window on top, where the user sees it.
fn raise(hwnd: isize) {
    // SAFETY: plain calls on window and thread handles Windows gave us; the
    // input is detached again before returning.
    unsafe {
        let front = GetForegroundWindow();
        if front == hwnd {
            return;
        }
        let me = GetCurrentThreadId();
        let theirs = if front != 0 { GetWindowThreadProcessId(front, std::ptr::null_mut()) } else { 0 };
        let joined = theirs != 0 && theirs != me && AttachThreadInput(me, theirs, 1) != 0;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if joined {
            AttachThreadInput(me, theirs, 0);
        }
        if GetForegroundWindow() != hwnd {
            let keep = SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW;
            SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, keep);
            SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, keep);
        }
    }
}

/// Bring this program's visible window titled `title` to the front, as
/// `raise` does: a dialog that opens on its own (a save-as dialog for a
/// caught download, a completion dialog). False when there is no such
/// window yet.
pub fn raise_window(title: &str) -> bool {
    struct Search {
        pid: u32,
        title: Vec<u16>,
        found: isize,
    }
    extern "system" fn visit(hwnd: isize, param: isize) -> i32 {
        // SAFETY: `param` is the `Search` below, alive for the whole
        // EnumWindows call; the text buffer is sized for the call.
        unsafe {
            let search = &mut *(param as *mut Search);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid != search.pid || IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let mut text = [0u16; 512];
            let n = GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32).max(0) as usize;
            if text[..n] == search.title[..] {
                search.found = hwnd;
                return 0;
            }
            1
        }
    }
    let mut search = Search {
        // SAFETY: no arguments, no state.
        pid: unsafe { GetCurrentProcessId() },
        title: title.encode_utf16().collect(),
        found: 0,
    };
    // SAFETY: `visit` reads `search` only during this call.
    unsafe {
        EnumWindows(visit, &mut search as *mut Search as isize);
    }
    if search.found == 0 {
        return false;
    }
    raise(search.found);
    true
}

/// Hide the window; it lives on in the tray.
pub fn hide() {
    let hwnd = WINDOW.load(Ordering::Relaxed);
    if hwnd != 0 {
        // SAFETY: plain call on our own window handle.
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// Show Windows' "Open with" picker for `path` on its own thread (it waits
/// for the user), and open the file with the program chosen.
pub fn open_with(path: &str) {
    let file: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let parent = WINDOW.load(Ordering::Relaxed);
    std::thread::spawn(move || {
        let info = OpenAsInfo {
            file: file.as_ptr(),
            class: std::ptr::null(),
            flags: OAIF_ALLOW_REGISTRATION | OAIF_REGISTER_EXT | OAIF_EXEC,
        };
        // SAFETY: `info` and the path it points to live until the call
        // returns.
        unsafe { SHOpenWithDialog(parent, &info) };
    });
}

/// A number that changes whenever the clipboard does.
pub fn clipboard_sequence() -> u32 {
    // SAFETY: takes no arguments and only reads a counter.
    unsafe { GetClipboardSequenceNumber() }
}

/// The clipboard's text, if it holds any.
pub fn clipboard_text() -> Option<String> {
    // SAFETY: the clipboard is opened and closed around the read; the
    // locked memory is a NUL-terminated UTF-16 string by CF_UNICODETEXT's
    // contract, read only while locked.
    unsafe {
        if OpenClipboard(0) == 0 {
            return None;
        }
        let mut text = None;
        let handle = GetClipboardData(CF_UNICODETEXT);
        if !handle.is_null() {
            let ptr = GlobalLock(handle) as *const u16;
            if !ptr.is_null() {
                // Up to the NUL, but never past the end of the block: the
                // program that put it there may have left the NUL out.
                let max = GlobalSize(handle) / 2;
                let mut len = 0;
                while len < max && *ptr.add(len) != 0 {
                    len += 1;
                }
                text = Some(String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)));
                GlobalUnlock(handle);
            }
        }
        CloseClipboard();
        text
    }
}

/// Windows' display languages, most preferred first, as tags such as
/// `de-DE`.
pub fn display_languages() -> Vec<String> {
    let (mut count, mut size) = (0u32, 0u32);
    // SAFETY: the first call only reports the size; the second fills a
    // buffer of that size with NUL-separated strings, ending in two NULs.
    unsafe {
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, std::ptr::null_mut(), &mut size) == 0 || size == 0 {
            return Vec::new();
        }
        let mut buffer = vec![0u16; size as usize];
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buffer.as_mut_ptr(), &mut size) == 0 {
            return Vec::new();
        }
        buffer
            .split(|&c| c == 0)
            .filter(|tag| !tag.is_empty())
            .map(String::from_utf16_lossy)
            .collect()
    }
}
