//! `ThisSystem`, the `SystemServices` adapter: what the window asks of
//! the operating system, from the modules that know it.

use std::path::Path;

use pullbay_application::window::{SystemFonts, SystemProxy, SystemServices};
use pullbay_domain::ports::FilesError;
use pullbay_domain::settings::Sound;
use pullbay_domain::theme::SystemContrast;

/// The operating system this copy runs on.
pub struct ThisSystem;

impl SystemServices for ThisSystem {
    fn fonts(&self) -> SystemFonts {
        // Segoe UI and Consolas, from the Windows font folder, so nothing
        // is bundled. Elsewhere egui's own fonts are used.
        let read = |file: &str| crate::paths::fonts_dir().and_then(|d| std::fs::read(d.join(file)).ok());
        SystemFonts { text: read("segoeui.ttf"), code: read("consola.ttf") }
    }

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

    fn play(&self, sound: Sound, file: Option<&Path>) {
        crate::notify::play(sound, file);
    }

    fn save_credential(&self, target: &str, username: &str, password: &str) -> Result<(), FilesError> {
        crate::net::save_credential(target, username, password).map_err(|e| FilesError(e.to_string()))
    }

    fn start_window_in_tray(&self) -> Result<(), FilesError> {
        let mut cmd = crate::platform::program_command("pullbay-gui").map_err(|e| FilesError(e.to_string()))?;
        cmd.arg(crate::cli::IN_TRAY)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        // Its own process group, as the daemon has: this copy is about to
        // exit, and the new one outlives it.
        crate::platform::spawn_reaped(crate::platform::detach(&mut cmd)).map_err(|e| FilesError(e.to_string()))
    }

    fn contrast(&self) -> Option<SystemContrast> {
        contrast()
    }
}

/// Windows' contrast theme colors, while its high-contrast setting
/// (Settings > Accessibility > Contrast themes) is on.
#[cfg(windows)]
fn contrast() -> Option<SystemContrast> {
    use pullbay_domain::theme::Color;

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
/// gives Pullbay's own colors there.
#[cfg(not(windows))]
fn contrast() -> Option<SystemContrast> {
    None
}
