//! Checking GitHub for a newer UDM release, and the file attached to it
//! that installs it here. The window shows a notice with a link to the
//! release page; Update now has the daemon download that file, check it
//! against the release's signed checksums, and install it
//! (`update_in_place`). `GitHub` is the adapter for `ReleaseSource`.
//!
//! The check reads GitHub's website, not its API, which answers a network
//! only 60 times an hour: the latest release's page redirects to the page
//! of its tag, and that release's `SHA256SUMS` names its files.
//!
//! The release workflow lists every file's SHA-256 in `SHA256SUMS` and
//! signs the list with minisign (`installer/sign-checksums.js`). UDM pins
//! the public key, `release-key.pub`, and installs nothing the signed list
//! does not name with the same checksum, so a file swapped on GitHub, or a
//! release made without the key, is refused.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use udm_domain::bytes::ByteCount;

use tracing::info;
use udm_domain::download::Download;
use udm_domain::ports::{ReleaseSource, UpdateProgress};
use udm_domain::update::UpdateError;

use crate::connect::{config_for, NetState};
use crate::engine;
use crate::live_settings::LiveSettings;

use udm_domain::tools::{listed_names, listed_sha256};
use udm_domain::update::{Install, Installer, Release, SetupAsset, SignedChecksums, CHECKSUMS, CHECKSUMS_SIGNATURE};

/// The public half of UDM's release key, in minisign's format.
pub const RELEASE_KEY: &str = include_str!("release-key.pub");

/// The largest checksum list or signature UDM reads.
const MAX_CHECKSUMS_BYTES: ByteCount = ByteCount::kib(64);

/// The page of UDM's latest release, which GitHub redirects to the page of
/// that release's tag. The releases it may name, and their files, are under
/// the same `.../releases`.
pub const LATEST_RELEASE: &str = "https://github.com/tajbinkhan/UDM/releases/latest";

/// The largest Setup UDM downloads.
pub const MAX_SETUP_BYTES: ByteCount = ByteCount::mib(256);

/// The tag of the release whose page is `page`, when that is a release's
/// page under `releases` (`https://github.com/tajbinkhan/UDM/releases`) and
/// the tag is one that can go in an address as it is.
fn release_tag<'a>(page: &'a str, releases: &str) -> Option<&'a str> {
    let tag = page.strip_prefix(releases)?.strip_prefix("/tag/")?;
    let plain = tag.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'));
    (plain && !tag.is_empty() && tag != "." && tag != "..").then_some(tag)
}

/// The release tagged `tag` under `releases`, with the file that updates a
/// copy installed as `install` on a machine whose processor is `arch`: the
/// one of that kind the release's checksum list, `sums`, names. Without a
/// list the release has nothing UDM installs.
pub fn release_from(releases: &str, tag: &str, sums: Option<&str>, install: Install, arch: &str) -> Release {
    let downloads = format!("{releases}/download/{tag}/");
    let setup = install.installer().zip(sums).and_then(|(installer, sums)| {
        let name = listed_names(sums).find(|name| installer.matches(name, arch))?;
        Some(SetupAsset {
            name: name.to_string(),
            url: format!("{downloads}{name}"),
            installer,
            checksums: Some(SignedChecksums {
                list: format!("{downloads}{CHECKSUMS}"),
                signature: format!("{downloads}{CHECKSUMS_SIGNATURE}"),
            }),
        })
    });
    Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        url: format!("{releases}/tag/{tag}"),
        // A package UDM can install itself is updated like any other copy.
        package_manager: install.is_package() && setup.is_none(),
        setup,
    }
}

/// Why GitHub's answer or a downloaded Setup is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReleaseError {
    #[error("GitHub names no latest release.")]
    NoRelease,
    /// What the file should have been.
    #[error("The update is not {0}.")]
    NotProgram(&'static str),
    #[error("This release's checksums are not signed, so UDM will not install it.")]
    Unsigned,
    /// What minisign said.
    #[error("The signature on this release's checksums is not UDM's ({0}), so UDM will not install it.")]
    BadSignature(String),
    #[error("This release's signed checksums do not list the update, so UDM will not install it.")]
    NotListed,
    #[error("The update does not match this release's signed checksums.")]
    WrongSignedChecksum,
}

