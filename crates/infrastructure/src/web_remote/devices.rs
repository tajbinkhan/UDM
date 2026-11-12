//! The paired devices' file and the random source pairing draws on.
//!
//! `devices.bin` in the remote folder holds each device's id, name, times
//! and the SHA-256 digest of its secret (never the secret), as JSON sealed
//! for the user like the saved requests (`dpapi`), written readable by the
//! user only and replaced whole, so a reader never sees half of it.

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use udm_domain::pairing::{Device, DeviceSecret, Verifier, RANDOM_BYTES};
use udm_domain::ports::{DevicesError, PairedDevices, PairingKeys};

use super::certificate::write_private_atomic;
use crate::dpapi;

/// The file's format; a newer one is refused rather than misread.
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Saved {
    version: u32,
    devices: Vec<Device>,
}

/// The `PairedDevices` adapter: `devices.bin` at `path`. Without a path
/// (no data folder) none are kept, and none can pair.
pub struct DeviceFile {
    path: Option<PathBuf>,
}

impl DeviceFile {
    pub fn at(path: PathBuf) -> Self {
        DeviceFile { path: Some(path) }
    }

    /// `devices.bin` in the remote folder `dir`, if there is one.
    pub fn in_dir(dir: Option<&std::path::Path>) -> Self {
        DeviceFile { path: dir.map(|d| d.join("devices.bin")) }
    }
}

fn failed(what: &str, e: impl std::fmt::Display) -> DevicesError {
    DevicesError(format!("could not {what} the paired devices: {e}"))
}

impl PairedDevices for DeviceFile {
    fn load(&self) -> Result<Vec<Device>, DevicesError> {
        let Some(path) = &self.path else {
            return Ok(Vec::new());
        };
        let sealed = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(failed("read", e)),
        };
        let plain = dpapi::unprotect(&sealed).map_err(|e| failed("unseal", e))?;
        // serde's message could quote a verifier; say only where it failed.
        let saved: Saved = serde_json::from_slice(&plain)
            .map_err(|e| failed("read", format!("line {} column {}", e.line(), e.column())))?;
        if saved.version > VERSION {
            return Err(failed("read", format!("the file is from a newer UDM (format {})", saved.version)));
        }
        Ok(saved.devices)
    }

    fn save(&self, devices: &[Device]) -> Result<(), DevicesError> {
        let plain = serde_json::to_vec(&Saved { version: VERSION, devices: devices.to_vec() })
            .map_err(|e| failed("save", e))?;
        let path = self.path.as_ref().ok_or_else(|| failed("save", "there is no data folder"))?;
        let sealed = dpapi::protect(&plain).map_err(|e| failed("seal", e))?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| failed("save", e))?;
        }
        write_private_atomic(path, &sealed).map_err(|e| failed("save", e))
    }
}

/// The `PairingKeys` adapter: the operating system's random source and
/// SHA-256.
pub struct OsKeys;

impl PairingKeys for OsKeys {
    fn random(&self) -> Result<[u8; RANDOM_BYTES], DevicesError> {
        let mut bytes = [0u8; RANDOM_BYTES];
        getrandom::fill(&mut bytes).map_err(|e| DevicesError(format!("no random bytes: {e}")))?;
        Ok(bytes)
    }

    fn verifier(&self, secret: &DeviceSecret) -> Verifier {
        let mut digest = [0u8; RANDOM_BYTES];
        digest.copy_from_slice(&sha2::Sha256::digest(secret.bytes()));
        Verifier::new(digest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use udm_domain::id::DeviceId;
    use udm_domain::pairing::DeviceName;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("udm-devices-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("remote").join("devices.bin")
    }

    #[test]
    fn devices_are_kept_sealed_and_read_back() {
        let path = temp_path("keep");
        let file = DeviceFile::at(path.clone());
        assert_eq!(file.load(), Ok(Vec::new()), "nothing saved yet");
        let secret = DeviceSecret::from_random(OsKeys.random().unwrap());
        let device = Device {
            id: DeviceId::new(1),
            name: DeviceName::clean("Pixel"),
            paired_at: 5,
            last_seen: Some(6),
            verifier: OsKeys.verifier(&secret),
        };
        file.save(std::slice::from_ref(&device)).unwrap();
        assert_eq!(file.load(), Ok(vec![device.clone()]));
        let raw = fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(!text.contains("Pixel") && !text.contains(&String::from(device.verifier.clone())), "sealed");
        assert!(!text.contains(&secret.to_hex()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        file.save(&[]).unwrap();
        assert_eq!(file.load(), Ok(Vec::new()));
    }

    #[test]
    fn a_damaged_file_is_an_error_not_an_empty_list() {
        let path = temp_path("damaged");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"not sealed").unwrap();
        assert!(DeviceFile::at(path).load().is_err());
    }

    #[test]
    fn secrets_are_random_and_verifiers_are_their_sha256() {
        let (a, b) = (OsKeys.random().unwrap(), OsKeys.random().unwrap());
        assert_ne!(a, b);
        let zero = DeviceSecret::from_random([0; RANDOM_BYTES]);
        // SHA-256 of 32 zero bytes.
        let expected = "66687aadf862bd776c8fc18b8e9f8e20089714856ee233b3902a591d0d5f2925";
        assert_eq!(String::from(OsKeys.verifier(&zero)), expected);
    }
}
