use pullbay_domain::pairing::DeviceInfo;

use super::{RemoteError, Session};
use crate::daemon::{Command, DaemonClient};

/// The paired devices: `pullbay-cli devices`.
pub struct ListDevices<'a, 's, C: DaemonClient + ?Sized> {
    session: &'a Session<'s, C>,
}

impl<'a, 's, C: DaemonClient + ?Sized> ListDevices<'a, 's, C> {
    pub fn new(session: &'a Session<'s, C>) -> Self {
        ListDevices { session }
    }

    pub fn run(&self) -> Result<Vec<DeviceInfo>, RemoteError> {
        Ok(self.session.remote(Command::RemoteStatus)?.devices)
    }
}
