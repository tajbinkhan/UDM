use std::net::IpAddr;

use pullbay_domain::pairing::{Credential, DeviceName, DeviceSecret, PairingError, Pairings};
use pullbay_domain::ports::{PairedDevices, PairingKeys};

/// Exchange the pairing code a phone read from the QR code for a
/// credential of its own, once: the remote-control page's pairing.
pub struct PairDevice<'a, K: PairingKeys + ?Sized, D: PairedDevices + ?Sized> {
    keys: &'a K,
    devices: &'a D,
}

impl<'a, K: PairingKeys + ?Sized, D: PairedDevices + ?Sized> PairDevice<'a, K, D> {
    pub fn new(keys: &'a K, devices: &'a D) -> Self {
        PairDevice { keys, devices }
    }

    /// Pair the device at `peer` that sent `code`, naming it `name`. The
    /// code is used up whether or not the device list can be saved; one
    /// that cannot is not paired.
    pub fn run(
        &self,
        pairings: &mut Pairings,
        code: &str,
        name: &str,
        peer: IpAddr,
        now: u64,
    ) -> Result<Credential, PairingError> {
        pairings.has_room()?;
        pairings.redeem(code, peer, now)?;
        let secret = DeviceSecret::from_random(self.keys.random().map_err(|e| PairingError::Store(e.0))?);
        let device = pairings.add(DeviceName::clean(name), self.keys.verifier(&secret), now)?;
        if let Err(e) = self.devices.save(pairings.devices()) {
            let _ = pairings.remove(device);
            return Err(PairingError::Store(e.0));
        }
        Ok(Credential { device, secret })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::tests::{peer, Book, Keys};
    use crate::pairing::StartPairing;
    use pullbay_domain::id::DeviceId;
    use pullbay_domain::pairing::MAX_DEVICES;

    #[test]
    fn the_code_shown_pairs_one_device_and_its_verifier_is_kept() {
        let (keys, book) = (Keys::default(), Book::default());
        let mut pairings = Pairings::default();
        let shown = StartPairing::new(&keys).run(&mut pairings, 10).unwrap();
        let pair = PairDevice::new(&keys, &book);
        let credential = pair.run(&mut pairings, &shown.code.grouped(), " Pixel\n", peer(5), 11).unwrap();
        assert_eq!(credential.device, DeviceId::new(1));
        let saved = book.saved();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].name.as_str(), "Pixel");
        assert!(saved[0].verifier.matches(&keys.verifier(&credential.secret)), "only the verifier is kept");
        assert_eq!(
            pair.run(&mut pairings, shown.code.as_str(), "again", peer(6), 12),
            Err(PairingError::WrongCode),
            "the code works once"
        );
    }

    #[test]
    fn a_device_that_cannot_be_saved_is_not_paired() {
        let keys = Keys::default();
        let book = Book::broken();
        let mut pairings = Pairings::default();
        let shown = StartPairing::new(&keys).run(&mut pairings, 10).unwrap();
        let result = PairDevice::new(&keys, &book).run(&mut pairings, shown.code.as_str(), "x", peer(5), 11);
        assert!(matches!(result, Err(PairingError::Store(_))));
        assert!(pairings.devices().is_empty());
    }

    #[test]
    fn a_full_list_keeps_the_code_for_after_a_removal() {
        let (keys, book) = (Keys::default(), Book::default());
        let mut pairings = Pairings::default();
        for _ in 0..MAX_DEVICES {
            let shown = StartPairing::new(&keys).run(&mut pairings, 10).unwrap();
            PairDevice::new(&keys, &book).run(&mut pairings, shown.code.as_str(), "x", peer(5), 11).unwrap();
        }
        let shown = StartPairing::new(&keys).run(&mut pairings, 10).unwrap();
        let pair = PairDevice::new(&keys, &book);
        assert_eq!(pair.run(&mut pairings, shown.code.as_str(), "x", peer(5), 11), Err(PairingError::TooManyDevices));
        pairings.remove(DeviceId::new(1)).unwrap();
        assert!(pair.run(&mut pairings, shown.code.as_str(), "x", peer(5), 12).is_ok());
    }
}