/// Whether `bytes` start (or end) as an `installer` file does, and if not,
/// what it should have been.
fn looks_like(installer: Installer, bytes: &[u8]) -> Result<(), ReleaseError> {
    let (ok, what) = match installer {
        Installer::WindowsSetup => (bytes.starts_with(b"MZ"), "a Windows program"),
        // A UDIF disk image ends with a 512-byte trailer that starts "koly".
        Installer::MacDiskImage => {
            (bytes.len() >= 512 && bytes[bytes.len() - 512..].starts_with(b"koly"), "a macOS disk image")
        }
        // An ELF program with the AppImage type 2 mark at byte 8.
        Installer::AppImage => {
            (bytes.starts_with(b"\x7fELF") && bytes.get(8..11) == Some(b"AI\x02".as_slice()), "an AppImage")
        }
        Installer::LinuxTar => (bytes.starts_with(&[0x1f, 0x8b]), "a gzip archive"),
        // An ar archive whose first member is debian-binary.
        Installer::Deb => (bytes.starts_with(b"!<arch>\ndebian-binary"), "a Debian package"),
        // The RPM lead's magic number.
        Installer::Rpm => (bytes.starts_with(&[0xed, 0xab, 0xee, 0xdb]), "an RPM package"),
    };
    if ok {
        Ok(())
    } else {
        Err(ReleaseError::NotProgram(what))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A release's `SHA256SUMS` and the text of its minisign signature.
pub struct Signed<'a> {
    pub list: &'a [u8],
    pub signature: &'a str,
}

/// Check a downloaded update against what the release says about it: its
/// kind, and its SHA-256 on the release's list of checksums, which must
/// carry a good signature by `key` (minisign's public key format).
pub fn check_setup(asset: &SetupAsset, bytes: &[u8], signed: Option<Signed>, key: &str) -> Result<(), ReleaseError> {
    looks_like(asset.installer, bytes)?;
    let signed = signed.ok_or(ReleaseError::Unsigned)?;
    verify_list(&signed, key)?;
    let list = std::str::from_utf8(signed.list).map_err(|_| ReleaseError::NotListed)?;
    let want = listed_sha256(list, &asset.name).ok_or(ReleaseError::NotListed)?;
    if sha256_hex(bytes) != want {
        return Err(ReleaseError::WrongSignedChecksum);
    }
    Ok(())
}

/// Whether `signed.signature` is a good signature of `signed.list` by
/// `key`. Only minisign's prehashed signatures are taken.
fn verify_list(signed: &Signed, key: &str) -> Result<(), ReleaseError> {
    let bad = |e: minisign_verify::Error| ReleaseError::BadSignature(e.to_string());
    let key = minisign_verify::PublicKey::decode(key).map_err(bad)?;
    let signature = minisign_verify::Signature::decode(signed.signature).map_err(bad)?;
    key.verify(signed.list, &signature, false).map_err(bad)
}

/// This copy's version.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Why GitHub could not be asked: it turns a network that asked too much
/// away with 403 or 429, which is not a network failure.
fn unreachable(e: engine::EngineError) -> UpdateError {
    match e {
        engine::EngineError::Status(403 | 429) => UpdateError::RateLimited,
        e => UpdateError::Unreachable(e.to_string()),
    }
}

/// GitHub's releases, whose latest one's page is `url` (see
/// `LATEST_RELEASE`), reached through the settings' proxy, for this copy as
/// it was installed, checked against UDM's release key.
pub struct GitHub {
    settings: Arc<LiveSettings>,
    retries: Arc<NetState>,
    url: String,
    install: Install,
    key: &'static str,
}

impl GitHub {
    pub fn new(settings: Arc<LiveSettings>, retries: Arc<NetState>, url: &str) -> Self {
        let install = crate::update_in_place::this_install();
        GitHub { settings, retries, url: url.to_string(), install, key: RELEASE_KEY }
    }

    /// The same, for a copy installed as `install`, trusting `key` instead
    /// of UDM's release key: for tests with a test key.
    pub fn with(mut self, install: Install, key: &'static str) -> Self {
        self.install = install;
        self.key = key;
        self
    }

    /// Fetch `url` through the proxy the settings choose, up to `limit` bytes.
    fn fetch(&self, url: &str, headers: &[(String, String)], limit: ByteCount) -> Result<Vec<u8>, engine::EngineError> {
        engine::fetch(url, headers, &self.net(url), limit).map(|page| page.body)
    }

    /// The proxy and logins the settings give `url`.
    fn net(&self, url: &str) -> crate::net::NetConfig {
        let settings = self.settings.get();
        config_for(&Download { url: url.into(), ..Download::default() }, &settings, &self.retries)
    }
}

impl ReleaseSource for GitHub {
    fn latest(&self) -> Result<Release, UpdateError> {
        let headers = [("User-Agent".to_string(), format!("UDM/{VERSION}"))];
        // Only where the page redirects to is wanted, so none of it is read.
        let page = engine::fetch(&self.url, &headers, &self.net(&self.url), ByteCount::ZERO).map_err(unreachable)?.url;
        let releases = self.url.strip_suffix("/latest").unwrap_or(&self.url);
        let tag =
            release_tag(&page, releases).ok_or_else(|| UpdateError::Answer(ReleaseError::NoRelease.to_string()))?;
        // The list is only read for a copy UDM updates itself.
        let sums = match self.install.installer() {
            None => None,
            Some(_) => {
                match self.fetch(&format!("{releases}/download/{tag}/{CHECKSUMS}"), &headers, MAX_CHECKSUMS_BYTES) {
                    Ok(list) => Some(String::from_utf8_lossy(&list).into_owned()),
                    // A release without the list: its page is shown.
                    Err(engine::EngineError::Status(404)) => None,
                    Err(e) => return Err(unreachable(e)),
                }
            }
        };
        Ok(release_from(releases, tag, sums.as_deref(), self.install, std::env::consts::ARCH))
    }

    fn fetch_setup(&self, setup: &SetupAsset, progress: &UpdateProgress<'_>) -> Result<PathBuf, UpdateError> {
        let headers = [("User-Agent".to_string(), format!("UDM/{VERSION}"))];
        let get = |url: &str, limit| self.fetch(url, &headers, limit).map_err(|e| UpdateError::Download(e.to_string()));
        // The signed list first: a release without one is refused before
        // its installer is downloaded.
        let checksums =
            setup.checksums.as_ref().ok_or_else(|| UpdateError::BadSetup(ReleaseError::Unsigned.to_string()))?;
        let list = get(&checksums.list, MAX_CHECKSUMS_BYTES)?;
        let signature = get(&checksums.signature, MAX_CHECKSUMS_BYTES)?;
        let signed = Signed { list: &list, signature: &String::from_utf8_lossy(&signature) };
        info!(url = %setup.url, "downloading the update");
        let net = self.net(&setup.url);
        let body = engine::fetch_watched(&setup.url, &headers, &net, MAX_SETUP_BYTES, progress)
            .map_err(|e| UpdateError::Download(e.to_string()))?;
        check_setup(setup, &body, Some(signed), self.key).map_err(|e| UpdateError::BadSetup(e.to_string()))?;
        // A fresh folder only this user can write to, so nothing can swap
        // the checked file before it is installed.
        let dir = crate::paths::update_dir()
            .map_err(|e| UpdateError::Save { path: "update".into(), reason: e.to_string() })?;
        let path = dir.join(&setup.name);
        crate::update_in_place::save_private(&dir, &path, &body)
            .map_err(|e| UpdateError::Save { path: path.display().to_string(), reason: e.to_string() })?;
        Ok(path)
    }

    fn run_setup(&self, setup: &SetupAsset, path: &Path) -> Result<(), UpdateError> {
        crate::update_in_place::install(setup.installer, path)
            .map_err(|e| UpdateError::Start { path: path.display().to_string(), reason: e.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testserver::{Behavior, TestServer};
    use std::collections::HashMap;
    use udm_domain::settings::Settings;

    /// A throwaway key made with `installer/sign-checksums.js keygen` for
    /// these tests, the list it signed, and the signature.
    const TEST_KEY: &str = "untrusted comment: minisign public key 0D8A3690E4BCB7BC\n\
        RWS8t7zkkDaKDQzPHF4XUAM+gjUOqQyR1M9SnAIawhiHDWEDcwJ86d62\n";
    const LIST: &str = "574ce2739035aaff515080c231b4fb9ed9103174d63e201caec23d3d9a657dfc  UDM-Setup-0.6.0.exe\n\
        d9298a10d1b0735837dc4bd85dac641b0f3cef27a47e5d53a54f2f3f5b2fcffa  udm-0.6.0-linux-x86_64.tar.gz\n";
    const SIGNATURE: &str = "untrusted comment: signature from UDM's release key\n\
        RUS8t7zkkDaKDRE4b4iHy5xG3IDrKbTvH+UbvbmKOGXjsLvFGBKLNljk8mjA1bR7Jixvf+D2DsFKJ9G5ylglqY3ILTc80O7nDgc=\n\
        trusted comment: timestamp:1794652006\tfile:SHA256SUMS\thashed\n\
        HW3QQ4rVJRPI1qaAyXnK9IpjJjj1ckbDHm4Gzyc23ym1ygLpHyUcs8xzae2/dQWtGOXUCngdDznt/KCigTEfAA==\n";
    /// The Setup the list names.
    const SETUP: &[u8] = b"MZ setup";

    fn github(url: &str) -> GitHub {
        GitHub::new(Arc::new(LiveSettings::new(Settings::default(), None)), Arc::default(), url)
            .with(Install::WindowsSetup, TEST_KEY)
    }

    fn setup(url: &str, checksums: Option<SignedChecksums>) -> SetupAsset {
        SetupAsset {
            name: "UDM-Setup-0.6.0.exe".into(),
            url: url.into(),
            installer: Installer::WindowsSetup,
            checksums,
        }
    }

    fn signed() -> Option<Signed<'static>> {
        Some(Signed { list: LIST.as_bytes(), signature: SIGNATURE })
    }

    /// A server whose `/releases/latest` redirects to the page of `tag`,
    /// which serves `files` too.
    fn releases(tag: &str, mut files: HashMap<String, Vec<u8>>) -> TestServer {
        files.insert(format!("/releases/tag/{tag}"), b"<html>the release's page</html>".to_vec());
        let redirects = HashMap::from([("/releases/latest".to_string(), format!("/releases/tag/{tag}"))]);
        TestServer::start_files_with(files, Behavior { redirects, ..Behavior::default() })
    }

    #[test]
    fn the_latest_release_is_read_from_the_server() {
        let files = HashMap::from([("/releases/download/v0.6.0/SHA256SUMS".to_string(), LIST.as_bytes().to_vec())]);
        let server = releases("v0.6.0", files);
        let release = github(&format!("{}/releases/latest", server.url)).latest().unwrap();
        assert_eq!(release.version, "0.6.0");
        assert_eq!(release.url, format!("{}/releases/tag/v0.6.0", server.url));
        let setup = release.setup.unwrap();
        assert_eq!(setup.name, "UDM-Setup-0.6.0.exe");
        assert_eq!(setup.url, format!("{}/releases/download/v0.6.0/UDM-Setup-0.6.0.exe", server.url));
        let sums = setup.checksums.unwrap();
        assert_eq!(sums.list, format!("{}/releases/download/v0.6.0/SHA256SUMS", server.url));
        assert_eq!(sums.signature, format!("{}/releases/download/v0.6.0/SHA256SUMS.minisig", server.url));
    }

    #[test]
    fn a_release_without_a_checksum_list_offers_its_page() {
        let server = releases("v9.0.0", HashMap::new());
        let latest = format!("{}/releases/latest", server.url);
        let release = github(&latest).latest().unwrap();
        assert_eq!((release.version.as_str(), release.setup), ("9.0.0", None));
        // A copy UDM does not update itself never asks for the list.
        let other = github(&latest).with(Install::Other, TEST_KEY).latest().unwrap();
        assert_eq!((other.version.as_str(), other.setup), ("9.0.0", None));
    }

    #[test]
    fn a_page_that_is_not_a_release_names_none() {
        // A repository without releases sends its latest release's page to
        // the list of them.
        let files = HashMap::from([("/releases".to_string(), b"none yet".to_vec())]);
        let redirects = HashMap::from([("/releases/latest".to_string(), "/releases".to_string())]);
        let server = TestServer::start_files_with(files, Behavior { redirects, ..Behavior::default() });
        let latest = github(&format!("{}/releases/latest", server.url)).latest();
        assert_eq!(latest, Err(UpdateError::Answer(ReleaseError::NoRelease.to_string())));
    }

    #[test]
    fn a_network_github_turns_away_is_told_so() {
        assert_eq!(unreachable(engine::EngineError::Status(403)), UpdateError::RateLimited);
        assert_eq!(unreachable(engine::EngineError::Status(429)), UpdateError::RateLimited);
        assert_eq!(
            unreachable(engine::EngineError::Status(500)),
            UpdateError::Unreachable("server returned HTTP 500".into())
        );
    }

    #[test]
    fn a_tag_is_read_only_from_a_release_page() {
        let releases = "https://github.com/tajbinkhan/UDM/releases";
        assert_eq!(release_tag("https://github.com/tajbinkhan/UDM/releases/tag/v0.11.2", releases), Some("v0.11.2"));
        for page in [
            "https://github.com/tajbinkhan/UDM/releases",
            "https://github.com/tajbinkhan/UDM/releases/tag/",
            "https://github.com/tajbinkhan/UDM/releases/tag/..",
            "https://github.com/tajbinkhan/UDM/releases/tag/v1/../../x",
            "https://github.com/tajbinkhan/UDM/releases/tag/v1?x=1",
            "https://github.com/someone/else/releases/tag/v9",
            "https://evil.example/tajbinkhan/UDM/releases/tag/v9",
        ] {
            assert_eq!(release_tag(page, releases), None, "{page}");
        }
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
    fn a_signed_update_is_downloaded_and_saved() {
        let files = HashMap::from([
            ("/UDM-Setup-0.6.0.exe".to_string(), SETUP.to_vec()),
            ("/SHA256SUMS".to_string(), LIST.as_bytes().to_vec()),
            ("/SHA256SUMS.minisig".to_string(), SIGNATURE.as_bytes().to_vec()),
        ]);
        let server = TestServer::start_files(files);
        let sums = SignedChecksums {
            list: format!("{}/SHA256SUMS", server.url),
            signature: format!("{}/SHA256SUMS.minisig", server.url),
        };
        let asset = setup(&format!("{}/UDM-Setup-0.6.0.exe", server.url), Some(sums.clone()));
        let told = std::sync::Mutex::new(Vec::new());
        let progress = |done: ByteCount, total: Option<ByteCount>| {
            told.lock().unwrap().push((done.get(), total.map(ByteCount::get)))
        };
        let path = github(&server.url).fetch_setup(&asset, &progress).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), SETUP);
        let size = SETUP.len() as u64;
        // Told of the installer alone, from nothing to all of it.
        let told = told.into_inner().unwrap();
        assert_eq!((told.first(), told.last()), (Some(&(0, Some(size))), Some(&(size, Some(size)))));
        let _ = std::fs::remove_file(&path);

        // The same files, checked against another key, are refused.
        let other = GitHub::new(Arc::new(LiveSettings::new(Settings::default(), None)), Arc::default(), &server.url)
            .with(
                Install::WindowsSetup,
                "untrusted comment: minisign public key\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3\n",
            );
        assert!(matches!(other.fetch_setup(&asset, &|_, _| {}), Err(UpdateError::BadSetup(_))));
    }

    #[test]
    fn a_release_without_signed_checksums_is_refused_before_downloading() {
        let server = TestServer::start(SETUP.to_vec());
        let asset = setup(&server.url, None);
        match github(&server.url).fetch_setup(&asset, &|_, _| {}) {
            Err(UpdateError::BadSetup(why)) => assert_eq!(why, ReleaseError::Unsigned.to_string()),
            other => panic!("{other:?}"),
        }
        assert_eq!(server.served.load(std::sync::atomic::Ordering::Relaxed), 0, "nothing was downloaded");
    }

    #[test]
    fn an_update_must_be_on_the_signed_list() {
        let asset = setup("u", None);
        assert_eq!(check_setup(&asset, SETUP, signed(), TEST_KEY), Ok(()));
        assert_eq!(check_setup(&asset, SETUP, None, TEST_KEY), Err(ReleaseError::Unsigned));
        assert_eq!(
            check_setup(&asset, b"MZ setuP", signed(), TEST_KEY),
            Err(ReleaseError::WrongSignedChecksum),
            "the same size, other bytes"
        );
        let unlisted = SetupAsset { name: "UDM-Setup-0.6.1.exe".into(), ..asset.clone() };
        assert_eq!(check_setup(&unlisted, SETUP, signed(), TEST_KEY), Err(ReleaseError::NotListed));

        // A list changed after signing.
        let forged = LIST.replace("574c", "474c");
        let forged = Some(Signed { list: forged.as_bytes(), signature: SIGNATURE });
        assert!(matches!(check_setup(&asset, SETUP, forged, TEST_KEY), Err(ReleaseError::BadSignature(_))));
        let garbage = Some(Signed { list: LIST.as_bytes(), signature: "not a signature" });
        assert!(matches!(check_setup(&asset, SETUP, garbage, TEST_KEY), Err(ReleaseError::BadSignature(_))));
        assert_eq!(check_setup(&asset, b"MZ", signed(), TEST_KEY), Err(ReleaseError::WrongSignedChecksum));
    }

    #[test]
    fn the_pinned_key_reads() {
        assert!(minisign_verify::PublicKey::decode(RELEASE_KEY).is_ok());
    }

    #[test]
    fn each_kind_of_update_must_look_like_one() {
        assert_eq!(looks_like(Installer::WindowsSetup, b"MZ.."), Ok(()));
        assert_eq!(looks_like(Installer::WindowsSetup, b"XX.."), Err(ReleaseError::NotProgram("a Windows program")));
        let mut dmg = vec![0u8; 2048];
        dmg[2048 - 512..2048 - 508].copy_from_slice(b"koly");
        assert_eq!(looks_like(Installer::MacDiskImage, &dmg), Ok(()));
        assert!(looks_like(Installer::MacDiskImage, &dmg[..1000]).is_err());
        assert_eq!(looks_like(Installer::AppImage, b"\x7fELF\x02\x01\x01\x00AI\x02...."), Ok(()));
        assert!(looks_like(Installer::AppImage, b"\x7fELF\x02\x01\x01\x00\x00\x00\x00....").is_err(), "a plain ELF");
        assert_eq!(looks_like(Installer::LinuxTar, &[0x1f, 0x8b, 8]), Ok(()));
        assert!(looks_like(Installer::LinuxTar, b"PK\x03\x04").is_err());
        assert_eq!(
            looks_like(Installer::Deb, b"!<arch>\ndebian-binary   1342977118  0     0     100644  4  `\n"),
            Ok(())
        );
        assert!(looks_like(Installer::Deb, b"!<arch>\ncontrol.tar.gz").is_err(), "some other ar archive");
        assert_eq!(looks_like(Installer::Rpm, &[0xed, 0xab, 0xee, 0xdb, 3, 0]), Ok(()));
        assert!(looks_like(Installer::Rpm, &[0x1f, 0x8b]).is_err());
    }

    const RELEASES: &str = "https://github.com/tajbinkhan/UDM/releases";

    #[test]
    fn each_install_finds_its_file_and_the_signed_list() {
        let sum = "a".repeat(64);
        let list: String = [
            "UDM-0.6.0-windows-portable.zip",
            "UDM-Setup-0.6.0.exe",
            "UDM-0.6.0-macos.dmg",
            "UDM-0.6.0-x86_64.AppImage",
            "udm-0.6.0-linux-x86_64.tar.gz",
            "udm_0.6.0_amd64.deb",
            "udm-0.6.0-1.x86_64.rpm",
        ]
        .iter()
        .map(|name| format!("{sum}  {name}\n"))
        .collect();
        let on = |install, arch: &str| release_from(RELEASES, "v0.6.0", Some(&list), install, arch);
        let read = |install| on(install, "x86_64");
        let release = read(Install::WindowsSetup);
        assert_eq!(release.version, "0.6.0");
        assert_eq!(release.url, "https://github.com/tajbinkhan/UDM/releases/tag/v0.6.0");
        let windows = release.setup.unwrap();
        assert_eq!((windows.name.as_str(), windows.installer), ("UDM-Setup-0.6.0.exe", Installer::WindowsSetup));
        assert_eq!(windows.url, "https://github.com/tajbinkhan/UDM/releases/download/v0.6.0/UDM-Setup-0.6.0.exe");
        let sums = windows.checksums.unwrap();
        assert!(sums.list.ends_with("/v0.6.0/SHA256SUMS") && sums.signature.ends_with("/v0.6.0/SHA256SUMS.minisig"));
        assert_eq!(read(Install::MacApp).setup.unwrap().name, "UDM-0.6.0-macos.dmg");
        assert_eq!(read(Install::AppImage).setup.unwrap().name, "UDM-0.6.0-x86_64.AppImage");
        assert_eq!(read(Install::LinuxTar).setup.unwrap().installer, Installer::LinuxTar);
        let deb = read(Install::Deb);
        assert_eq!(
            deb.setup.as_ref().map(|s| (s.name.as_str(), s.installer)),
            Some(("udm_0.6.0_amd64.deb", Installer::Deb))
        );
        assert!(!deb.package_manager, "UDM installs it");
        let rpm = read(Install::Rpm);
        assert_eq!(
            rpm.setup.as_ref().map(|s| (s.name.as_str(), s.installer)),
            Some(("udm-0.6.0-1.x86_64.rpm", Installer::Rpm))
        );
        let package = read(Install::LinuxPackage);
        assert!(package.setup.is_none() && package.package_manager);
        // No package for this processor: the package manager it is.
        let arm = on(Install::Deb, "aarch64");
        assert!(arm.setup.is_none() && arm.package_manager);
        assert!(read(Install::Other).setup.is_none());
        assert!(on(Install::AppImage, "aarch64").setup.is_none());

        // A release without the list has nothing UDM installs.
        let bare = release_from(RELEASES, "v0.6.0", None, Install::WindowsSetup, "x86_64");
        assert_eq!((bare.version.as_str(), bare.setup), ("0.6.0", None));

        // A name that would leave the release's folder is not a file of it.
        let elsewhere = format!("{sum}  ../../evil/UDM-Setup-0.6.0.exe\n");
        let outside = release_from(RELEASES, "v0.6.0", Some(&elsewhere), Install::WindowsSetup, "x86_64");
        assert_eq!(outside.setup, None, "only from the release's own files");
    }
}
