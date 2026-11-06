use uidm_domain::ports::PowerControl;
use uidm_domain::queue::{PowerRequest, PowerStep, COUNTDOWN_SECS};

/// Carry out what a finished queue or download asks: close UIDM at once, or
/// count down before sleeping, hibernating or shutting down.
pub struct RunWhenDone<'a, P> {
    power: &'a P,
}

impl<'a, P: PowerControl> RunWhenDone<'a, P> {
    pub fn new(power: &'a P) -> Self {
        RunWhenDone { power }
    }

    /// Returns how `request` is carried out.
    pub fn run(&self, request: &PowerRequest) -> PowerStep {
        let step = request.action.step();
        match step {
            PowerStep::None => {}
            PowerStep::Exit => self.power.exit(),
            PowerStep::Countdown => self.power.start_countdown(request, COUNTDOWN_SECS),
        }
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Power;
    use uidm_domain::queue::WhenDone;

    fn request(action: WhenDone) -> PowerRequest {
        PowerRequest { action, queue: "Night".into(), file: None, force_close: false }
    }

    #[test]
    fn exit_is_at_once_and_power_actions_count_down() {
        let power = Power::default();
        let run = RunWhenDone::new(&power);
        assert_eq!(run.run(&request(WhenDone::Nothing)), PowerStep::None);
        assert_eq!(run.run(&request(WhenDone::Exit)), PowerStep::Exit);
        assert_eq!(power.exits(), 1);
        assert_eq!(run.run(&request(WhenDone::ShutDown)), PowerStep::Countdown);
        assert_eq!(power.started(), [(request(WhenDone::ShutDown), COUNTDOWN_SECS)]);
        assert!(power.powered_off().is_empty(), "the countdown decides when");
    }
}
