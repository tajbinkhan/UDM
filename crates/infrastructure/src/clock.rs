//! The computer's local clock. The queue rules in `uidm_domain::queue` take
//! the time as a parameter; this is where it comes from.

use uidm_domain::queue::{days_from_civil, Moment};

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
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
    fn GetLocalTime(time: *mut SystemTime);
}

/// The computer's local date and time now.
#[cfg(windows)]
pub fn local_now() -> Moment {
    let mut t = SystemTime::default();
    // SAFETY: fills the struct it is given.
    unsafe { GetLocalTime(&mut t) };
    Moment {
        day: days_from_civil(t.year as i64, t.month as u32, t.day as u32),
        minute: t.hour as u32 * 60 + t.minute as u32,
    }
}

/// The computer's local date and time now, in its time zone.
#[cfg(unix)]
pub fn local_now() -> Moment {
    // SAFETY: `time` with a null pointer only returns the time; `localtime_r`
    // fills the struct it is given and keeps no pointer to it.
    let t = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut t: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut t);
        t
    };
    Moment {
        day: days_from_civil(t.tm_year as i64 + 1900, t.tm_mon as u32 + 1, t.tm_mday as u32),
        minute: t.tm_hour as u32 * 60 + t.tm_min as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_clock_is_sane() {
        let now = local_now();
        assert!(now.minute < 1440);
        // After 2024-01-01 and before 2200.
        assert!(now.day > days_from_civil(2024, 1, 1) && now.day < days_from_civil(2200, 1, 1));
    }
}
