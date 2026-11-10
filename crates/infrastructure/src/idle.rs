//! How long the computer has gone without keyboard or mouse input, for the
//! speed limit that applies only while someone uses it.
//!
//! Windows has `GetLastInputInfo`, macOS `CGEventSourceSecondsSinceLastEventType`.
//! Linux has no one call: GNOME's Mutter answers on `org.gnome.Mutter.IdleMonitor`
//! and KDE's on `org.freedesktop.ScreenSaver.GetSessionIdleTime`, both in
//! milliseconds on the session bus, which `dbus-send` reaches (as
//! `platform::reveal` does). Where none answers the time is unknown, and the
//! speed limit stays.

use std::time::Duration;

use pullbay_domain::ports::UserIdle;

/// The time since the last input, as the operating system tells it.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemIdle;

impl UserIdle for SystemIdle {
    fn idle_for(&self) -> Option<Duration> {
        idle_for()
    }
}

#[cfg(windows)]
fn idle_for() -> Option<Duration> {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: `info` is a LASTINPUTINFO with its size set, as the call asks;
    // GetTickCount reads a counter.
    let (read, now) = unsafe { (GetLastInputInfo(&mut info).as_bool(), GetTickCount()) };
    // Both count milliseconds since the system started in 32 bits, which
    // wrap after 49.7 days; the difference still holds across the wrap.
    read.then(|| Duration::from_millis(u64::from(now.wrapping_sub(info.dwTime))))
}

#[cfg(target_os = "macos")]
fn idle_for() -> Option<Duration> {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    }
    /// `kCGEventSourceStateCombinedSessionState`: every source in the session.
    const COMBINED_SESSION_STATE: i32 = 0;
    /// `kCGAnyInputEventType`.
    const ANY_INPUT_EVENT: u32 = u32::MAX;
    // SAFETY: a CoreGraphics function that takes two plain values and reads
    // the session's event state.
    let secs = unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT) };
    (secs.is_finite() && secs >= 0.0).then(|| Duration::from_secs_f64(secs))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn idle_for() -> Option<Duration> {
    use std::process::{Command, Stdio};

    /// Each desktop's service, object, method, and the reply's type.
    const ASK: [[&str; 3]; 2] = [
        [
            "org.gnome.Mutter.IdleMonitor",
            "/org/gnome/Mutter/IdleMonitor/Core",
            "org.gnome.Mutter.IdleMonitor.GetIdletime",
        ],
        [
            "org.freedesktop.ScreenSaver",
            "/org/freedesktop/ScreenSaver",
            "org.freedesktop.ScreenSaver.GetSessionIdleTime",
        ],
    ];
    ASK.iter().find_map(|[dest, path, method]| {
        let out = Command::new("dbus-send")
            .args(["--session", "--print-reply", "--reply-timeout=1000"])
            .arg(format!("--dest={dest}"))
            .args([path, method])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status.success().then(|| reply_millis(&String::from_utf8_lossy(&out.stdout))).flatten()
    })
}

/// The milliseconds in a `dbus-send --print-reply` answer such as
/// `method return ...\n   uint64 1234`.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
fn reply_millis(reply: &str) -> Option<Duration> {
    let mut words = reply.lines().skip(1).flat_map(str::split_whitespace);
    match words.next()? {
        "uint64" | "uint32" | "int64" | "int32" => words.next()?.parse::<u64>().ok().map(Duration::from_millis),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_milliseconds_are_read_from_a_dbus_reply() {
        let gnome =
            "method return time=1.2 sender=:1.9 -> destination=:1.80 serial=7 reply_serial=2\n   uint64 61500\n";
        assert_eq!(reply_millis(gnome), Some(Duration::from_millis(61_500)));
        let kde = "method return time=1.2 sender=:1.9 -> destination=:1.80 serial=7 reply_serial=2\n   uint32 1200\n";
        assert_eq!(reply_millis(kde), Some(Duration::from_millis(1200)));
        assert_eq!(reply_millis("method return\n   string \"no\"\n"), None);
        assert_eq!(reply_millis("method return\n   int32 -4\n"), None, "a negative time is not a time");
        assert_eq!(reply_millis(""), None);
    }

    #[test]
    fn the_system_answers_without_failing() {
        // Whatever the machine says, asking must not panic or hang.
        let _ = SystemIdle.idle_for();
    }
}
