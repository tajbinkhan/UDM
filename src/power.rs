//! What a finished queue can do to the computer: sleep, hibernate or shut
//! down. The daemon calls these only after the 60-second countdown the window
//! shows has run out without being canceled.

use std::io;
use std::process::Command;

use crate::queue::WhenDone;

#[cfg(windows)]
#[link(name = "powrprof")]
extern "system" {
    fn SetSuspendState(hibernate: u8, force: u8, wakeup_events_disabled: u8) -> u8;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn SetThreadExecutionState(flags: u32) -> u32;
}

/// Tell Windows the system is in use (`on`), so its idle timer does not put
/// it to sleep, or hand that back. The request belongs to the calling
/// thread and lasts until the same thread changes it or ends, so one thread
/// should own it. The display may still turn off, and an explicit sleep or
/// shutdown (a queue's "when done" action) still goes ahead. Does nothing
/// on other systems.
pub fn keep_awake(on: bool) {
    #[cfg(windows)]
    {
        const ES_CONTINUOUS: u32 = 0x8000_0000;
        const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
        let flags = if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS };
        // SAFETY: takes flags only; returns 0 on failure, which changes nothing.
        unsafe { SetThreadExecutionState(flags) };
    }
    #[cfg(not(windows))]
    let _ = on;
}

