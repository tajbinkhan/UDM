use pullbay_domain::ports::{LimitLift, SettingsStore, UserIdle};

/// Lift the speed limit while nobody uses the computer, when the settings
/// ask for that, and apply it again once someone does. Runs with the
/// scheduler.
pub struct LiftLimitWhileAway<'a, S, I, L> {
    settings: &'a S,
    idle: &'a I,
    lift: &'a L,
}

impl<'a, S: SettingsStore, I: UserIdle, L: LimitLift> LiftLimitWhileAway<'a, S, I, L> {
    pub fn new(settings: &'a S, idle: &'a I, lift: &'a L) -> Self {
        LiftLimitWhileAway { settings, idle, lift }
    }

    /// Lift or apply the limit as the time since the last input says.
    /// Returns whether it is lifted now, when that changed.
    pub fn run(&self) -> Option<bool> {
        let s = self.settings.settings();
        // The system is only asked while its answer matters.
        let lifted = s.lift_limit_after().is_some() && s.limit_lifted(self.idle.idle_for());
        if lifted == self.lift.lifted() {
            return None;
        }
        self.lift.set_lifted(lifted);
        Some(lifted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Idle, Lift, Prefs};
    use pullbay_domain::settings::Settings;
    use std::time::Duration;

    fn away_after(minutes: u32) -> Settings {
        Settings {
            speed_limit_enabled: true,
            speed_limit_only_while_active: true,
            speed_limit_idle_minutes: minutes,
            ..Settings::default()
        }
    }

    #[test]
    fn the_limit_is_lifted_after_the_minutes_away_and_applied_at_the_next_input() {
        let prefs = Prefs::with(away_after(5));
        let (idle, lift) = (Idle::default(), Lift::default());
        let check = LiftLimitWhileAway::new(&prefs, &idle, &lift);
        idle.set(Some(Duration::from_secs(4 * 60)));
        assert_eq!(check.run(), None);
        idle.set(Some(Duration::from_secs(5 * 60)));
        assert_eq!(check.run(), Some(true));
        assert_eq!(check.run(), None, "it is already lifted");
        idle.set(Some(Duration::from_secs(1)));
        assert_eq!(check.run(), Some(false));
        assert_eq!(lift.changes(), [true, false]);
    }

    #[test]
    fn the_system_is_not_asked_unless_the_settings_ask_for_it() {
        let prefs = Prefs::with(Settings { speed_limit_only_while_active: false, ..away_after(5) });
        let (idle, lift) = (Idle::default(), Lift::default());
        idle.set(Some(Duration::from_secs(60 * 60)));
        assert_eq!(LiftLimitWhileAway::new(&prefs, &idle, &lift).run(), None);
        assert_eq!(idle.asked(), 0);
    }

    #[test]
    fn a_lifted_limit_comes_back_when_the_setting_goes_or_the_idle_time_is_unknown() {
        let (idle, lift) = (Idle::default(), Lift::default());
        lift.set_lifted(true);
        let prefs = Prefs::with(Settings { speed_limit_enabled: false, ..away_after(5) });
        idle.set(Some(Duration::from_secs(60 * 60)));
        assert_eq!(LiftLimitWhileAway::new(&prefs, &idle, &lift).run(), Some(false));
        lift.set_lifted(true);
        let prefs = Prefs::with(away_after(5));
        idle.set(None);
        assert_eq!(LiftLimitWhileAway::new(&prefs, &idle, &lift).run(), Some(false));
    }
}
