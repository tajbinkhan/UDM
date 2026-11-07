//! Every folder UIDM uses, and the only module that asks the system where
//! the user's folders are (AGENTS.md section 6). The base folders come from
//! the `directories` crate; UIDM's own folder names are joined onto them
//! here, so each file stays where earlier versions put it.
//!
//! | | Windows | macOS | Linux |
//! |---|---|---|---|
//! | settings, grabber projects ([`config_dir`]) | `%APPDATA%\UIDM` | `~/Library/Application Support/UIDM` | `$XDG_CONFIG_HOME/uidm` (`~/.config/uidm`) |
//! | list, token, quota, keys ([`data_dir`]) | `%LOCALAPPDATA%\UIDM` | `~/Library/Application Support/UIDM` | `$XDG_DATA_HOME/uidm` (`~/.local/share/uidm`) |
//! | daemon log ([`daemon_log`]) | `%TEMP%\uidm-daemon.log` | in [`data_dir`] | in [`data_dir`] |
//! | window layout ([`window_dir`]) | `%LOCALAPPDATA%\UIDM` | none | none |
//! | sign-in entry ([`autostart_dir`]) | the registry | `~/Library/LaunchAgents` | `$XDG_CONFIG_HOME/autostart` |
//!
//! With [`INSTANCE_VAR`] set, [`config_dir`] and [`data_dir`] get the
//! instance's name after a dash (`UIDM-test`), and the daemon its own port
//! (see `ipc::daemon_addr`): a second copy, such as a build under test, runs
//! beside the installed one without sharing its list, settings or daemon.
//! The other folders have no instance name, as before.
//!
//! On Windows, `APPDATA`, `LOCALAPPDATA` and `USERPROFILE` are read first
//! and the known folders only when they are not set. `directories` asks
//! Windows for the known folders and ignores the variables, but end-to-end
//! runs point the variables at a scratch folder to keep a test daemon's
//! files apart, and a process started with them changed expects UIDM to
//! follow them, as it always has.

use std::io;
use std::path::PathBuf;

use directories::BaseDirs;

/// The environment variable naming a separate copy of UIDM. Programs UIDM
/// starts inherit it, so a whole copy (window, daemon, host) stays apart.
pub const INSTANCE_VAR: &str = "UIDM_INSTANCE";

/// The name of this copy of UIDM, from [`INSTANCE_VAR`]; `None` for the
/// usual one. A name is up to 32 letters, digits, `-` and `_`; anything else
/// is ignored, so a typo cannot reach a folder outside UIDM's own.
pub fn instance() -> Option<String> {
    instance_name(std::env::var(INSTANCE_VAR).ok())
}

fn instance_name(value: Option<String>) -> Option<String> {
    let name = value?.trim().to_string();
    let ok =
        !name.is_empty() && name.len() <= 32 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then_some(name)
}

/// UIDM's folder name, `base`, with instance `name` after it.
fn folder(base: &str, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("{base}-{name}"),
        None => base.to_string(),
    }
}

/// UIDM's own folder name on this system, for this copy.
fn own_folder() -> String {
    folder(if cfg!(any(windows, target_os = "macos")) { "UIDM" } else { "uidm" }, instance().as_deref())
}

fn not_found(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("the {what} folder cannot be found"))
}

/// The folder environment variable `var` names, if it names one.
#[cfg(windows)]
fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The base folders a user's folders sit in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Root {
    /// The home folder.
    Home,
    /// `%APPDATA%` on Windows, which roams with the user; the home folder
    /// elsewhere.
    Roaming,
    /// `%LOCALAPPDATA%` on Windows, which stays on this machine; the home
    /// folder elsewhere.
    Local,
}

/// The base folder `root` on this system.
pub fn root(root: Root) -> Option<PathBuf> {
    #[cfg(windows)]
    return match root {
        Root::Home => env_dir("USERPROFILE").or_else(|| BaseDirs::new().map(|b| b.home_dir().to_path_buf())),
        Root::Roaming => env_dir("APPDATA").or_else(|| BaseDirs::new().map(|b| b.config_dir().to_path_buf())),
        Root::Local => env_dir("LOCALAPPDATA").or_else(|| BaseDirs::new().map(|b| b.data_local_dir().to_path_buf())),
    };
    #[cfg(not(windows))]
    {
        let _ = root;
        BaseDirs::new().map(|b| b.home_dir().to_path_buf())
    }
}

/// The running user's home folder.
pub fn home_dir() -> Option<PathBuf> {
    root(Root::Home)
}

/// The folder settings, templates and grabber projects live in, which
/// roams with the user on Windows.
pub fn config_dir() -> io::Result<PathBuf> {
    #[cfg(windows)]
    let base = root(Root::Roaming).ok_or_else(|| not_found("APPDATA"))?;
    // directories puts macOS's config folder in Application Support too.
    #[cfg(not(windows))]
    let base = BaseDirs::new().map(|b| b.config_dir().to_path_buf()).ok_or_else(|| not_found("config"))?;
    Ok(base.join(own_folder()))
}

/// The folder the download list, the daemon's token and other machine-local
/// state live in.
pub fn data_dir() -> io::Result<PathBuf> {
    #[cfg(windows)]
    let base = root(Root::Local).ok_or_else(|| not_found("LOCALAPPDATA"))?;
    #[cfg(not(windows))]
    let base = BaseDirs::new().map(|b| b.data_local_dir().to_path_buf()).ok_or_else(|| not_found("data"))?;
    Ok(base.join(own_folder()))
}

