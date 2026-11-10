//! Every folder Pullbay uses, and the only module that asks the system where
//! the user's folders are (AGENTS.md section 6). The base folders come from
//! the `directories` crate; Pullbay's own folder names are joined onto them
//! here, so each file stays where earlier versions put it.
//!
//! | | Windows | macOS | Linux |
//! |---|---|---|---|
//! | settings, grabber projects ([`config_dir`]) | `%APPDATA%\Pullbay` | `~/Library/Application Support/Pullbay` | `$XDG_CONFIG_HOME/pullbay` (`~/.config/pullbay`) |
//! | list, token, quota, keys ([`data_dir`]) | `%LOCALAPPDATA%\Pullbay` | `~/Library/Application Support/Pullbay` | `$XDG_DATA_HOME/pullbay` (`~/.local/share/pullbay`) |
//! | logs ([`daemon_log`], [`host_log`], [`gui_log`], [`cli_log`], [`mcp_log`]) | in [`data_dir`] | in [`data_dir`] | in [`data_dir`] |
//! | window layout ([`window_dir`]) | `%LOCALAPPDATA%\Pullbay` | none | none |
//! | sign-in entry ([`autostart_dir`]) | the registry | `~/Library/LaunchAgents` | `$XDG_CONFIG_HOME/autostart` |
//!
//! With [`INSTANCE_VAR`] set, [`config_dir`] and [`data_dir`] get the
//! instance's name after a dash (`Pullbay-test`), and the daemon its own port
//! (see `ipc::daemon_addr`): a second copy, such as a build under test, runs
//! beside the installed one without sharing its list, settings or daemon.
//! The other folders have no instance name, as before.
//!
//! A portable copy, one with [`PORTABLE_MARKER`] beside its programs (see
//! [`portable_root`]), keeps all of these in its own `Data` folder instead
//! (`Data-test` for an instance), downloads into its own `Downloads` folder
//! ([`portable_downloads_dir`]) and has no sign-in entry, so it can run from a
//! USB stick and leave the computer as it found it (`docs/portable.md`).
//!
//! Two variables are read here rather than left to `directories`, so the
//! folders stay where earlier versions resolved them:
//!
//! - On Windows, `APPDATA`, `LOCALAPPDATA` and `USERPROFILE` come first and
//!   the known folders only when they are not set. `directories` asks
//!   Windows for the known folders and ignores the variables, but end-to-end
//!   runs point the variables at a scratch folder to keep a test daemon's
//!   files apart, and a process started with them changed expects Pullbay to
//!   follow them, as it always has.
//! - On Linux, any non-empty `XDG_CONFIG_HOME` and `XDG_DATA_HOME` is used
//!   as given. `directories` ignores a relative one, which would move the
//!   files of a user who has one set.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use directories::BaseDirs;

/// The environment variable naming a separate copy of Pullbay. Programs Pullbay
/// starts inherit it, so a whole copy (window, daemon, host) stays apart.
pub const INSTANCE_VAR: &str = "PULLBAY_INSTANCE";

/// Pullbay's own folder name on this system, before any instance name.
const OWN: &str = if cfg!(any(windows, target_os = "macos")) { "Pullbay" } else { "pullbay" };

/// The name of this copy of Pullbay, from [`INSTANCE_VAR`]; `None` for the
/// usual one. A name is up to 32 letters, digits, `-` and `_`; anything else
/// is ignored, so a typo cannot reach a folder outside Pullbay's own.
pub fn instance() -> Option<String> {
    instance_name(std::env::var(INSTANCE_VAR).ok())
}

fn instance_name(value: Option<String>) -> Option<String> {
    let name = value?.trim().to_string();
    let ok =
        !name.is_empty() && name.len() <= 32 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then_some(name)
}

/// The file that makes a copy portable when it sits beside the programs, or
/// in the folder above `target\release` as in the portable zip. What it says
/// does not matter.
pub const PORTABLE_MARKER: &str = "portable.txt";

/// The folder a portable copy keeps its settings, list and logs in, inside
/// the copy's own folder.
const PORTABLE_DATA: &str = "Data";

