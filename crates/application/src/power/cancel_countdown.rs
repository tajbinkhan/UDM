use udm_domain::ports::PowerControl;

/// Stop the countdown, so its action does not happen.
pub struct CancelCountdown<'a, P> {
    power: &'a P,
}

impl<'a, P: PowerControl> CancelCountdown<'a, P> {
    pub fn new(power: &'a P) -> Self {
        CancelCountdown { power }
    }

    /// Returns whether a countdown was running.
    pub fn run(&self) -> bool {
        if self.power.take_countdown().is_none() {
            return false;
        }
        self.power.countdown_canceled();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Power;
    use udm_domain::queue::{PowerRequest, WhenDone};

    #[test]
    fn a_stopped_countdown_does_nothing() {
        let power = Power::default();
        assert!(!CancelCountdown::new(&power).run());
        let r = PowerRequest { action: WhenDone::Sleep, queue: "Night".into(), file: None, force_close: false };
        power.start_countdown(&r, 60);
        assert!(CancelCountdown::new(&power).run());
        assert!(power.powered_off().is_empty());
        assert_eq!(power.canceled(), 1);
    }
}
