use udm_domain::id::DeviceId;
use udm_domain::pairing::{PairingError, Pairings};
use udm_domain::ports::PairedDevices;

/// Unpair a device: Options' Remove button, or the page's Unpair. Its
/// credential stops working at once.
pub struct RemoveDevice<'a, D: PairedDevices + ?Sized> {
    devices: &'a D,
}

impl<'a, D: PairedDevices + ?Sized> RemoveDevice<'a, D> {
    pub fn new(devices: &'a D) -> Self {
        RemoveDevice { devices }
    }

    /// Remove device `id`. When the list cannot be saved, the device is
    /// still refused from now on, and the error says it may be back after
    /// a restart.
    pub fn run(&self, pairings: &mut Pairings, id: DeviceId) -> Result<(), PairingError> {
        pairings.remove(id)?;
        self.devices.save(pairings.devices()).map_err(|e| PairingError::Store(e.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::tests::Book;
    use udm_domain::pairing::{DeviceName, Verifier};

    #[test]
    fn a_removed_device_is_gone_even_when_the_list_cannot_be_saved() {
        let book = Book::broken();
        let mut pairings = Pairings::default();
        let id = pairings.add(DeviceName::default(), Verifier::new([1; 32]), 5).unwrap();
        assert!(matches!(RemoveDevice::new(&book).run(&mut pairings, id), Err(PairingError::Store(_))));
        assert!(pairings.devices().is_empty());
        assert_eq!(RemoveDevice::new(&book).run(&mut pairings, id), Err(PairingError::NoDevice(id)));
    }
}
