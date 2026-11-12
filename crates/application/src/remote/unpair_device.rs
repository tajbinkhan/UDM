use udm_domain::id::DeviceId;

use super::{RemoteError, Session};
use crate::daemon::{Command, DaemonClient};

/// Unpair a device at once: `udm-cli unpair`.
pub struct UnpairDevice<'a, 's, C: DaemonClient + ?Sized> {
    session: &'a Session<'s, C>,
}

impl<'a, 's, C: DaemonClient + ?Sized> UnpairDevice<'a, 's, C> {
    pub fn new(session: &'a Session<'s, C>) -> Self {
        UnpairDevice { session }
    }

    /// Unpair device `id`; the daemon refuses one that is not paired.
    pub fn run(&self, id: DeviceId) -> Result<(), RemoteError> {
        self.session.ask(Command::RemoveDevice { id }).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Event;
    use crate::memory::FakeDaemon;
    use crate::remote::ListDevices;
    use std::time::Duration;
    use udm_domain::pairing::{DeviceInfo, DeviceName, RemoteState};

    #[test]
    fn devices_are_listed_and_unpaired() {
        let state = RemoteState {
            devices: vec![DeviceInfo {
                id: DeviceId::new(2),
                name: DeviceName::clean("Pixel"),
                paired_at: 1,
                last_seen: None,
            }],
            ..RemoteState::default()
        };
        let daemon = FakeDaemon::new(move |c| match c {
            Command::RemoteStatus | Command::RemoveDevice { .. } => vec![Event::Remote { state: state.clone() }],
            _ => Vec::new(),
        });
        let session = Session::open(&daemon, Duration::from_secs(5)).unwrap();
        assert_eq!(ListDevices::new(&session).run().unwrap()[0].id, DeviceId::new(2));
        assert_eq!(UnpairDevice::new(&session).run(DeviceId::new(2)), Ok(()));
        assert!(daemon.sent().iter().any(|c| matches!(c, Command::RemoveDevice { id } if id.get() == 2)));
    }
}
