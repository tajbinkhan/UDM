//! Checking GitHub for a newer Pullbay release, and the Setup program attached
//! to it. The window shows a notice with a link to the release page; Update
//! now has the daemon download that Setup, check it against what GitHub
//! says about it, and run it. `GitHub` is the adapter for `ReleaseSource`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use pullbay_domain::download::Download;
use pullbay_domain::ports::ReleaseSource;
use pullbay_domain::update::UpdateError;
use tracing::info;

use crate::connect::{config_for, ProxyRetries};
use crate::engine;
use crate::live_settings::LiveSettings;

use pullbay_domain::update::{Release, SetupAsset};

/// The releases of Pullbay's repository, newest first by GitHub's reckoning.
pub const LATEST_RELEASE: &str = "https://api.github.com/repos/tajbinkhan/pullbay/releases/latest";

/// Where release files are served from; a Setup anywhere else is not used.
const DOWNLOADS: &str = "https://github.com/tajbinkhan/pullbay/releases/download/";
/// The largest Setup Pullbay downloads.
pub const MAX_SETUP_BYTES: u64 = 256 * 1024 * 1024;

/// The Setup asset among a release's `assets`, if there is one Pullbay can
/// use: named `Pullbay-Setup-*.exe`, served from the repository's releases,
/// and of a sensible size.
fn setup_asset(assets: &serde_json::Value) -> Option<SetupAsset> {
    assets.as_array()?.iter().find_map(|a| {
        let name = a["name"].as_str()?;
        let url = a["browser_download_url"].as_str()?;
        let size = a["size"].as_u64()?;
        let lower = name.to_ascii_lowercase();
        let usable = lower.starts_with("pullbay-setup-")
            && lower.ends_with(".exe")
            && !name.contains(['/', '\\', ':'])
            && url.starts_with(DOWNLOADS)
            && (1..=MAX_SETUP_BYTES).contains(&size);
        let sha256 = a["digest"]
            .as_str()
            .and_then(|d| d.strip_prefix("sha256:"))
            .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
            .map(str::to_ascii_lowercase);
        usable.then(|| SetupAsset { name: name.to_string(), url: url.to_string(), size, sha256 })
    })
}

/// Why GitHub's answer or a downloaded Setup is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReleaseError {
    /// What the JSON parser said.
    #[error("GitHub's answer is not JSON: {0}")]
    NotJson(String),
    #[error("GitHub's answer names no release.")]
    NoRelease,
    /// The bytes downloaded and the size GitHub lists.
    #[error("The update is {0} bytes, not the {1} GitHub lists.")]
    WrongSize(usize, u64),
    #[error("The update is not a Windows program.")]
    NotProgram,
    #[error("The update does not match the checksum GitHub lists.")]
    WrongChecksum,
}

/// Check a downloaded Setup against what GitHub said about it: its size,
/// its SHA-256 when given, and that it is a Windows program.
pub fn check_setup(asset: &SetupAsset, bytes: &[u8]) -> Result<(), ReleaseError> {
    use sha2::Digest as _;
    if bytes.len() as u64 != asset.size {
        return Err(ReleaseError::WrongSize(bytes.len(), asset.size));
    }
    if !bytes.starts_with(b"MZ") {
        return Err(ReleaseError::NotProgram);
    }
    if let Some(want) = &asset.sha256 {
        let got: String = sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
        if &got != want {
            return Err(ReleaseError::WrongChecksum);
        }
    }
    Ok(())
}

/// The release in GitHub's answer for the latest release.
pub fn parse_release(json: &[u8]) -> Result<Release, ReleaseError> {
    let v: serde_json::Value = serde_json::from_slice(json).map_err(|e| ReleaseError::NotJson(e.to_string()))?;
    let tag = v["tag_name"].as_str().ok_or(ReleaseError::NoRelease)?;
    let url = v["html_url"].as_str().unwrap_or("https://github.com/tajbinkhan/pullbay/releases");
    // Only a link to GitHub is ever shown.
    let url =
        if url.starts_with("https://github.com/") { url } else { "https://github.com/tajbinkhan/pullbay/releases" };
    Ok(Release {
        version: tag.trim().trim_start_matches(['v', 'V']).to_string(),
        url: url.to_string(),
        setup: setup_asset(&v["assets"]),
    })
}

/// This copy's version.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub's releases at `url`, reached through the settings' proxy.
pub struct GitHub {
    settings: Arc<LiveSettings>,
    retries: Arc<ProxyRetries>,
    url: String,
}

impl GitHub {
    pub fn new(settings: Arc<LiveSettings>, retries: Arc<ProxyRetries>, url: &str) -> Self {
        GitHub { settings, retries, url: url.to_string() }
    }

    /// Fetch `url` through the proxy the settings choose, up to `limit` bytes.
    fn fetch(&self, url: &str, headers: &[(String, String)], limit: u64) -> Result<Vec<u8>, engine::EngineError> {
        let settings = self.settings.get();
        let net = config_for(&Download { url: url.to_string(), ..Download::default() }, &settings, &self.retries);
        engine::fetch(url, headers, &net, limit).map(|page| page.body)
    }
}

impl ReleaseSource for GitHub {
    fn latest(&self) -> Result<Release, UpdateError> {
        let headers = [
            ("User-Agent".to_string(), format!("Pullbay/{VERSION}")),
            ("Accept".to_string(), "application/vnd.github+json".to_string()),
        ];
        let body = self.fetch(&self.url, &headers, 1024 * 1024).map_err(|e| UpdateError::Unreachable(e.to_string()))?;
        let release = parse_release(&body).map_err(|e| UpdateError::Answer(e.to_string()))?;
        // Only Windows has a Setup to run; elsewhere the page is shown.
        Ok(Release { setup: release.setup.filter(|_| cfg!(windows)), ..release })
    }