/// The folder a portable copy downloads into by default.
const PORTABLE_DOWNLOADS: &str = "Downloads";

/// The folder of a portable copy: the one holding [`PORTABLE_MARKER`] next to
/// the running program, or `None` for a copy that uses the system's
/// folders. It is looked up once, since the programs do not move while they
/// run.
pub fn portable_root() -> Option<&'static Path> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        let exe = std::env::current_exe().ok()?;
        portable_root_from(exe.parent()?)
    })
    .as_deref()
}

/// Whether this is a portable copy (see [`portable_root`]).
pub fn is_portable() -> bool {
    portable_root().is_some()
}

/// The portable copy's folder for programs in `programs`: `programs` itself
/// when the marker is there, or, for programs in `target\release` as the
/// portable zip has them, the folder holding `target` when it is there.
fn portable_root_from(programs: &Path) -> Option<PathBuf> {
    let zip_root = programs
        .parent()
        .filter(|target| {
            programs.file_name().is_some_and(|n| n == "release") && target.file_name().is_some_and(|n| n == "target")
        })
        .and_then(Path::parent);
    std::iter::once(programs).chain(zip_root).find(|dir| dir.join(PORTABLE_MARKER).is_file()).map(Path::to_path_buf)
}

/// Where Pullbay's own folders go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place<'a> {
    /// The system's folders for this user.
    System,
    /// A portable copy's folder.
    Portable(&'a Path),
}

fn place() -> Place<'static> {
    portable_root().map_or(Place::System, Place::Portable)
}

/// A portable copy's folder for its settings and data, for instance `name`.
fn portable_data(root: &Path, name: Option<&str>) -> PathBuf {
    root.join(folder_named(PORTABLE_DATA, name))
}

/// The folder name versions up to 0.3.3 used, when Pullbay was called UIDM.
const UIDM_OWN: &str = if cfg!(any(windows, target_os = "macos")) { "UIDM" } else { "uidm" };

/// Pullbay's folder name for instance `name`: `Pullbay`, or `Pullbay-test`.
fn own_folder(name: Option<&str>) -> String {
    folder_named(OWN, name)
}

/// Folder `own` for instance `name`: `own`, or `own-name`.
fn folder_named(own: &str, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("{own}-{name}"),
        None => own.to_string(),
    }
}

fn not_found(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("the {what} folder cannot be found"))
}

