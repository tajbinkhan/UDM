//! Where the daemon runs: beside someone's desktop, or on a server nobody
//! sits at (`udm-daemon --server`, the Docker image). The two differ in
//! how long the daemon lives and in whether it may put anything on a
//! screen.

/// Where the daemon runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Seat {
    /// On a computer someone uses: the browser or the window starts the
    /// daemon, which shows windows and notifications and exits once it has
    /// had nothing to do for a while.
    #[default]
    Desktop,
    /// On a server or NAS: started by its service manager or a container,
    /// it runs until it is stopped and has no screen to show anything on.
    Server,
}

impl Seat {
    /// Whether the daemon exits once it has had no clients, downloads or
    /// queues for a while. A desktop daemon does, so one the browser started
    /// does not linger; a server's runs until it is stopped, since nothing
    /// would start it again.
    pub fn exits_when_idle(self) -> bool {
        self == Seat::Desktop
    }

    /// Whether the daemon may start the window, show notifications, play
    /// sounds and register with the browsers on this computer. A server has
    /// no desktop for any of these.
    pub fn has_desktop(self) -> bool {
        self == Seat::Desktop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_daemon_exits_when_idle_and_a_server_runs_on() {
        assert!(Seat::Desktop.exits_when_idle());
        assert!(!Seat::Server.exits_when_idle());
        assert_eq!(Seat::default(), Seat::Desktop, "the daemon runs as it always has unless told otherwise");
    }

    #[test]
    fn only_a_desktop_daemon_shows_anything() {
        assert!(Seat::Desktop.has_desktop());
        assert!(!Seat::Server.has_desktop());
    }
}
