//! `pullbay-daemon`'s command line. A daemon the browser, the window or
//! `pullbay-cli` starts gets no arguments and runs beside the desktop; a
//! server's is started with `--server` by its service manager or container
//! (`docs/server.md`), and may name its folders here instead of in the
//! environment (`paths::Folders`).
//!
//! ```text
//! pullbay-daemon
//! pullbay-daemon --server [--config DIR] [--data DIR] [--downloads DIR]
//! pullbay-daemon --help | --version
//! ```

use std::path::{Path, PathBuf};

use pullbay_domain::seat::Seat;

use crate::paths::Folders;

/// What `pullbay-daemon --help` prints.
pub const USAGE: &str = "\
Usage: pullbay-daemon [--server [options]]

Pullbay's background service, which runs the downloads. The browser, the window and
pullbay-cli start it without arguments when they need it, and it exits once it has
had nothing to do for a minute.

  --server           Run on a server or NAS: keep running until stopped, log to
                     standard output, and show no window or notification
    --config DIR     keep the settings in DIR (or set PULLBAY_CONFIG_DIR)
    --data DIR       keep the download list, the token and the keys in DIR
                     (or set PULLBAY_DATA_DIR)
    --downloads DIR  download into DIR when the settings name no folder
                     (or set PULLBAY_DOWNLOADS_DIR)
  --help, --version

Programs that talk to a server's daemon, such as pullbay-cli, find it through the
same PULLBAY_DATA_DIR. See docs/server.md.";

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// Run the daemon.
    Run {
        seat: Seat,
        folders: Folders,
    },
    /// Move a file saved with administrator rights into place and exit
    /// (`elevate::MOVE_ARG`).
    Move {
        from: PathBuf,
        to: PathBuf,
    },
    Help,
    Version,
}

/// Why a command line cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UsageError {
    #[error("{0} is not an option of pullbay-daemon")]
    UnknownOption(String),
    #[error("{0} needs a folder after it")]
    NeedsFolder(String),
    #[error("{0} is for a server; add --server")]
    NeedsServer(String),
    #[error("{0} is given twice")]
    Twice(String),
    #[error("did not expect {0}")]
    Extra(String),
}

/// Read `args` (without the program name). `cwd` resolves a relative
/// folder.
pub fn parse<S: AsRef<str>>(args: &[S], cwd: &Path) -> Result<Invocation, UsageError> {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    if let [flag, from, to] = args.as_slice() {
        if *flag == crate::elevate::MOVE_ARG {
            return Ok(Invocation::Move { from: PathBuf::from(from), to: PathBuf::from(to) });
        }
    }
    let mut server = false;
    let mut folders = Folders::default();
    let mut named: Vec<String> = Vec::new();
    let mut words = args.iter();
    while let Some(&word) = words.next() {
        let (name, inline) = match word.split_once('=').filter(|_| word.starts_with("--")) {
            Some((name, value)) => (name, Some(value)),
            None => (word, None),
        };
        let slot = match name {
            "--help" | "-h" if inline.is_none() => return Ok(Invocation::Help),
            "--version" | "-V" if inline.is_none() => return Ok(Invocation::Version),
            "--server" if inline.is_none() => {
                server = true;
                continue;
            }
            "--config" => &mut folders.config,
            "--data" => &mut folders.data,
            "--downloads" => &mut folders.downloads,
            _ if word.starts_with('-') => return Err(UsageError::UnknownOption(word.to_string())),
            _ => return Err(UsageError::Extra(word.to_string())),
        };
        if named.iter().any(|n| n == name) {
            return Err(UsageError::Twice(name.to_string()));
        }
        named.push(name.to_string());
        let value = inline.or_else(|| words.next().copied()).filter(|v| !v.is_empty() && !v.starts_with("--"));
        let Some(value) = value else {
            return Err(UsageError::NeedsFolder(name.to_string()));
        };
        *slot = Some(cwd.join(value));
    }
    if let Some(name) = named.first().filter(|_| !server) {
        return Err(UsageError::NeedsServer(name.clone()));
    }
    let seat = if server { Seat::Server } else { Seat::Desktop };
    Ok(Invocation::Run { seat, folders })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Invocation, UsageError> {
        parse(args, Path::new("/srv/pullbay"))
    }

    #[test]
    fn no_arguments_runs_beside_the_desktop() {
        assert_eq!(run(&[]).unwrap(), Invocation::Run { seat: Seat::Desktop, folders: Folders::default() });
    }

    #[test]
    fn server_mode_takes_its_folders() {
        assert_eq!(run(&["--server"]).unwrap(), Invocation::Run { seat: Seat::Server, folders: Folders::default() });
        let given = run(&["--config", "/config", "--server", "--data=state", "--downloads", "/downloads"]).unwrap();
        assert_eq!(
            given,
            Invocation::Run {
                seat: Seat::Server,
                folders: Folders {
                    config: Some("/config".into()),
                    data: Some("/srv/pullbay/state".into()),
                    downloads: Some("/downloads".into()),
                },
            }
        );
    }

    #[test]
    fn folders_need_server_mode_and_a_value() {
        assert_eq!(run(&["--data", "/x"]), Err(UsageError::NeedsServer("--data".into())));
        assert_eq!(run(&["--server", "--data"]), Err(UsageError::NeedsFolder("--data".into())));
        assert_eq!(run(&["--server", "--data="]), Err(UsageError::NeedsFolder("--data".into())));
        assert_eq!(run(&["--data", "--server"]), Err(UsageError::NeedsFolder("--data".into())));
        assert_eq!(run(&["--server", "--data", "/a", "--data=/b"]), Err(UsageError::Twice("--data".into())));
    }

    #[test]
    fn anything_else_is_refused() {
        assert_eq!(run(&["--listen", "0.0.0.0"]), Err(UsageError::UnknownOption("--listen".into())));
        assert_eq!(run(&["--server=yes"]), Err(UsageError::UnknownOption("--server=yes".into())));
        assert_eq!(run(&["server"]), Err(UsageError::Extra("server".into())));
        assert_eq!(run(&["--help"]).unwrap(), Invocation::Help);
        assert_eq!(run(&["--server", "--version"]).unwrap(), Invocation::Version);
    }

    #[test]
    fn the_elevated_move_keeps_its_arguments() {
        let moved = run(&[crate::elevate::MOVE_ARG, "C:\\a b\\f.part", "C:\\Program Files\\f"]).unwrap();
        assert_eq!(
            moved,
            Invocation::Move { from: PathBuf::from("C:\\a b\\f.part"), to: PathBuf::from("C:\\Program Files\\f") }
        );
    }
}
