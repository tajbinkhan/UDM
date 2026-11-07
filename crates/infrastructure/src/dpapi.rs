//! Keeping saved requests, which hold the browser's cookies, and saved
//! passwords encrypted at rest, so only the same user on the same machine
//! can read them.
//!
//! On Windows this is DPAPI (`CryptProtectData`). On macOS and Linux it is
//! AES-256-CBC with an HMAC-SHA256 tag, keyed by a random key in UIDM's data
//! folder that only the user can read (`secret.key`, mode 600).

pub use imp::{protect, unprotect};

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::io;
    use std::ptr;

    /// Fixed extra entropy, so another program calling DPAPI as the same user
    /// cannot decrypt UIDM's data by accident.
    const ENTROPY: &[u8] = b"uidm-saved-requests-v1";
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            data_in: *const DataBlob,
            description: *const u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *const c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            data_in: *const DataBlob,
            description: *mut *mut u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *const c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(mem: *mut c_void) -> *mut c_void;
    }

    /// Encrypt `data` for the current user.
    pub fn protect(data: &[u8]) -> io::Result<Vec<u8>> {
        call(data, true)
    }

    /// Decrypt data that `protect` produced for the current user.
    pub fn unprotect(data: &[u8]) -> io::Result<Vec<u8>> {
        call(data, false)
    }

    fn blob(data: &[u8]) -> io::Result<DataBlob> {
        Ok(DataBlob {
            cb_data: data
                .len()
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "data too large for DPAPI"))?,
            // DPAPI only reads input blobs.
            pb_data: data.as_ptr() as *mut u8,
        })
    }

    fn call(data: &[u8], encrypt: bool) -> io::Result<Vec<u8>> {
        let input = blob(data)?;
        let entropy = blob(ENTROPY)?;
        let mut output = DataBlob { cb_data: 0, pb_data: ptr::null_mut() };
        // SAFETY: every pointer refers to a live blob for the length of the call;
        // DPAPI allocates `output` with LocalAlloc, freed below after copying.
        let ok = unsafe {
            if encrypt {
                CryptProtectData(
                    &input,
                    ptr::null(),
                    &entropy,
                    ptr::null_mut(),
                    ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    ptr::null_mut(),
                    &entropy,
                    ptr::null_mut(),
                    ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            }
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: on success DPAPI set `output` to `cb_data` readable bytes.
        let bytes = unsafe {
            let out = std::slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec();
            LocalFree(output.pb_data as *mut c_void);
            out
        };
        Ok(bytes)
    }
}

#[cfg(unix)]
mod imp {
    use std::io;

    use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
    use hmac::{KeyInit, Mac};
    use sha2::Sha256;

    const MAGIC: &[u8] = b"UIDM-SEAL1";
    const IV: usize = 16;
    const TAG: usize = 32;

    fn bad(why: &str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, why.to_string())
    }

    /// The user's key, made the first time.
    fn master_key() -> io::Result<[u8; 32]> {
        key_at(&crate::paths::data_dir()?.join("secret.key"))
    }

    /// The key kept at `path`, made there if there is none.
    pub(super) fn key_at(path: &std::path::Path) -> io::Result<[u8; 32]> {
        let path = path.to_path_buf();
        if let Ok(bytes) = std::fs::read(&path) {
            return bytes.try_into().map_err(|_| bad("secret.key is damaged"));
        }
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).map_err(|e| io::Error::other(format!("getrandom: {e}")))?;
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir)?;
        // Two processes (the daemon and the window) or threads can both find
        // no key the first time. Each writes its key whole to a file of its
        // own and links it into place, which fails rather than replaces a
        // key already there; the loser uses the winner's, so nothing is ever
        // sealed with a key that is then overwritten.
        let mut nonce = [0u8; 8];
        getrandom::fill(&mut nonce).map_err(|e| io::Error::other(format!("getrandom: {e}")))?;
        let tmp = dir.join(format!("secret.key.{}.{:016x}", std::process::id(), u64::from_le_bytes(nonce)));
        crate::platform::write_private(&tmp, &key)?;
        let linked = std::fs::hard_link(&tmp, &path);
        let result = match linked {
            Ok(()) => Ok(key),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                std::fs::read(&path).and_then(|bytes| bytes.try_into().map_err(|_| bad("secret.key is damaged")))
            }
            // A file system without hard links: put the key in place as
            // before, unless another caller did meanwhile.
            Err(_) if !path.exists() => std::fs::rename(&tmp, &path).map(|()| key),
            Err(_) => std::fs::read(&path).and_then(|bytes| bytes.try_into().map_err(|_| bad("secret.key is damaged"))),
        };
        let _ = std::fs::remove_file(&tmp);
        result
    }

    /// One key for encrypting and one for the tag, from the user's key.
    fn keys() -> io::Result<([u8; 32], [u8; 32])> {
        let master = master_key()?;
        let derive = |label: &[u8]| -> [u8; 32] {
            let mut mac = <hmac::Hmac<Sha256> as KeyInit>::new_from_slice(&master).expect("any key length");
            mac.update(label);
            mac.finalize().into_bytes().into()
        };
        Ok((derive(b"uidm-encrypt"), derive(b"uidm-authenticate")))
    }

    fn tag(key: &[u8; 32], sealed: &[u8]) -> hmac::Hmac<Sha256> {
        let mut mac = <hmac::Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("any key length");
        mac.update(sealed);
        mac
    }

    /// Encrypt `data` for the current user.
    pub fn protect(data: &[u8]) -> io::Result<Vec<u8>> {
        let (enc, auth) = keys()?;
        let mut iv = [0u8; IV];
        getrandom::fill(&mut iv).map_err(|e| io::Error::other(format!("getrandom: {e}")))?;
        let cipher = cbc::Encryptor::<aes::Aes256>::new(&enc.into(), &iv.into());
        let mut buf = vec![0u8; data.len() + 16];
        buf[..data.len()].copy_from_slice(data);
        let n = cipher.encrypt_padded_mut::<Pkcs7>(&mut buf, data.len()).map_err(|_| bad("could not encrypt"))?.len();
        let mut out = Vec::with_capacity(MAGIC.len() + IV + n + TAG);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&iv);
        out.extend_from_slice(&buf[..n]);
        let t = tag(&auth, &out).finalize().into_bytes();
        out.extend_from_slice(&t);
        Ok(out)
    }

    /// Decrypt data that `protect` produced for the current user.
    pub fn unprotect(data: &[u8]) -> io::Result<Vec<u8>> {
        if data.len() < MAGIC.len() + IV + TAG || !data.starts_with(MAGIC) {
            return Err(bad("not data UIDM sealed"));
        }
        let (enc, auth) = keys()?;
        let (sealed, t) = data.split_at(data.len() - TAG);
        tag(&auth, sealed).verify_slice(t).map_err(|_| bad("sealed data was changed or sealed for another user"))?;
        let iv: [u8; IV] = sealed[MAGIC.len()..MAGIC.len() + IV].try_into().unwrap();
        let mut body = sealed[MAGIC.len() + IV..].to_vec();
        let plain = cbc::Decryptor::<aes::Aes256>::new(&enc.into(), &iv.into())
            .decrypt_padded_mut::<Pkcs7>(&mut body)
            .map_err(|_| bad("could not decrypt"))?;
        Ok(plain.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_hides_the_plaintext() {
        let secret = b"Cookie: session=abc123; theme=dark";
        let sealed = protect(secret).unwrap();
        assert!(!sealed.windows(secret.len()).any(|w| w == secret));
        assert_eq!(unprotect(&sealed).unwrap(), secret);
    }

    #[cfg(unix)]
    #[test]
    fn a_key_made_by_many_at_once_is_one_key() {
        let dir = std::env::temp_dir().join(format!("uidm_key_race_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("secret.key");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let keys: Vec<[u8; 32]> = (0..8)
            .map(|_| {
                let (path, barrier) = (path.clone(), std::sync::Arc::clone(&barrier));
                std::thread::spawn(move || {
                    barrier.wait();
                    imp::key_at(&path).unwrap()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect();
        assert!(keys.iter().all(|k| *k == keys[0]), "every caller got the same key");
        assert_eq!(std::fs::read(&path).unwrap(), keys[0]);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temporary files are left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tampered_data_is_rejected() {
        let mut sealed = protect(b"hello").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xff;
        assert!(unprotect(&sealed).is_err());
    }
}
