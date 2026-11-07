//! Keeping the download quota's count in `%LOCALAPPDATA%\UIDM\quota.json`
//! (the minutes of the longest window), so it survives a restart. The
//! counting rules are `uidm_domain::quota`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use uidm_domain::quota::Usage;

use crate::settings::write_atomic;

/// The count saved at `path`, or none when it is missing or unreadable.
pub fn load(path: &Path) -> Usage {
    fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Save `usage` at `path` through a temporary file and a rename.
pub fn save(usage: &Usage, path: &Path) -> io::Result<()> {
    let bytes = serde_json::to_vec(usage).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_atomic(path, &bytes)
}

/// `quota.json` in UIDM's data folder (`%LOCALAPPDATA%\UIDM` on Windows).
pub fn quota_path() -> io::Result<PathBuf> {
    Ok(crate::paths::data_dir()?.join("quota.json"))
}

/// Minutes since 1970.
pub fn now_minute() -> u64 {
    crate::state::unix_now() / 60
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survives_a_save_and_load() {
        let mut u = Usage::default();
        u.started(1, 0);
        u.progress(1, 42, 5);
        let path = std::env::temp_dir().join(format!("uidm_quota_{}.json", std::process::id()));
        save(&u, &path).unwrap();
        let mut back = load(&path);
        assert_eq!(back.used(5, 60), 42);
        let _ = fs::remove_file(&path);
        assert_eq!(load(&path), Usage::default(), "a missing file is no usage");
    }
}
