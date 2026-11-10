use pullbay_domain::pairing::RemoteState;

use super::{RemoteError, Session};
use crate::daemon::{Command, DaemonClient};

/// Show a pairing code for a phone, as Options' Pair a phone does:
/// `pullbay-cli pair`, for a daemon with no window, such as a server's.
pub struct PairPhone<'a, 's, C: DaemonClient + ?Sized> {
    session: &'a Session<'s, C>,
}

impl<'a, 's, C: DaemonClient + ?Sized> PairPhone<'a, 's, C> {
    pub fn new(session: &'a Session<'s, C>) -> Self {
        PairPhone { session }
    }

    /// The code shown, with the page's address and fingerprint. The daemon
    /// refuses while remote control is not listening.
    pub fn run(&self) -> Result<RemoteState, RemoteError> {
        let state = self.session.remote(Command::StartPairing)?;
        if state.pairing.is_none() {
            return Err(RemoteError::NoAnswer(self.session.wait().as_secs()));
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::service::ServiceError;
    use crate::daemon::{Event, Failure};
    use crate::memory::FakeDaemon;
    use pullbay_domain::id::DeviceId;
    use pullbay_domain::pairing::DeviceInfo;
    use pullbay_domain::pairing::{DeviceName, PairingCode, PairingError, PairingOffer};
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(5);

    fn state(pairing: bool) -> RemoteState {
        let code = PairingCode::from_random(&[3; 32]);
        RemoteState {
            devices: vec![DeviceInfo {
                id: DeviceId::new(2),
                name: DeviceName::clean("Pixel"),
                paired_at: 1,
                last_seen: None,
            }],
            pairing: pairing.then(|| PairingOffer { link: "https://h:1/#pair=x".into(), code, expires_at: 9 }),
            ..RemoteState::default()
        }
    }

    #[test]
    fn pairing_shows_the_code_the_daemon_made() {
        let daemon = FakeDaemon::new(|c| match c {
            Command::StartPairing => vec![Event::Remote { state: state(true) }],
            _ => Vec::new(),
        });
        let session = Session::open(&daemon, WAIT).unwrap();
        let shown = PairPhone::new(&session).run().unwrap();
        assert_eq!(shown.pairing.map(|p| p.expires_at), Some(9));
    }

    #[test]
    fn pairing_is_refused_while_remote_control_is_off() {
        let off = Failure::Service(ServiceError::Pairing(PairingError::NotListening));
        let refusal = off.clone();
        let daemon = FakeDaemon::new(move |c| match c {
            Command::StartPairing => vec![Event::Error { error: refusal.clone() }],
            _ => Vec::new(),
        });
        let session = Session::open(&daemon, WAIT).unwrap();
        assert_eq!(PairPhone::new(&session).run(), Err(RemoteError::Refused(off)));
    }
}