/// The site grabber's projects, templates and filters, next to the
/// settings file.
pub fn grabber_dir() -> io::Result<PathBuf> {
    Ok(config_dir()?.join("grabber"))
}

/// The daemon's log: in the temp folder on Windows, which is the user's
/// own; elsewhere in UIDM's data folder, as `/tmp` is shared by every user
/// and the log holds download addresses.
pub fn daemon_log() -> PathBuf {
    if cfg!(windows) {
        return std::env::temp_dir().join("uidm-daemon.log");
    }
    data_dir().map(|d| d.join("uidm-daemon.log")).unwrap_or_else(|_| std::env::temp_dir().join("uidm-daemon.log"))
}

/// The folder the window keeps its layout and the drop target's place in:
/// `%LOCALAPPDATA%\UIDM` on Windows, shared by every instance. Elsewhere
/// the window keeps neither.
pub fn window_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    return root(Root::Local).map(|base| base.join("UIDM"));
    #[cfg(not(windows))]
    return None;
}

/// The folder the entry that starts UIDM at sign-in goes in. Windows keeps
/// it in the registry instead.
#[cfg(unix)]
pub fn autostart_dir() -> io::Result<PathBuf> {
    #[cfg(target_os = "macos")]
    return Ok(home_dir().ok_or_else(|| not_found("home"))?.join("Library/LaunchAgents"));
    #[cfg(not(target_os = "macos"))]
    return Ok(BaseDirs::new().ok_or_else(|| not_found("config"))?.config_dir().join("autostart"));
}

/// Where Windows keeps app packages' private copies of AppData:
/// `%LOCALAPPDATA%\Packages`.
#[cfg(windows)]
pub fn packages_dir() -> Option<PathBuf> {
    root(Root::Local).map(|base| base.join("Packages"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instance_name_is_short_and_plain() {
        assert_eq!(instance_name(Some(" test ".into())).as_deref(), Some("test"));
        assert_eq!(instance_name(Some("dev_2-a".into())).as_deref(), Some("dev_2-a"));
        assert_eq!(instance_name(None), None);
        assert_eq!(instance_name(Some("".into())), None);
        assert_eq!(instance_name(Some("../x".into())), None, "no path parts");
        assert_eq!(instance_name(Some("a".repeat(33))), None);
    }

    #[test]
    fn an_instance_gets_folders_of_its_own() {
        assert_eq!(folder("UIDM", None), "UIDM");
        assert_eq!(folder("UIDM", Some("test")), "UIDM-test");
        assert_eq!(folder("uidm", Some("dev_2")), "uidm-dev_2");
    }

    fn home() -> PathBuf {
        home_dir().expect("tests run with a home folder")
    }

    #[cfg(windows)]
    #[test]
    fn windows_folders_sit_in_appdata() {
        let roaming = root(Root::Roaming).unwrap();
        let local = root(Root::Local).unwrap();
        assert_eq!(roaming, PathBuf::from(std::env::var_os("APPDATA").unwrap()));
        assert_eq!(local, PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()));
        assert_eq!(home(), PathBuf::from(std::env::var_os("USERPROFILE").unwrap()));
        assert_eq!(config_dir().unwrap(), roaming.join(own_folder()));
        assert_eq!(data_dir().unwrap(), local.join(own_folder()));
        assert_eq!(grabber_dir().unwrap(), roaming.join(own_folder()).join("grabber"));
        assert_eq!(window_dir().unwrap(), local.join("UIDM"));
        assert_eq!(packages_dir().unwrap(), local.join("Packages"));
        assert_eq!(daemon_log(), std::env::temp_dir().join("uidm-daemon.log"));
        assert!(own_folder().starts_with("UIDM"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_known_folders_match_the_variables() {
        // The fallback for an unset variable lands where the variable would
        // have pointed.
        let dirs = BaseDirs::new().unwrap();
        assert_eq!(dirs.config_dir(), root(Root::Roaming).unwrap());
        assert_eq!(dirs.data_local_dir(), root(Root::Local).unwrap());
        assert_eq!(dirs.home_dir(), home());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_folders_sit_in_application_support() {
        let support = home().join("Library/Application Support");
        assert_eq!(config_dir().unwrap(), support.join(own_folder()));
        assert_eq!(data_dir().unwrap(), support.join(own_folder()));
        assert_eq!(grabber_dir().unwrap(), support.join(own_folder()).join("grabber"));
        assert_eq!(daemon_log(), support.join(own_folder()).join("uidm-daemon.log"));
        assert_eq!(autostart_dir().unwrap(), home().join("Library/LaunchAgents"));
        assert_eq!(window_dir(), None);
        assert_eq!(root(Root::Local), Some(home()));
        assert!(own_folder().starts_with("UIDM"));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_folders_follow_xdg() {
        let xdg = |var: &str, fallback: &str| {
            std::env::var_os(var)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home().join(fallback))
        };
        let config = xdg("XDG_CONFIG_HOME", ".config");
        let data = xdg("XDG_DATA_HOME", ".local/share");
        assert_eq!(config_dir().unwrap(), config.join(own_folder()));
        assert_eq!(data_dir().unwrap(), data.join(own_folder()));
        assert_eq!(grabber_dir().unwrap(), config.join(own_folder()).join("grabber"));
        assert_eq!(daemon_log(), data.join(own_folder()).join("uidm-daemon.log"));
        assert_eq!(autostart_dir().unwrap(), config.join("autostart"));
        assert_eq!(window_dir(), None);
        assert_eq!(root(Root::Roaming), Some(home()));
        assert!(own_folder().starts_with("uidm"));
    }
}
