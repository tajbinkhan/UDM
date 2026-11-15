use std::path::PathBuf;

use udm_domain::ports::{ReleaseSource, UpdateChecks};
use udm_domain::update::UpdateError;

use super::{CheckForUpdate, ReadyUpdate};

/// Update now: download the newest release's installer (Windows' Setup,
/// the macOS disk image, the AppImage or the Linux `.tar.gz`), check it
/// against the release's signed checksums, save it and install it. The
/// caller then stops the downloads and closes UDM, so Setup can replace the
/// files, or the new copy already put in place can start.
pub struct InstallUpdate<'a, S, N> {
    source: &'a S,
    checks: &'a N,
    current: &'a str,
    ready: Option<&'a ReadyUpdate>,
}

impl<'a, S: ReleaseSource, N: UpdateChecks> InstallUpdate<'a, S, N> {
    /// `current` is this copy's version.
    pub fn new(source: &'a S, checks: &'a N, current: &'a str) -> Self {
        InstallUpdate { source, checks, current, ready: None }
    }

    /// Use the installer `DownloadUpdate` saved, when it is the latest
    /// release's, instead of downloading it again.
    pub fn with_ready(mut self, ready: Option<&'a ReadyUpdate>) -> Self {
        self.ready = ready;
        self
    }

    /// Returns where the Setup program that is now running was saved.
    pub fn run(&self, now: u64) -> Result<PathBuf, UpdateError> {
        let release =
            CheckForUpdate::new(self.source, self.checks, self.current).run(now)?.ok_or(UpdateError::NewestAlready)?;
        let setup = release.setup.ok_or(UpdateError::NoSetup)?;
        let path = match self
            .ready
            .filter(|r| r.release.version == release.version && r.release.setup.as_ref() == Some(&setup))
        {
            Some(ready) => ready.path.clone(),
            None => self.source.fetch_setup(&setup)?,
        };
        self.source.run_setup(&setup, &path)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Checked, Releases};
    use udm_domain::bytes::ByteCount;
    use udm_domain::update::{Release, SetupAsset};

    fn setup() -> SetupAsset {
        SetupAsset {
            name: "UDM-Setup-1.0.0.exe".into(),
            url: "https://x/".into(),
            size: ByteCount::new(10),
            sha256: None,
            installer: Default::default(),
            checksums: None,
        }
    }

    #[test]
    fn a_newer_release_is_downloaded_and_started() {
        let release =
            Release { version: "1.0.0".into(), url: String::new(), setup: Some(setup()), package_manager: false };
        let (source, checks) = (Releases::offering(Ok(release)), Checked::default());
        let path = InstallUpdate::new(&source, &checks, "0.3.0").run(1).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/UDM-Setup-1.0.0.exe"));
        assert_eq!(source.fetched(), [setup()]);
        assert_eq!(source.started(), [path]);
    }

    #[test]
    fn nothing_newer_or_no_setup_installs_nothing() {
        let checks = Checked::default();
        let same = Releases::offering(Ok(Release {
            version: "0.3.0".into(),
            url: String::new(),
            setup: Some(setup()),
            package_manager: false,
        }));
        assert_eq!(InstallUpdate::new(&same, &checks, "0.3.0").run(1), Err(UpdateError::NewestAlready));
        let bare = Releases::offering(Ok(Release {
            version: "2.0.0".into(),
            url: String::new(),
            setup: None,
            package_manager: false,
        }));
        assert_eq!(InstallUpdate::new(&bare, &checks, "0.3.0").run(1), Err(UpdateError::NoSetup));
        assert!(same.fetched().is_empty() && bare.fetched().is_empty());
    }

    #[test]
    fn a_ready_installer_of_the_latest_release_is_not_downloaded_again() {
        let release =
            Release { version: "1.0.0".into(), url: String::new(), setup: Some(setup()), package_manager: false };
        let (source, checks) = (Releases::offering(Ok(release.clone())), Checked::default());
        let ready = ReadyUpdate { release: release.clone(), path: PathBuf::from("/data/UDM-Setup-1.0.0.exe") };
        let path = InstallUpdate::new(&source, &checks, "0.3.0").with_ready(Some(&ready)).run(1).unwrap();
        assert_eq!(path, ready.path);
        assert!(source.fetched().is_empty());
        // One saved for an older release is not used.
        let old = ReadyUpdate { release: Release { version: "0.9.0".into(), ..release }, path: "/old".into() };
        let path = InstallUpdate::new(&source, &checks, "0.3.0").with_ready(Some(&old)).run(1).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/UDM-Setup-1.0.0.exe"));
    }
}
