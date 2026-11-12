use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard};

use tracing::{debug, info, warn};
use udm_domain::id::DeviceId;
use udm_domain::pairing::{Credential, DeviceInfo, PairingError, Pairings, PendingPairing};
use udm_domain::ports::{PairedDevices, PairingKeys, Timestamps};
use udm_domain::sync::LockExt;

use super::{CheckDevice, PairDevice, RemoveDevice, StartPairing};

/// Pairing as the daemon runs it: the paired devices and the code shown,
/// shared by the window's commands and the remote-control page, each
/// change made through its use case.
pub struct PairingDesk {
    pairings: Mutex<Pairings>,
    devices: Arc<dyn PairedDevices>,
    keys: Arc<dyn PairingKeys>,
    clock: Arc<dyn Timestamps>,
}

impl PairingDesk {
    /// The devices `devices` kept. A list that cannot be read pairs
    /// nobody; the next pairing replaces it.
    pub fn open(devices: Arc<dyn PairedDevices>, keys: Arc<dyn PairingKeys>, clock: Arc<dyn Timestamps>) -> Self {
        let kept = devices.load().unwrap_or_else(|e| {
            warn!(error = %e, "could not read the paired devices; none are paired");
            Vec::new()
        });
        PairingDesk { pairings: Mutex::new(Pairings::new(kept)), devices, keys, clock }
    }

    /// The pairing state, locked. Every change to it is one call on
    /// `Pairings` that leaves it whole, so a lock a panic poisoned still
    /// holds a usable value.
    fn pairings(&self) -> MutexGuard<'_, Pairings> {
        self.pairings.lock_or_recover()
    }

    /// Show a new pairing code.
    pub fn start(&self) -> Result<PendingPairing, PairingError> {
        let now = self.clock.unix_now();
        let shown = StartPairing::new(&*self.keys).run(&mut self.pairings(), now)?;
        info!(expires_at = shown.expires_at, "pairing code shown");
        Ok(shown)
    }

    /// Withdraw the code shown.
    pub fn cancel(&self) {
        self.pairings().cancel();
    }

    /// The code shown, unless it ran out.
    pub fn pending(&self) -> Option<PendingPairing> {
        self.pairings().pending(self.clock.unix_now()).cloned()
    }

    /// The paired devices, as the window lists them.
    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.pairings().devices().iter().map(|d| d.info()).collect()
    }

    /// Unpair device `id`.
    pub fn remove(&self, id: DeviceId) -> Result<(), PairingError> {
        let removed = RemoveDevice::new(&*self.devices).run(&mut self.pairings(), id);
        if !matches!(removed, Err(PairingError::NoDevice(_))) {
            info!(device_id = id.get(), "device unpaired");
        }
        removed
    }

    /// Pair the device at `peer` that sent `code`.
    pub fn pair(&self, code: &str, name: &str, peer: IpAddr) -> Result<Credential, PairingError> {
        let now = self.clock.unix_now();
        let paired = PairDevice::new(&*self.keys, &*self.devices).run(&mut self.pairings(), code, name, peer, now);
        match &paired {
            Ok(c) => info!(device_id = c.device.get(), peer = %peer, "device paired"),
            // Each try past the limit is not worth a line of its own.
            Err(PairingError::TooManyTries { .. }) => debug!(peer = %peer, "pairing refused: too many tries"),
            Err(e) => warn!(peer = %peer, error = %e, "pairing refused"),
        }
        paired
    }

    /// The paired device `credential` names.
    pub fn check(&self, credential: &str, peer: IpAddr) -> Result<DeviceId, PairingError> {
        let now = self.clock.unix_now();
        let checked = CheckDevice::new(&*self.keys, &*self.devices).run(&mut self.pairings(), credential, peer, now);
        match &checked {
            Err(PairingError::TooManyTries { .. }) => {
                debug!(peer = %peer, "remote-control request refused: too many tries")
            }
            Err(e) => warn!(peer = %peer, error = %e, "remote-control request refused"),
            Ok(_) => {}
        }
        checked
    }
}
