//! The remote-control page's certificate: self-signed, made on this
//! computer the first time remote control is turned on, and made again
//! before it runs out. Its private key is sealed for the user (`dpapi`:
//! DPAPI on Windows, the data folder's key elsewhere) and written readable
//! by the user only; the certificate itself is public.
//!
//! The key is RSA, which SChannel's `Identity::from_pkcs8` needs, and it
//! keeps to what iOS and macOS ask of every TLS server certificate: a
//! 2048-bit key, SHA-256, the names in subjectAltName, serverAuth, and at
//! most 825 days.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use crate::dpapi;

/// How long a new certificate lasts, in days.
const LIFETIME_DAYS: i64 = 800;
/// A certificate this close to running out, in seconds, is made again.
const RENEW_BEFORE: u64 = 30 * 24 * 3600;
const DAY: u64 = 24 * 3600;

/// Why the certificate could not be read or made.
#[derive(Debug, thiserror::Error)]
pub enum CertificateError {
    #[error("could not make the remote-control certificate: {0}")]
    Make(String),
    #[error("could not save the remote-control certificate in {path}: {source}")]
    Save { path: PathBuf, source: io::Error },
}

/// The certificate and its key, both PEM, and what the window shows of it.
#[derive(Clone)]
pub struct Certificate {
    pub pem: String,
    /// PKCS#8. Never logged or sent anywhere.
    key: String,
    /// When it runs out, in seconds since 1970.
    pub expires: u64,
    /// The SHA-256 fingerprint as browsers show it: `AB:CD:...`.
    pub fingerprint: String,
}

impl std::fmt::Debug for Certificate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Certificate({})", self.fingerprint)
    }
}

/// `certificate.json`: the certificate and when it runs out.
#[derive(Serialize, Deserialize)]
struct Saved {
    pem: String,
    expires: u64,
}

impl Certificate {
    /// The TLS identity a server presents.
    pub fn identity(&self) -> Result<native_tls::Identity, native_tls::Error> {
        native_tls::Identity::from_pkcs8(self.pem.as_bytes(), self.key.as_bytes())
    }

    /// The certificate kept in `dir`, or a new one for `names` (host names
    /// and IP addresses) when there is none, it cannot be read, or it runs
    /// out within a month of `now`.
    pub fn load_or_make(dir: &Path, names: &[String], now: u64) -> Result<Certificate, CertificateError> {
        if let Some(kept) = Self::load(dir).filter(|c| c.expires > now + RENEW_BEFORE) {
            return Ok(kept);
        }
        let made = Self::make(names, now)?;
        made.save(dir)?;
        tracing::info!(fingerprint = %made.fingerprint, "made the remote-control certificate");
        Ok(made)
    }

    fn load(dir: &Path) -> Option<Certificate> {
        let saved: Saved = serde_json::from_slice(&fs::read(dir.join("certificate.json")).ok()?).ok()?;
        let key = String::from_utf8(dpapi::unprotect(&fs::read(dir.join("certificate.key")).ok()?).ok()?).ok()?;
        let fingerprint = fingerprint(&saved.pem)?;
        Some(Certificate { pem: saved.pem, key, expires: saved.expires, fingerprint })
    }

    fn save(&self, dir: &Path) -> Result<(), CertificateError> {
        let failed = |path: PathBuf| move |source| CertificateError::Save { path, source };
        fs::create_dir_all(dir).map_err(failed(dir.to_path_buf()))?;
        let key = dir.join("certificate.key");
        let sealed = dpapi::protect(self.key.as_bytes()).map_err(failed(key.clone()))?;
        write_private_atomic(&key, &sealed).map_err(failed(key))?;
        let json = dir.join("certificate.json");
        let saved = serde_json::to_vec_pretty(&Saved { pem: self.pem.clone(), expires: self.expires })
            .map_err(|e| failed(json.clone())(io::Error::other(e)))?;
        write_private_atomic(&json, &saved).map_err(failed(json))
    }

    /// A new self-signed certificate for `names`, valid from yesterday.
    pub fn make(names: &[String], now: u64) -> Result<Certificate, CertificateError> {
        use rsa::pkcs1::EncodeRsaPublicKey;
        use rsa::pkcs8::EncodePrivateKey;
        use rsa::signature::{SignatureEncoding, Signer};

        struct Key {
            signer: rsa::pkcs1v15::SigningKey<rsa::sha2::Sha256>,
            public: Vec<u8>,
        }
        impl rcgen::PublicKeyData for Key {
            fn der_bytes(&self) -> &[u8] {
                &self.public
            }
            fn algorithm(&self) -> &'static rcgen::SignatureAlgorithm {
                &rcgen::PKCS_RSA_SHA256
            }
        }
        impl rcgen::SigningKey for Key {
            fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, rcgen::Error> {
                Ok(self.signer.sign(msg).to_vec())
            }
        }

