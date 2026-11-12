//! The system clipboard's text, for the window's clipboard monitor
//! (`udm_application::window::Clipboard`). Windows keeps a change counter,
//! so the text is read only after a change; other systems keep none, so the
//! counter there is a hash of the text.

use udm_application::window::Clipboard;

/// The system's clipboard.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn text(&self) -> Option<String> {
        imp::text()
    }

    fn sequence(&self) -> u32 {
        imp::sequence()
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    const CF_UNICODETEXT: u32 = 13;

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalLock(mem: *mut c_void) -> *mut c_void;
        fn GlobalUnlock(mem: *mut c_void) -> i32;
        fn GlobalSize(mem: *mut c_void) -> usize;
    }

    #[link(name = "user32")]
    extern "system" {
        fn OpenClipboard(owner: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut c_void;
        fn GetClipboardSequenceNumber() -> u32;
    }

    pub fn sequence() -> u32 {
        // SAFETY: takes no arguments and only reads a counter.
        unsafe { GetClipboardSequenceNumber() }
    }

    pub fn text() -> Option<String> {
        // SAFETY: the clipboard is opened and closed around the read; the
        // locked memory is a NUL-terminated UTF-16 string by
        // CF_UNICODETEXT's contract, read only while locked.
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
}

#[cfg(not(windows))]
mod imp {
    use std::hash::{Hash, Hasher};
    use std::sync::{Mutex, OnceLock};

    use udm_domain::sync::LockExt;

    /// The clipboard, opened once; the system keeps one per process.
    fn clipboard() -> &'static Mutex<Option<arboard::Clipboard>> {
        static CLIPBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
        CLIPBOARD.get_or_init(|| Mutex::new(arboard::Clipboard::new().ok()))
    }

    pub fn text() -> Option<String> {
        clipboard().lock_or_recover().as_mut()?.get_text().ok()
    }

    pub fn sequence() -> u32 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text().hash(&mut hasher);
        hasher.finish() as u32
    }
}