/// Carry out a power action. `Nothing` and `Exit` are not power actions and
/// do nothing here. `force_close` shuts down without waiting for programs
/// that hold it up (such as one asking to save a file); sleep and hibernate
/// never wait for programs.
pub fn perform(action: WhenDone, force_close: bool) -> io::Result<()> {
    #[cfg(windows)]
    if matches!(action, WhenDone::Sleep | WhenDone::Hibernate) {
        let hibernate = (action == WhenDone::Hibernate) as u8;
        // SAFETY: no pointers; returns 0 on failure.
        if unsafe { SetSuspendState(hibernate, 0, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        return Ok(());
    }
    let Some((program, args)) = command(action, force_close) else {
        return Ok(());
    };
    // System32's own copy, not one found on the PATH.
    #[cfg(windows)]
    let path = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
        .join("System32")
        .join(program);
    #[cfg(not(windows))]
    let path = program;
    let status = Command::new(path).args(&args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{program} failed: {status}")))
    }
}

/// The program and arguments that carry out `action`.
///
/// - Windows: shutdown.exe, which holds the privilege dance ExitWindowsEx
///   would need; `/f` closes programs without waiting.
/// - macOS: pmset to sleep (the Mac's hibernate mode decides whether it
///   hibernates), and System Events to shut down the way the Apple menu does.
/// - Linux: systemd; forcing ignores the programs that ask to hold it up.
fn command(action: WhenDone, force_close: bool) -> Option<(&'static str, Vec<&'static str>)> {
    if matches!(action, WhenDone::Nothing | WhenDone::Exit) {
        return None;
    }
    #[cfg(windows)]
    {
        let _ = action;
        let mut args = vec!["/s", "/t", "0"];
        if force_close {
            args.push("/f");
        }
        Some(("shutdown.exe", args))
    }
    #[cfg(target_os = "macos")]
    {
        let _ = force_close;
        match action {
            WhenDone::ShutDown => Some(("osascript", vec!["-e", "tell application \"System Events\" to shut down"])),
            _ => Some(("pmset", vec!["sleepnow"])),
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let verb = match action {
            WhenDone::Sleep => "suspend",
            WhenDone::Hibernate => "hibernate",
            _ => "poweroff",
        };
        let mut args = vec![verb];
        if force_close {
            args.push("--ignore-inhibitors");
        }
        Some(("systemctl", args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forcing_skips_waiting_for_programs() {
        assert_eq!(command(WhenDone::Nothing, true), None);
        assert_eq!(command(WhenDone::Exit, false), None);
        let (_, gentle) = command(WhenDone::ShutDown, false).unwrap();
        let (_, forced) = command(WhenDone::ShutDown, true).unwrap();
        if cfg!(windows) {
            assert_eq!(gentle, ["/s", "/t", "0"]);
            assert_eq!(forced, ["/s", "/t", "0", "/f"]);
        } else if cfg!(target_os = "linux") {
            assert_eq!(gentle, ["poweroff"]);
            assert_eq!(forced, ["poweroff", "--ignore-inhibitors"]);
            assert_eq!(command(WhenDone::Hibernate, false).unwrap(), ("systemctl", vec!["hibernate"]));
        }
    }
}

/// How long before a scheduled start the computer wakes, so the queue starts
/// on time after Windows has resumed.
pub const WAKE_EARLY_SECS: i64 = 30;

/// A Windows wake timer: while set, Windows wakes from sleep or hibernation
/// at its time (when the power plan allows wake timers). Dropping it cancels
/// the wake. On other systems it does nothing.
pub struct WakeTimer {
    #[cfg(windows)]
    handle: isize,
    at: Option<crate::queue::Moment>,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct SystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn CreateWaitableTimerW(attributes: *const u8, manual_reset: i32, name: *const u16) -> isize;
    fn SetWaitableTimer(
        timer: isize,
        due: *const i64,
        period: i32,
        completion: *const u8,
        arg: *const u8,
        resume: i32,
    ) -> i32;
    fn CancelWaitableTimer(timer: isize) -> i32;
    fn CloseHandle(handle: isize) -> i32;
    fn TzSpecificLocalTimeToSystemTime(zone: *const u8, local: *const SystemTime, utc: *mut SystemTime) -> i32;
    fn SystemTimeToFileTime(time: *const SystemTime, file_time: *mut i64) -> i32;
}

/// A local date and time as a UTC FILETIME (100 ns units since 1601).
#[cfg(windows)]
fn local_to_file_time(at: crate::queue::Moment) -> Option<i64> {
    let (y, m, d) = crate::queue::civil_from_days(at.day);
    let local = SystemTime {
        year: u16::try_from(y).ok()?,
        month: m as u16,
        day: d as u16,
        hour: (at.minute / 60) as u16,
        minute: (at.minute % 60) as u16,
        ..SystemTime::default()
    };
    let mut utc = SystemTime::default();
    let mut ft = 0i64;
    // SAFETY: both structs are valid for the calls; a null zone means the
    // current time zone.
    let ok = unsafe {
        TzSpecificLocalTimeToSystemTime(std::ptr::null(), &local, &mut utc) != 0
            && SystemTimeToFileTime(&utc, &mut ft) != 0
    };
    ok.then_some(ft)
}

impl WakeTimer {
    pub fn new() -> Self {
        WakeTimer {
            #[cfg(windows)]
            // SAFETY: no attributes and no name; returns 0 on failure.
            handle: unsafe { CreateWaitableTimerW(std::ptr::null(), 1, std::ptr::null()) },
            at: None,
        }
    }

    /// When the timer wakes the computer, if it is set.
    pub fn at(&self) -> Option<crate::queue::Moment> {
        self.at
    }

    /// Wake the computer `WAKE_EARLY_SECS` before `at` (local time), or
    /// cancel the wake with `None`. Setting the time it already has does
    /// nothing.
    pub fn set(&mut self, at: Option<crate::queue::Moment>) -> io::Result<()> {
        if at == self.at {
            return Ok(());
        }
        #[cfg(windows)]
        {
            if self.handle == 0 {
                return Err(io::Error::other("no wake timer"));
            }
            match at.and_then(local_to_file_time) {
                Some(ft) => {
                    let due = ft - WAKE_EARLY_SECS * 10_000_000;
                    let null = std::ptr::null();
                    // SAFETY: a valid timer handle and due time; no completion routine.
                    if unsafe { SetWaitableTimer(self.handle, &due, 0, null, null, 1) } == 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                // SAFETY: a valid timer handle.
                None => unsafe {
                    CancelWaitableTimer(self.handle);
                },
            }
        }
        self.at = at;
        Ok(())
    }
}

impl Default for WakeTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WakeTimer {
    fn drop(&mut self) {
        #[cfg(windows)]
        if self.handle != 0 {
            // SAFETY: the handle is ours and closed once.
            unsafe {
                CancelWaitableTimer(self.handle);
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(windows)]
#[link(name = "powrprof")]
extern "system" {
    fn PowerGetActiveScheme(root: isize, scheme: *mut *mut Guid) -> u32;
    fn PowerReadACValueIndex(
        root: isize,
        scheme: *const Guid,
        sub: *const Guid,
        setting: *const Guid,
        value: *mut u32,
    ) -> u32;
    fn PowerReadDCValueIndex(
        root: isize,
        scheme: *const Guid,
        sub: *const Guid,
        setting: *const Guid,
        value: *mut u32,
    ) -> u32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(mem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

#[cfg(windows)]
#[repr(C)]
struct Guid(u32, u16, u16, [u8; 8]);

/// Whether the active power plan lets programs wake the computer, on mains
/// power and on battery. `None` when it cannot be read or on other systems.
/// Windows' "Allow wake timers" is 0 to disable, 1 to enable and 2 for
/// important timers only, which also blocks UIDM's.
pub fn wake_timers_allowed() -> Option<(bool, bool)> {
    #[cfg(windows)]
    {
        // GUID_SLEEP_SUBGROUP and GUID_ALLOW_RTC_WAKE (Allow wake timers).
        const SLEEP: Guid = Guid(0x238c9fa8, 0x0aad, 0x41ed, [0x83, 0xf4, 0x97, 0xbe, 0x24, 0x2c, 0x8f, 0x20]);
        const WAKE: Guid = Guid(0xbd3b718a, 0x0680, 0x4d9d, [0x8a, 0xb2, 0xe1, 0xd2, 0xb4, 0xac, 0x80, 0x6d]);
        let mut scheme: *mut Guid = std::ptr::null_mut();
        // SAFETY: Windows allocates the scheme GUID; it is freed with LocalFree below.
        if unsafe { PowerGetActiveScheme(0, &mut scheme) } != 0 || scheme.is_null() {
            return None;
        }
        let (mut ac, mut dc) = (0u32, 0u32);
        // SAFETY: valid GUID pointers and out values.
        let read = unsafe {
            PowerReadACValueIndex(0, scheme, &SLEEP, &WAKE, &mut ac) == 0
                && PowerReadDCValueIndex(0, scheme, &SLEEP, &WAKE, &mut dc) == 0
        };
        // SAFETY: allocated by PowerGetActiveScheme.
        unsafe { LocalFree(scheme.cast()) };
        read.then_some((ac == 1, dc == 1))
    }
    #[cfg(not(windows))]
    None
}

#[cfg(all(test, windows))]
mod wake_tests {
    use super::*;

    #[test]
    fn a_wake_timer_can_be_set_moved_and_cleared() {
        let now = crate::queue::local_now();
        let soon = crate::queue::Moment::from_total(now.total() + 24 * 60);
        let mut timer = WakeTimer::new();
        timer.set(Some(soon)).unwrap();
        assert_eq!(timer.at(), Some(soon));
        let later = crate::queue::Moment::from_total(soon.total() + 60);
        timer.set(Some(later)).unwrap();
        assert_eq!(timer.at(), Some(later));
        timer.set(None).unwrap();
        assert_eq!(timer.at(), None);
    }

    #[test]
    fn local_time_converts_to_a_file_time_a_day_apart() {
        let a = crate::queue::Moment { day: 20_000, minute: 600 };
        let b = crate::queue::Moment { day: 20_001, minute: 600 };
        let (fa, fb) = (local_to_file_time(a).unwrap(), local_to_file_time(b).unwrap());
        // A day, give or take a daylight-saving hour.
        assert!((fb - fa - 864_000_000_000).abs() <= 36_000_000_000);
    }

    #[test]
    fn the_power_plan_wake_setting_can_be_read() {
        assert!(wake_timers_allowed().is_some());
    }
}