/// The folder an environment variable's `value` names, if it names one:
/// anything but empty, used as given.
#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
fn named_dir(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The folder environment variable `var` names, if it names one.
#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
fn env_dir(var: &str) -> Option<PathBuf> {
    named_dir(std::env::var_os(var))
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

/// The base folder settings go in: `%APPDATA%`, Application Support, or
/// `$XDG_CONFIG_HOME` (`~/.config`).
fn config_base() -> io::Result<PathBuf> {
    #[cfg(windows)]
    return root(Root::Roaming).ok_or_else(|| not_found("APPDATA"));
    // directories puts macOS's config folder in Application Support too.
    #[cfg(target_os = "macos")]
    return BaseDirs::new().map(|b| b.config_dir().to_path_buf()).ok_or_else(|| not_found("config"));
    #[cfg(all(unix, not(target_os = "macos")))]
    return env_dir("XDG_CONFIG_HOME")
        .or_else(|| home_dir().map(|h| h.join(".config")))
        .ok_or_else(|| not_found("config"));
}

/// The base folder machine-local state goes in: `%LOCALAPPDATA%`,
/// Application Support, or `$XDG_DATA_HOME` (`~/.local/share`).
fn data_base() -> io::Result<PathBuf> {
    #[cfg(windows)]
    return root(Root::Local).ok_or_else(|| not_found("LOCALAPPDATA"));
    #[cfg(target_os = "macos")]
    return BaseDirs::new().map(|b| b.data_local_dir().to_path_buf()).ok_or_else(|| not_found("data"));
    #[cfg(all(unix, not(target_os = "macos")))]
    return env_dir("XDG_DATA_HOME")
        .or_else(|| home_dir().map(|h| h.join(".local/share")))
        .ok_or_else(|| not_found("data"));
}

fn config_dir_for(name: Option<&str>) -> io::Result<PathBuf> {
    config_dir_in(place(), name)
}

fn config_dir_in(place: Place, name: Option<&str>) -> io::Result<PathBuf> {
    match place {
        Place::System => Ok(config_base()?.join(own_folder(name))),
        Place::Portable(root) => Ok(portable_data(root, name)),
    }
}

fn data_dir_for(name: Option<&str>) -> io::Result<PathBuf> {
    data_dir_in(place(), name)
}

fn data_dir_in(place: Place, name: Option<&str>) -> io::Result<PathBuf> {
    match place {
        Place::System => Ok(data_base()?.join(own_folder(name))),
        Place::Portable(root) => Ok(portable_data(root, name)),
    }
}

fn grabber_dir_for(name: Option<&str>) -> io::Result<PathBuf> {
    Ok(config_dir_for(name)?.join("grabber"))
}

/// The log called `file` in the data folder, or in the temp folder when the
/// data folder cannot be found.
fn log_for(name: Option<&str>, file: &str) -> PathBuf {
    data_dir_for(name).map(|d| d.join(file)).unwrap_or_else(|_| std::env::temp_dir().join(file))
}

const DAEMON_LOG: &str = "pullbay-daemon.log";
const HOST_LOG: &str = "pullbay-host.log";
const GUI_LOG: &str = "pullbay-gui.log";
const CLI_LOG: &str = "pullbay-cli.log";
const MCP_LOG: &str = "pullbay-mcp.log";

/// This copy's settings and data folders under UIDM's name, each with the
/// Pullbay folder it moves to: `(old, new)`. macOS has one pair, since both
/// are the same folder there. A portable copy has none: UIDM's folders
/// belong to an installed copy.
pub fn uidm_folders() -> io::Result<Vec<(PathBuf, PathBuf)>> {
    if is_portable() {
        return Ok(Vec::new());
    }
    uidm_folders_for(instance().as_deref())
}

fn uidm_folders_for(name: Option<&str>) -> io::Result<Vec<(PathBuf, PathBuf)>> {
    let mut pairs = vec![
        (config_base()?.join(folder_named(UIDM_OWN, name)), config_dir_for(name)?),
        (data_base()?.join(folder_named(UIDM_OWN, name)), data_dir_for(name)?),
    ];
    pairs.dedup();
    Ok(pairs)
}

/// The folder settings, templates and grabber projects live in, which
/// roams with the user on Windows.
pub fn config_dir() -> io::Result<PathBuf> {
    config_dir_for(instance().as_deref())
}

/// The folder the download list, the daemon's token and other machine-local
/// state live in.
pub fn data_dir() -> io::Result<PathBuf> {
    data_dir_for(instance().as_deref())
}

/// The site grabber's projects, templates and filters, next to the
/// settings file.
pub fn grabber_dir() -> io::Result<PathBuf> {
    grabber_dir_for(instance().as_deref())
}

/// The daemon's log, in [`data_dir`]. It holds download addresses, so it
/// stays out of a shared `/tmp`.
pub fn daemon_log() -> PathBuf {
    log_for(instance().as_deref(), DAEMON_LOG)
}

/// The native-messaging host's log, beside the daemon's.
pub fn host_log() -> PathBuf {
    log_for(instance().as_deref(), HOST_LOG)
}

/// The window's log, beside the daemon's.
pub fn gui_log() -> PathBuf {
    log_for(instance().as_deref(), GUI_LOG)
}

/// The command-line tool's log, beside the daemon's.
pub fn cli_log() -> PathBuf {
    log_for(instance().as_deref(), CLI_LOG)
}

/// The AI assistants' server's log, beside the daemon's.
pub fn mcp_log() -> PathBuf {
    log_for(instance().as_deref(), MCP_LOG)
}

/// The Windows font folder, which the window's fonts come from. Elsewhere
/// the window draws with egui's own.
pub fn fonts_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    return std::env::var_os("WINDIR").map(|w| PathBuf::from(w).join("Fonts"));
    #[cfg(not(windows))]
    None
}

/// The folder the window keeps its layout and the drop target's place in:
/// `%LOCALAPPDATA%\Pullbay` on Windows (a portable copy's `Data` folder),
/// shared by every instance. Elsewhere the window keeps neither.
pub fn window_dir() -> Option<PathBuf> {
    window_dir_in(place())
}

fn window_dir_in(place: Place) -> Option<PathBuf> {
    #[cfg(windows)]
    return match place {
        Place::System => root(Root::Local).map(|base| base.join("Pullbay")),
        Place::Portable(root) => Some(portable_data(root, None)),
    };
    #[cfg(not(windows))]
    {
        let _ = place;
        None
    }
}

/// The folder a portable copy downloads into when the settings name none:
/// `Downloads` in the copy's folder, on the same drive. `None` for a copy
/// that uses the system's Downloads folder.
pub fn portable_downloads_dir() -> Option<PathBuf> {
    portable_root().map(|root| root.join(PORTABLE_DOWNLOADS))
}

/// The folder the engine keeps a download's parts in when the settings name
/// no temporary folder: the system's temporary folder, or a portable copy's
/// `Data` folder, so a copy that stops halfway leaves nothing behind on the
/// computer.
pub fn parts_dir() -> PathBuf {
    parts_dir_in(place(), instance().as_deref())
}

fn parts_dir_in(place: Place, name: Option<&str>) -> PathBuf {
    match place {
        Place::System => std::env::temp_dir().join(OWN_TEMP),
        Place::Portable(root) => portable_data(root, name).join("parts"),
    }
}

/// The folder in the system's temporary folder the engine keeps parts in.
const OWN_TEMP: &str = "Pullbay";

/// The folder the entry that starts Pullbay at sign-in goes in. Windows keeps
/// it in the registry instead. A portable copy has no sign-in entry (see
/// `autostart`), so this is the installed copy's folder.
#[cfg(unix)]
pub fn autostart_dir() -> io::Result<PathBuf> {
    #[cfg(target_os = "macos")]
    return Ok(home_dir().ok_or_else(|| not_found("home"))?.join("Library/LaunchAgents"));
    #[cfg(not(target_os = "macos"))]
    return Ok(config_base()?.join("autostart"));
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
        let config = config_dir_for(None).unwrap();
        let data = data_dir_for(None).unwrap();
        let base = |dir: &PathBuf| dir.parent().unwrap().to_path_buf();
        let suffix = format!("{OWN}-test");
        assert_eq!(config_dir_for(Some("test")).unwrap(), base(&config).join(&suffix));
        assert_eq!(data_dir_for(Some("test")).unwrap(), base(&data).join(&suffix));
        assert_eq!(grabber_dir_for(None).unwrap(), config.join("grabber"));
        assert_eq!(grabber_dir_for(Some("test")).unwrap(), base(&config).join(&suffix).join("grabber"));
    }

    #[test]
    fn uidm_folders_move_to_pullbays() {
        let pairs = uidm_folders_for(None).unwrap();
        assert_eq!(pairs.len(), if cfg!(target_os = "macos") { 1 } else { 2 });
        for (old, new) in &pairs {
            assert_eq!(old.parent(), new.parent(), "{old:?} moves beside itself");
            assert_eq!(old.file_name().unwrap(), UIDM_OWN);
            assert_eq!(new.file_name().unwrap(), OWN);
        }
        assert_eq!(pairs[0].1, config_dir_for(None).unwrap());
        let test = uidm_folders_for(Some("test")).unwrap();
        assert_eq!(test[0].0.file_name().unwrap(), format!("{UIDM_OWN}-test").as_str());
    }

    fn home() -> PathBuf {
        home_dir().expect("tests run with a home folder")
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pullbay_paths_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_marker_beside_the_programs_makes_a_copy_portable() {
        let dir = scratch("marker");
        assert_eq!(portable_root_from(&dir), None, "no marker, the system's folders");
        std::fs::create_dir(dir.join(PORTABLE_MARKER)).unwrap();
        assert_eq!(portable_root_from(&dir), None, "a folder is not the marker");
        std::fs::remove_dir(dir.join(PORTABLE_MARKER)).unwrap();
        std::fs::write(dir.join(PORTABLE_MARKER), "").unwrap();
        assert_eq!(portable_root_from(&dir), Some(dir.clone()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_portable_zip_keeps_its_marker_above_target_release() {
        let dir = scratch("zip");
        let release = dir.join("target").join("release");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(dir.join(PORTABLE_MARKER), "").unwrap();
        assert_eq!(portable_root_from(&release), Some(dir.clone()));
        // Only above target\release, not above any folder.
        let other = dir.join("bin").join("release");
        std::fs::create_dir_all(&other).unwrap();
        assert_eq!(portable_root_from(&other), None);
        // A marker beside the programs wins.
        std::fs::write(release.join(PORTABLE_MARKER), "").unwrap();
        assert_eq!(portable_root_from(&release), Some(release.clone()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_portable_copy_keeps_everything_in_its_folder() {
        let root = Path::new("portable-root");
        let at = Place::Portable(root);
        assert_eq!(config_dir_in(at, None).unwrap(), root.join("Data"));
        assert_eq!(data_dir_in(at, None).unwrap(), root.join("Data"));
        assert_eq!(config_dir_in(at, Some("test")).unwrap(), root.join("Data-test"));
        assert_eq!(data_dir_in(at, Some("test")).unwrap(), root.join("Data-test"));
        assert_eq!(parts_dir_in(at, None), root.join("Data").join("parts"));
        assert_eq!(parts_dir_in(at, Some("test")), root.join("Data-test").join("parts"));
        assert_eq!(window_dir_in(at), if cfg!(windows) { Some(root.join("Data")) } else { None });
        assert_eq!(parts_dir_in(Place::System, None), std::env::temp_dir().join("Pullbay"));
    }

    #[test]
    fn tests_run_as_an_installed_copy() {
        assert_eq!(place(), Place::System);
        assert_eq!(portable_downloads_dir(), None);
    }

    #[cfg(windows)]
    fn var(name: &str) -> PathBuf {
        PathBuf::from(std::env::var_os(name).expect("the test runs with the variable set"))
    }

    #[cfg(windows)]
    #[test]
    fn windows_folders_sit_in_appdata() {
        let roaming = var("APPDATA");
        let local = var("LOCALAPPDATA");
        assert_eq!(root(Root::Roaming).unwrap(), roaming);
        assert_eq!(root(Root::Local).unwrap(), local);
        assert_eq!(home(), var("USERPROFILE"));
        assert_eq!(config_dir_for(None).unwrap(), roaming.join("Pullbay"));
        assert_eq!(config_dir_for(Some("test")).unwrap(), roaming.join("Pullbay-test"));
        assert_eq!(data_dir_for(None).unwrap(), local.join("Pullbay"));
        assert_eq!(data_dir_for(Some("test")).unwrap(), local.join("Pullbay-test"));
        assert_eq!(grabber_dir_for(None).unwrap(), roaming.join(r"Pullbay\grabber"));
        assert_eq!(grabber_dir_for(Some("test")).unwrap(), roaming.join(r"Pullbay-test\grabber"));
        // The window's files are shared by every instance; the logs are not.
        assert_eq!(window_dir().unwrap(), local.join("Pullbay"));
        assert_eq!(log_for(None, DAEMON_LOG), local.join(r"Pullbay\pullbay-daemon.log"));
        assert_eq!(log_for(Some("test"), DAEMON_LOG), local.join(r"Pullbay-test\pullbay-daemon.log"));
        assert_eq!(log_for(None, HOST_LOG), local.join(r"Pullbay\pullbay-host.log"));
        assert_eq!(log_for(None, GUI_LOG), local.join(r"Pullbay\pullbay-gui.log"));
        assert_eq!(log_for(None, CLI_LOG), local.join(r"Pullbay\pullbay-cli.log"));
        assert_eq!(log_for(None, MCP_LOG), local.join(r"Pullbay\pullbay-mcp.log"));
        assert_eq!(packages_dir().unwrap(), local.join("Packages"));
    }

    #[cfg(windows)]
    #[test]
    fn a_set_variable_is_used_as_given() {
        assert_eq!(named_dir(Some(r"D:\scratch\Roaming".into())), Some(PathBuf::from(r"D:\scratch\Roaming")));
        assert_eq!(named_dir(Some("".into())), None, "an empty variable is not set");
        assert_eq!(named_dir(None), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_folders_sit_in_application_support() {
        let support = home().join("Library/Application Support");
        assert_eq!(config_dir_for(None).unwrap(), support.join("Pullbay"));
        assert_eq!(config_dir_for(Some("test")).unwrap(), support.join("Pullbay-test"));
        assert_eq!(data_dir_for(None).unwrap(), support.join("Pullbay"));
        assert_eq!(data_dir_for(Some("test")).unwrap(), support.join("Pullbay-test"));
        assert_eq!(grabber_dir_for(None).unwrap(), support.join("Pullbay/grabber"));
        assert_eq!(grabber_dir_for(Some("test")).unwrap(), support.join("Pullbay-test/grabber"));
        assert_eq!(log_for(None, DAEMON_LOG), support.join("Pullbay/pullbay-daemon.log"));
        assert_eq!(log_for(Some("test"), DAEMON_LOG), support.join("Pullbay-test/pullbay-daemon.log"));
        assert_eq!(log_for(None, HOST_LOG), support.join("Pullbay/pullbay-host.log"));
        assert_eq!(log_for(None, GUI_LOG), support.join("Pullbay/pullbay-gui.log"));
        assert_eq!(log_for(None, CLI_LOG), support.join("Pullbay/pullbay-cli.log"));
        assert_eq!(log_for(None, MCP_LOG), support.join("Pullbay/pullbay-mcp.log"));
        assert_eq!(autostart_dir().unwrap(), home().join("Library/LaunchAgents"));
        assert_eq!(window_dir(), None);
        assert_eq!(root(Root::Local), Some(home()));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_folders_follow_xdg() {
        // Earlier versions used any non-empty XDG value as given, relative
        // ones too.
        assert_eq!(named_dir(Some("rel/config".into())), Some(PathBuf::from("rel/config")));
        assert_eq!(named_dir(Some("/abs/config".into())), Some(PathBuf::from("/abs/config")));
        assert_eq!(named_dir(Some("".into())), None, "an empty variable is not set");
        assert_eq!(named_dir(None), None);

        let config = env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"));
        let data = env_dir("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share"));
        assert_eq!(config_base().unwrap(), config);
        assert_eq!(data_base().unwrap(), data);
        assert_eq!(config_dir_for(None).unwrap(), config.join("pullbay"));
        assert_eq!(config_dir_for(Some("test")).unwrap(), config.join("pullbay-test"));
        assert_eq!(data_dir_for(None).unwrap(), data.join("pullbay"));
        assert_eq!(data_dir_for(Some("test")).unwrap(), data.join("pullbay-test"));
        assert_eq!(grabber_dir_for(None).unwrap(), config.join("pullbay/grabber"));
        assert_eq!(grabber_dir_for(Some("test")).unwrap(), config.join("pullbay-test/grabber"));
        assert_eq!(log_for(None, DAEMON_LOG), data.join("pullbay/pullbay-daemon.log"));
        assert_eq!(log_for(Some("test"), DAEMON_LOG), data.join("pullbay-test/pullbay-daemon.log"));
        assert_eq!(log_for(None, HOST_LOG), data.join("pullbay/pullbay-host.log"));
        assert_eq!(log_for(None, GUI_LOG), data.join("pullbay/pullbay-gui.log"));
        assert_eq!(log_for(None, CLI_LOG), data.join("pullbay/pullbay-cli.log"));
        assert_eq!(log_for(None, MCP_LOG), data.join("pullbay/pullbay-mcp.log"));
        assert_eq!(autostart_dir().unwrap(), config.join("autostart"));
        assert_eq!(window_dir(), None);
        assert_eq!(root(Root::Roaming), Some(home()));
    }
}