    fn fetch_setup(&self, setup: &SetupAsset) -> Result<PathBuf, UpdateError> {
        let headers = [("User-Agent".to_string(), format!("Pullbay/{VERSION}"))];
        info!(url = %setup.url, "downloading the update");
        let body =
            self.fetch(&setup.url, &headers, MAX_SETUP_BYTES).map_err(|e| UpdateError::Download(e.to_string()))?;
        check_setup(setup, &body).map_err(|e| UpdateError::BadSetup(e.to_string()))?;
        let path = std::env::temp_dir().join(&setup.name);
        std::fs::write(&path, &body)
            .map_err(|e| UpdateError::Save { path: path.display().to_string(), reason: e.to_string() })?;
        Ok(path)
    }

    fn run_setup(&self, path: &Path) -> Result<(), UpdateError> {
        // /frompullbay closes Pullbay without asking: the user asked for this.
        Command::new(path)
            .arg("/frompullbay=yes")
            .spawn()
            .map(|_| ())
            .map_err(|e| UpdateError::Start { path: path.display().to_string(), reason: e.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testserver::TestServer;
    use pullbay_domain::settings::Settings;

    fn github(url: &str) -> GitHub {
        GitHub::new(Arc::new(LiveSettings::new(Settings::default(), None)), Arc::default(), url)
    }

    #[test]
    fn the_latest_release_is_read_from_the_server() {
        let answer = br#"{"tag_name":"v9.0.0","html_url":"https://github.com/tajbinkhan/pullbay/releases/tag/v9.0.0"}"#;
        let server = TestServer::start(answer.to_vec());
        let release = github(&server.url).latest().unwrap();
        assert_eq!(release.version, "9.0.0");
        assert!(release.setup.is_none());
    }

    #[test]
    fn a_server_that_is_not_there_is_unreachable() {
        let gone = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}/latest", l.local_addr().unwrap())
        };
        assert!(matches!(github(&gone).latest(), Err(UpdateError::Unreachable(_))));
    }

    #[test]
    fn a_setup_that_does_not_match_its_release_is_refused() {
        let server = TestServer::start(b"not a program".to_vec());
        let setup = SetupAsset { name: "Pullbay-Setup-9.exe".into(), url: server.url.clone(), size: 13, sha256: None };
        assert!(matches!(github(&server.url).fetch_setup(&setup), Err(UpdateError::BadSetup(_))));
    }

    #[test]
    fn releases_are_read_from_githubs_answer() {
        let r = parse_release(
            br#"{"tag_name":"v0.4.1","html_url":"https://github.com/tajbinkhan/pullbay/releases/tag/v0.4.1"}"#,
        )
        .unwrap();
        assert_eq!(r.version, "0.4.1");
        assert!(r.url.ends_with("/v0.4.1"));
        let elsewhere = parse_release(br#"{"tag_name":"v9","html_url":"https://evil.example/x"}"#).unwrap();
        assert_eq!(elsewhere.url, "https://github.com/tajbinkhan/pullbay/releases");
        assert!(parse_release(b"{}").is_err());
        assert!(parse_release(b"not json").is_err());
        assert_eq!(r.setup, None);
    }

    #[test]
    fn the_setup_asset_is_found_and_checked() {
        let body = b"MZ setup";
        let sha = {
            use sha2::Digest as _;
            sha2::Sha256::digest(body).iter().map(|b| format!("{b:02x}")).collect::<String>()
        };
        let json = format!(
            r#"{{"tag_name":"v0.5.0","html_url":"https://github.com/tajbinkhan/pullbay/releases/tag/v0.5.0","assets":[
                {{"name":"pullbay-0.5.0.zip","browser_download_url":"https://github.com/tajbinkhan/pullbay/releases/download/v0.5.0/pullbay-0.5.0.zip","size":5}},
                {{"name":"Pullbay-Setup-0.5.0.exe","browser_download_url":"https://github.com/tajbinkhan/pullbay/releases/download/v0.5.0/Pullbay-Setup-0.5.0.exe","size":{},"digest":"sha256:{sha}"}}
            ]}}"#,
            body.len()
        );
        let setup = parse_release(json.as_bytes()).unwrap().setup.unwrap();
        assert_eq!(setup.name, "Pullbay-Setup-0.5.0.exe");
        assert_eq!(setup.sha256.as_deref(), Some(sha.as_str()));
        assert!(check_setup(&setup, body).is_ok());
        assert_eq!(check_setup(&setup, b"MZ setuP").unwrap_err(), ReleaseError::WrongChecksum);
        assert_eq!(check_setup(&setup, b"MZ").unwrap_err(), ReleaseError::WrongSize(2, 8));
        assert_eq!(
            check_setup(&SetupAsset { sha256: None, ..setup.clone() }, b"XX setup").unwrap_err(),
            ReleaseError::NotProgram
        );

        let elsewhere = json.replace(
            "https://github.com/tajbinkhan/pullbay/releases/download/v0.5.0/Pullbay",
            "https://evil.example/Pullbay",
        );
        assert_eq!(parse_release(elsewhere.as_bytes()).unwrap().setup, None, "only from the repository's releases");
    }
}