        let make = |e: &dyn std::fmt::Display| CertificateError::Make(e.to_string());
        let private = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 2048).map_err(|e| make(&e))?;
        let key = private.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).map_err(|e| make(&e))?.to_string();
        let public = private.to_public_key().to_pkcs1_der().map_err(|e| make(&e))?.as_bytes().to_vec();
        let signer = Key { signer: rsa::pkcs1v15::SigningKey::new(private), public };

        let mut params = rcgen::CertificateParams::new(names.to_vec()).map_err(|e| make(&e))?;
        params.distinguished_name.push(rcgen::DnType::CommonName, "Pullbay remote control");
        params.distinguished_name.push(rcgen::DnType::OrganizationName, "Pullbay");
        params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature, rcgen::KeyUsagePurpose::KeyEncipherment];
        let mut serial = [0u8; 16];
        getrandom::fill(&mut serial).map_err(|e| make(&e))?;
        // A positive serial number, as X.509 asks.
        serial[0] &= 0x7f;
        params.serial_number = Some(rcgen::SerialNumber::from(serial.to_vec()));
        let today = (now / DAY) as i64;
        let (y, m, d) = civil(today - 1);
        params.not_before = rcgen::date_time_ymd(y, m, d);
        let (y, m, d) = civil(today + LIFETIME_DAYS);
        params.not_after = rcgen::date_time_ymd(y, m, d);
        let pem = params.self_signed(&signer).map_err(|e| make(&e))?.pem();
        let fingerprint = fingerprint(&pem).ok_or_else(|| make(&"the certificate made is not PEM"))?;
        Ok(Certificate { pem, key, expires: (today + LIFETIME_DAYS) as u64 * DAY, fingerprint })
    }
}

/// The SHA-256 fingerprint of a PEM certificate, as browsers show it.
fn fingerprint(pem: &str) -> Option<String> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    let der = base64::engine::general_purpose::STANDARD.decode(body.trim()).ok()?;
    let digest = sha2::Sha256::digest(&der);
    Some(digest.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":"))
}

/// The year, month and day of day `days` after 1 January 1970 (Howard
/// Hinnant's `civil_from_days`).
pub(crate) fn civil(days: i64) -> (i32, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year as i32, month as u8, day as u8)
}

/// Write `bytes` to `path` readable by the user only, through a temporary
/// file renamed into place, so a reader never sees half a file.
pub(super) fn write_private_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    crate::platform::write_private(&tmp, bytes)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        // Renaming onto a file can fail on Windows; try again without it.
        Err(_) => {
            let _ = fs::remove_file(path);
            fs::rename(&tmp, path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pullbay-cert-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    const NOW: u64 = 1_750_000_000;

    #[test]
    fn days_are_calendar_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(19_723), (2024, 1, 1));
        assert_eq!(civil(19_782), (2024, 2, 29));
        assert_eq!(civil(-1), (1969, 12, 31));
    }

    #[test]
    fn a_certificate_is_made_once_kept_sealed_and_serves_tls() {
        let dir = temp_dir("keep");
        let names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        let made = Certificate::load_or_make(&dir, &names, NOW).unwrap();
        assert_eq!(made.fingerprint.len(), 32 * 3 - 1);
        assert!(made.expires > NOW + 790 * DAY && made.expires <= NOW + 801 * DAY);
        let again = Certificate::load_or_make(&dir, &names, NOW + DAY).unwrap();
        assert_eq!(again.fingerprint, made.fingerprint, "kept, not made again");
        native_tls::TlsAcceptor::new(again.identity().unwrap()).unwrap();

        let sealed = fs::read(dir.join("certificate.key")).unwrap();
        assert!(!String::from_utf8_lossy(&sealed).contains("PRIVATE KEY"), "the key is sealed");
        assert!(!fs::read_to_string(dir.join("certificate.json")).unwrap().contains("PRIVATE KEY"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join("certificate.key")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(!format!("{made:?}").contains("PRIVATE"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_certificate_near_its_end_or_damaged_is_made_again() {
        let dir = temp_dir("renew");
        let names = vec!["localhost".to_string()];
        let made = Certificate::load_or_make(&dir, &names, NOW).unwrap();
        let later = Certificate::load_or_make(&dir, &names, made.expires - DAY).unwrap();
        assert_ne!(later.fingerprint, made.fingerprint);
        fs::write(dir.join("certificate.key"), b"damaged").unwrap();
        let fresh = Certificate::load_or_make(&dir, &names, NOW).unwrap();
        assert_ne!(fresh.fingerprint, later.fingerprint);
        let _ = fs::remove_dir_all(&dir);
    }
}
