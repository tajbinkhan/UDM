//! What the desktop window needs besides its daemon connection (see
//! `daemon::DaemonClient`): ports for its own files, the grabber's library,
//! list files, the settings' disk checks and the operating system.

use std::path::{Path, PathBuf};

use uidm_domain::download::{Download, DownloadEntry};
use uidm_domain::grab::{Filter, GrabRequest, Project, Template};
use uidm_domain::ports::{FilesError, SettingsError};
use uidm_domain::settings::{Settings, Sound};

/// The window's own files, such as its layout: small, rewritten whole, and
/// kept per user.
pub trait WindowFiles: Send + Sync {
    /// The window's file `name`, or `None` when it was never saved or
    /// cannot be read.
    fn read(&self, name: &str) -> Option<Vec<u8>>;

    /// Save `bytes` as the window's file `name`, without waiting: the
    /// write happens on another thread and replaces the file whole, so a
    /// crash never leaves half a file. Of two saves of one name, the later
    /// wins.
    fn save(&self, name: &str, bytes: Vec<u8>);
}

/// The site grabber's saved projects, templates and filters, for the
/// grabber dialog. The window has one when the projects folder can be
/// found.
pub trait GrabLibrary: Send + Sync {
    /// Where the projects are kept, for a file picker to start in.
    fn folder(&self) -> PathBuf;

    /// The saved projects' names, the most recently used first.
    fn recent(&self) -> Vec<String>;

    /// The project named `name`.
    fn load(&self, name: &str) -> Result<Project, FilesError>;

    /// A project file from anywhere, such as one copied from another
    /// computer, read and checked as a saved one is.
    fn load_file(&self, path: &Path) -> Result<Project, FilesError>;

    /// Save `project` under its name, its login cookies sealed.
    fn save(&self, project: &Project) -> Result<(), FilesError>;

    /// Whether no saved project other than `own` (the one being edited)
    /// is called `name`.
    fn name_is_unique(&self, name: &str, own: Option<&str>) -> bool;

    /// The request a new project starts from.
    fn template(&self) -> GrabRequest;

    /// The built-in templates and the saved ones.
    fn templates(&self) -> Vec<Template>;

    /// Save `req` as the template named `name`, and as the one a new
    /// project starts from.
    fn save_template(&self, name: &str, req: &GrabRequest) -> Result<(), FilesError>;

    /// Delete the saved template named `name`.
    fn delete_template(&self, name: &str) -> Result<(), FilesError>;

    /// The file-type filters the user keeps.
    fn filters(&self) -> Vec<Filter>;

    fn save_filters(&self, filters: &[Filter]) -> Result<(), FilesError>;
}

/// The download list's file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFormat {
    Json,
    Text,
    /// IDM's export format.
    Ef2,
}

/// Why a list could not be read, in words for the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ListError(pub String);

/// Exporting and importing the download list.
pub trait ListFiles: Send + Sync {
    /// The text of a list file, whatever its encoding.
    fn decode(&self, bytes: &[u8]) -> String;

    /// The downloads in a list's `text`, and how many lines were not
    /// downloads.
    fn import(&self, text: &str) -> Result<(Vec<Download>, usize), ListError>;

    /// `entries` written as `format`.
    fn export(&self, format: ListFormat, entries: &[&DownloadEntry]) -> String;
}

/// The checks of the settings that look at the disk.
pub trait SettingsChecks: Send + Sync {
    /// The rules `Settings::validate` checks, and then that folder settings
    /// do not name files and ffmpeg is not a folder.
    fn validate(&self, settings: &Settings) -> Result<(), SettingsError>;

    /// The ffmpeg `settings` would use: the one set if it exists, else the
    /// first on `PATH`.
    fn ffmpeg(&self, settings: &Settings) -> Option<PathBuf>;
}

/// The system's fixed proxies, as Options' Get System copies them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemProxy {
    /// `host:port` for http addresses.
    pub http: Option<String>,
    /// `host:port` for https addresses.
    pub https: Option<String>,
    /// `scheme://host:port` of a SOCKS proxy.
    pub socks: Option<String>,
    /// `host:port` of an FTP proxy.
    pub ftp: Option<String>,
    /// The addresses that go without a proxy.
    pub bypass: Vec<String>,
}

/// What the window asks of the operating system.
pub trait SystemServices: Send + Sync {
    /// The system's fixed proxies, without a configuration script. `None`
    /// when none is set.
    fn fixed_proxy(&self) -> Option<SystemProxy>;

    /// Whether the power plan lets programs wake the computer, on mains
    /// power and on battery. `None` when it cannot be read.
    fn wake_timers_allowed(&self) -> Option<(bool, bool)>;

    /// Whether this system can ask for administrator rights to save a file.
    fn can_elevate(&self) -> bool;

    /// Whether a downloaded file's creation time can be set.
    fn can_set_creation_time(&self) -> bool;

    /// Play `file` (a WAV file) for `sound`, or the system's sound for it,
    /// without waiting.
    fn play(&self, sound: Sound, file: Option<&Path>);

    /// Keep `password` for `username` in the system's credential store
    /// under `target`.
    fn save_credential(&self, target: &str, username: &str, password: &str) -> Result<(), FilesError>;
}
