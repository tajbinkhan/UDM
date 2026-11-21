use std::path::PathBuf;

use udm_domain::ports::{ReleaseSource, UpdateChecks};
use udm_domain::update::{Release, UpdateError};

/// A newer release's installer, downloaded and checked against the
/// release's signed checksums, waiting for the user to install it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyUpdate {
    pub release: Release,
    /// Where it was saved.
    pub path: PathBuf,
}

/// Download a newer release's installer ahead, when the settings ask for
/// it and this copy can install one, so Install now does not wait for it.
/// Nothing is installed: `InstallUpdate` does that when the user asks.
pub struct DownloadUpdate<'a, S, N> {
    source: &'a S,
    checks: &'a N,
}

impl<'a, S: ReleaseSource, N: UpdateChecks> DownloadUpdate<'a, S, N> {
    pub fn new(source: &'a S, checks: &'a N) -> Self {
        DownloadUpdate { source, checks }
    }

    /// Download `release`'s installer. `None` when it is not wanted ahead
    /// or the release has none for this copy (a package manager's copy).
    pub fn run(&self, release: &Release) -> Result<Option<ReadyUpdate>, UpdateError> {
        let Some(setup) = release.setup.as_ref().filter(|_| self.checks.download_ahead()) else {
            return Ok(None);
        };
        // Downloaded in the background: nobody watches its progress.
        let path = self.source.fetch_setup(setup, &|_, _| {})?;
        Ok(Some(ReadyUpdate { release: release.clone(), path }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Checked, Releases};
    use udm_domain::update::SetupAsset;

    fn release(setup: bool) -> Release {
        Release {
            version: "1.0.0".into(),
            url: String::new(),
            setup: setup.then(|| SetupAsset {
                name: "udm-1.0.0-linux-x86_64.tar.gz".into(),
                url: "https://x/".into(),
                installer: Default::default(),
                checksums: None,
            }),
            package_manager: !setup,
        }
    }

    #[test]
    fn a_release_with_an_installer_is_downloaded_but_not_started() {
        let source = Releases::offering(Ok(release(true)));
        let checks = Checked::ahead();
        let ready = DownloadUpdate::new(&source, &checks).run(&release(true)).unwrap().unwrap();
        assert_eq!(ready.path, PathBuf::from("/tmp/udm-1.0.0-linux-x86_64.tar.gz"));
        assert_eq!(source.fetched().len(), 1);
        assert!(source.started().is_empty());
    }

    #[test]
    fn nothing_is_downloaded_when_not_wanted_or_not_installable() {
        let source = Releases::offering(Ok(release(true)));
        let off = Checked::default();
        assert_eq!(DownloadUpdate::new(&source, &off).run(&release(true)), Ok(None));
        let on = Checked::ahead();
        assert_eq!(DownloadUpdate::new(&source, &on).run(&release(false)), Ok(None));
        assert!(source.fetched().is_empty());
    }
}
