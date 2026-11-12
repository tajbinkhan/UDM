use udm_domain::pairing::{PairingCode, PairingError, Pairings, PendingPairing};
use udm_domain::ports::PairingKeys;

/// Show a new pairing code in the window, in place of any shown before:
/// Options' Pair a phone button.
pub struct StartPairing<'a, K: PairingKeys + ?Sized> {
    keys: &'a K,
}

impl<'a, K: PairingKeys + ?Sized> StartPairing<'a, K> {
    pub fn new(keys: &'a K) -> Self {
        StartPairing { keys }
    }

    /// The code shown from `now` on, made from fresh random bytes.
    pub fn run(&self, pairings: &mut Pairings, now: u64) -> Result<PendingPairing, PairingError> {
        let random = self.keys.random().map_err(|e| PairingError::Store(e.0))?;
        Ok(pairings.start(PairingCode::from_random(&random), now).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::tests::Keys;
    use udm_domain::pairing::PAIRING_LIFETIME;

    #[test]
    fn each_pairing_gets_a_fresh_code_that_runs_out() {
        let keys = Keys::default();
        let mut pairings = Pairings::default();
        let first = StartPairing::new(&keys).run(&mut pairings, 100).unwrap();
        assert_eq!(first.expires_at, 100 + PAIRING_LIFETIME);
        let second = StartPairing::new(&keys).run(&mut pairings, 200).unwrap();
        assert_ne!(first.code, second.code);
        assert_eq!(pairings.pending(200).map(|p| &p.code), Some(&second.code));
    }

    #[test]
    fn no_random_bytes_no_code() {
        let keys = Keys::broken();
        let mut pairings = Pairings::default();
        assert!(matches!(StartPairing::new(&keys).run(&mut pairings, 1), Err(PairingError::Store(_))));
        assert!(pairings.pending(1).is_none());
    }
}
