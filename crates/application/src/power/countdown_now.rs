use uidm_domain::ports::PowerControl;
use uidm_domain::queue::PowerRequest;

/// End the countdown early and carry out its action now.
pub struct CountdownNow<'a, P> {
    power: &'a P,
}

impl<'a, P: PowerControl> CountdownNow<'a, P> {
    pub fn new(power: &'a P) -> Self {
        CountdownNow { power }
    }

    /// Returns what was carried out, or `None` when no countdown ran.
    pub fn run(&self) -> Option<PowerRequest> {
        let request = self.power.take_countdown()?;
        self.power.countdown_canceled();
        self.power.power_off(request.action, request.force_close);
        Some(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Power;
    use uidm_domain::queue::WhenDone;

    #[test]
    fn the_running_countdowns_action_happens_now() {
        let power = Power::default();
        assert_eq!(CountdownNow::new(&power).run(), None);
        let r = PowerRequest { action: WhenDone::Hibernate, queue: String::new(), file: None, force_close: true };
        power.start_countdown(&r, 60);
        assert_eq!(CountdownNow::new(&power).run(), Some(r));
        assert_eq!(power.powered_off(), [(WhenDone::Hibernate, true)]);
        assert_eq!(power.canceled(), 1, "the windows close their countdown");
    }
}
