use std::net::IpAddr;

use pullbay_domain::id::DeviceId;
use pullbay_domain::pairing::{PairingError, Pairings};
use pullbay_domain::ports::{PairedDevices, PairingKeys};
use tracing::warn;

/// Recognize a paired device by the credential it sent: every request of
/// the remote-control page but pairing.
pub struct CheckDevice<'a, K: PairingKeys + ?Sized, D: PairedDevices + ?Sized> {
    keys: &'a K,
    devices: &'a D,
}

impl<'a, K: PairingKeys + ?Sized, D: PairedDevices + ?Sized> CheckDevice<'a, K, D> {
    pub fn new(keys: &'a K, devices: &'a D) -> Self {
        CheckDevice { keys, devices }
    }

    /// The device `credential` names, when it is paired and its secret is
    /// right. Its last-seen time is saved now and then.
    pub fn run(
        &self,
        pairings: &mut Pairings,
        credential: &str,
        peer: IpAddr,
        now: u64,
    ) -> Result<DeviceId, PairingError> {
        let (id, save) = pairings.check(credential, peer, now, |secret| self.keys.verifier(secret))?;
        if save {
            if let Err(e) = self.devices.save(pairings.devices()) {
                warn!(error = %e, "could not save when a paired device was last seen");
            }
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::tests::{peer, Book, Keys};
    use crate::pairing::{PairDevice, RemoveDevice, StartPairing};

    #[test]
    fn only_a_paired_devices_own_credential_works() {
        let (keys, book) = (Keys::default(), Book::default());
        let mut pairings = Pairings::default();
        let shown = StartPairing::new(&keys).run(&mut pairings, 10).unwrap();
        let credential =
            PairDevice::new(&keys, &book).run(&mut pairings, shown.code.as_str(), "", peer(5), 11).unwrap();
        let check = CheckDevice::new(&keys, &book);
        assert_eq!(check.run(&mut pairings, &credential.to_text(), peer(5), 12), Ok(credential.device));
        assert_eq!(check.run(&mut pairings, "", peer(5), 12), Err(PairingError::NotPaired));
        assert_eq!(
            check.run(&mut pairings, &format!("1.{}", "0".repeat(64)), peer(5), 12),
            Err(PairingError::NotPaired)
        );

        RemoveDevice::new(&book).run(&mut pairings, credential.device).unwrap();
        assert_eq!(check.run(&mut pairings, &credential.to_text(), peer(5), 13), Err(PairingError::NotPaired));
        assert!(book.saved().is_empty(), "the removal is saved");
    }
}
