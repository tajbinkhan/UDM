//! `ThisSystem`, the `SystemServices` adapter: what the window asks of
//! the operating system, from the modules that know it.

use std::path::Path;

use udm_application::window::{SystemProxy, SystemServices};
use udm_domain::ports::FilesError;
use udm_domain::settings::Sound;
use udm_domain::theme::SystemContrast;

/// The operating system this copy runs on.
pub struct ThisSystem;

impl SystemServices for ThisSystem {
    fn fixed_proxy(&self) -> Option<SystemProxy> {
        let rules = crate::net::system_fixed_proxy()?;
        Some(SystemProxy {
            http: rules.http,
            https: rules.https,
            socks: rules.socks,
            ftp: rules.ftp,
            bypass: rules.bypass,
        })
    }

    fn wake_timers_allowed(&self) -> Option<(bool, bool)> {
        crate::power::wake_timers_allowed()
    }

    fn can_elevate(&self) -> bool {
        crate::elevate::supported()
    }

    fn can_set_creation_time(&self) -> bool {
        crate::engine::CAN_SET_CREATION_TIME
    }

    fn is_portable(&self) -> bool {
        crate::paths::is_portable()
    }

    fn file_type_name(&self, ext: &str) -> Option<String> {
        file_type_name(ext)
    }

    fn play(&self, sound: Sound, file: Option<&Path>) {
        crate::notify::play(sound, file);
    }

    fn save_credential(&self, target: &str, username: &str, password: &str) -> Result<(), FilesError> {
        crate::net::save_credential(target, username, password).map_err(|e| FilesError(e.to_string()))
    }

    fn contrast(&self) -> Option<SystemContrast> {
        contrast()
    }
}

/// What Windows calls files with extension `ext`, such as "PDF Document"
/// or "BIN File", from the file types registered on this computer.
#[cfg(windows)]
fn file_type_name(ext: &str) -> Option<String> {
    /// `SHFILEINFOW`.
    #[repr(C)]
    struct ShFileInfo {
        icon: isize,
        icon_index: i32,
        attributes: u32,
        display_name: [u16; 260],
        type_name: [u16; 80],
    }
    #[link(name = "shell32")]
    extern "system" {
        fn SHGetFileInfoW(path: *const u16, attributes: u32, info: *mut ShFileInfo, size: u32, flags: u32) -> usize;
    }
    const SHGFI_USEFILEATTRIBUTES: u32 = 0x10;
    const SHGFI_TYPENAME: u32 = 0x400;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;

    if ext.is_empty() || ext.contains(['\\', '/', '\0']) {
        return None;
    }
    // With SHGFI_USEFILEATTRIBUTES the file need not exist: Windows answers
    // from the extension alone.
    let name: Vec<u16> = format!("file.{ext}").encode_utf16().chain(std::iter::once(0)).collect();
    let mut info = ShFileInfo { icon: 0, icon_index: 0, attributes: 0, display_name: [0; 260], type_name: [0; 80] };
    // SAFETY: `name` is NUL-terminated and lives until the call returns;
    // the call writes within `info`, whose size it is given.
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

/// Other systems name no file types.
#[cfg(not(windows))]
fn file_type_name(_ext: &str) -> Option<String> {
    None
}

/// Windows' contrast theme colors, while its high-contrast setting
/// (Settings > Accessibility > Contrast themes) is on.
#[cfg(windows)]
fn contrast() -> Option<SystemContrast> {
    use udm_domain::theme::Color;

    #[repr(C)]
    struct HighContrastW {
        cb_size: u32,
        flags: u32,
        default_scheme: *mut u16,
    }
    #[link(name = "user32")]
    extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, value: *mut std::ffi::c_void, win_ini: u32) -> i32;
        fn GetSysColor(index: i32) -> u32;
    }
    const SPI_GETHIGHCONTRAST: u32 = 0x0042;
    const HCF_HIGHCONTRASTON: u32 = 0x0000_0001;
    const COLOR_WINDOW: i32 = 5;
    const COLOR_WINDOWTEXT: i32 = 8;
    const COLOR_HIGHLIGHT: i32 = 13;
    const COLOR_HIGHLIGHTTEXT: i32 = 14;
    const COLOR_BTNFACE: i32 = 15;
    const COLOR_GRAYTEXT: i32 = 17;
    const COLOR_BTNTEXT: i32 = 18;
    const COLOR_HOTLIGHT: i32 = 26;

    let mut hc = HighContrastW {
        cb_size: std::mem::size_of::<HighContrastW>() as u32,
        flags: 0,
        default_scheme: std::ptr::null_mut(),
    };
    // SAFETY: `hc` is a HIGHCONTRASTW with its size set, which the call
    // fills in; its scheme name points into the system's own memory and is
    // not read.
    let read =
        unsafe { SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cb_size, (&mut hc as *mut HighContrastW).cast(), 0) };
    if read == 0 || hc.flags & HCF_HIGHCONTRASTON == 0 {
        return None;
    }
    let color = |index: i32| {
        // SAFETY: no pointers; an index the system knows.
        let bgr = unsafe { GetSysColor(index) };
        Color::rgb((bgr & 0xff) as u8, ((bgr >> 8) & 0xff) as u8, ((bgr >> 16) & 0xff) as u8)
    };
    Some(SystemContrast {
        window: color(COLOR_WINDOW),
        text: color(COLOR_WINDOWTEXT),
        highlight: color(COLOR_HIGHLIGHT),
        highlight_text: color(COLOR_HIGHLIGHTTEXT),
        link: color(COLOR_HOTLIGHT),
        gray_text: color(COLOR_GRAYTEXT),
        button: color(COLOR_BTNFACE),
        button_text: color(COLOR_BTNTEXT),
    })
}

/// macOS and Linux have no contrast colors every program takes; their
/// contrast settings (macOS's Increase contrast, GNOME's High Contrast)
/// change their own controls' look instead, so the High contrast theme
/// gives UDM's own colors there.
#[cfg(not(windows))]
fn contrast() -> Option<SystemContrast> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_type_name_needs_a_plain_extension() {
        assert!(file_type_name("").is_none());
        assert!(file_type_name("a/b").is_none());
        // Windows knows text files, by some name in its language; other
        // systems name no file types.
        assert_eq!(file_type_name("txt").is_some(), cfg!(windows));
    }
}
