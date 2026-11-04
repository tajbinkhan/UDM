//! IDM's command-line switches for `uidm-gui`:
//!
//! ```text
//! uidm-gui /d URL [/p folder] [/f file_name] [/q] [/h] [/n] [/a]
//! uidm-gui /s
//! ```
//!
//! `/d` downloads URL, `/p` and `/f` set its folder and file name, `/a` adds
//! it to the main queue without starting it, `/n` asks no questions, and `/q`
//! exits UIDM once it finishes. `/s` starts the main queue. `/h` (hang up the
//! dial-up connection) is accepted and does nothing. As in IDM, `/p`, `/f`,
//! `/q`, `/h`, `/n` and `/a` need `/d`; without it they are ignored.
//!
//! Each switch also has a `-x` and a long form (`--download`, `--path`,
//! `--file`, `--quit`, `--hang-up`, `--silent`, `--add`, `--start-queue`), and
//! the slash forms take either case.

use std::path::Path;

use crate::protocol::{CommandLine, Download};
use crate::settings::{Duplicate, FileExists};

/// Read `args` (without the program name). `Ok(None)` means no switch asks
/// for anything, so the window opens as usual. `cwd` resolves a relative
/// `/p` folder.
pub fn parse<I, S>(args: I, cwd: &Path) -> Result<Option<CommandLine>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut url = None;
    let mut folder = None;
    let mut filename = None;
    let (mut quit, mut silent, mut add, mut start_queue) = (false, false, false, false);

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let arg = arg.as_ref();
        let mut value = |what: &str| {
            args.next()
                .map(|v| v.as_ref().to_string())
                .filter(|v| !v.is_empty())
                .ok_or_else(|| format!("{arg} needs {what}"))
        };
        match switch(arg) {
            Some('d') => url = Some(value("an address")?),
            Some('p') => folder = Some(value("a folder")?),
            Some('f') => filename = Some(value("a file name")?),
            Some('q') => quit = true,
            Some('h') => {}
            Some('n') => silent = true,
            Some('a') => add = true,
            Some('s') => start_queue = true,
            Some('m') => {}
            _ => return Err(format!("Unknown command-line argument {arg}")),
        }
    }

    let download = match url {
        None => None,
        Some(url) => {
            if !is_download_url(&url) {
                return Err(format!("{url} is not an http, https or ftp address"));
            }
            let folder = folder.map(|f| {
                let f = Path::new(&f);
                if f.is_absolute() {
                    f.to_path_buf()
                } else {
                    cwd.join(f)
                }
            });
            if let Some(name) = &filename {
                if name.contains(['/', '\\']) || name == "." || name == ".." {
                    return Err(format!("{name} is not a file name; give the folder with /p"));
                }
            }
            Some(Download {
                url,
                folder: folder.map(|f| f.display().to_string()),
                filename,
                start: add.then_some(false),
                ask: silent.then_some(false),
                duplicate: silent.then_some(Duplicate::NewName),
                file_exists: silent.then_some(FileExists::Rename),
                ..Download::default()
            })
        }
    };
    let quit_after = quit && download.is_some();
    if download.is_none() && !start_queue {
        return Ok(None);
    }
    Ok(Some(CommandLine {
        download,
        start_queue,
        quit_after,
    }))
}

/// The letter of an IDM switch: `/d`, `/D`, `-d` and `--download` are all
/// `d`. `--minimized`, the autostart flag, is `m`.
fn switch(arg: &str) -> Option<char> {
    let long = match arg {
        "--download" => 'd',
        "--path" => 'p',
        "--file" => 'f',
        "--quit" => 'q',
        "--hang-up" => 'h',
        "--silent" => 'n',
        "--add" => 'a',
        "--start-queue" => 's',
        "--minimized" => 'm',
        _ => {
            let mut chars = arg.chars();
            let (Some('/' | '-'), Some(c), None) = (chars.next(), chars.next(), chars.next()) else {
                return None;
            };
            let c = c.to_ascii_lowercase();
            return "dpfqhnas".contains(c).then_some(c);
        }
    };
    Some(long)
}

fn is_download_url(s: &str) -> bool {
    let Ok(u) = url::Url::parse(s) else {
        return false;
    };
    matches!(u.scheme(), "http" | "https" | "ftp" | "ftps") && u.host_str().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd() -> std::path::PathBuf {
        std::env::temp_dir()
    }

    fn run(args: &[&str]) -> Result<Option<CommandLine>, String> {
        parse(args, &cwd())
    }

    #[test]
    fn no_switches_open_the_window_as_usual() {
        assert!(run(&[]).unwrap().is_none());
        assert!(run(&["--minimized"]).unwrap().is_none());
    }

    #[test]
    fn d_adds_a_download_with_its_folder_and_name() {
        let folder = cwd().join("dl");
        let c = run(&["/d", "https://example.com/a.zip", "/p", &folder.display().to_string(), "/f", "b.zip"])
            .unwrap()
            .unwrap();
        let d = c.download.unwrap();
        assert_eq!(d.url, "https://example.com/a.zip");
        assert_eq!(d.folder.as_deref(), Some(folder.display().to_string().as_str()));
        assert_eq!(d.filename.as_deref(), Some("b.zip"));
        assert_eq!(d.start, None, "it starts");
        assert_eq!(d.ask, None, "the setting decides on the dialog");
        assert!(!c.start_queue && !c.quit_after);
    }

    #[test]
    fn a_relative_folder_is_under_the_current_one() {
        let c = run(&["/d", "https://example.com/a", "/p", "sub"]).unwrap().unwrap();
        assert_eq!(c.download.unwrap().folder, Some(cwd().join("sub").display().to_string()));
    }

    #[test]
    fn n_asks_nothing_and_a_only_adds() {
        let d = run(&["/n", "/a", "/d", "http://example.com/a"]).unwrap().unwrap().download.unwrap();
        assert_eq!(d.ask, Some(false));
        assert_eq!(d.duplicate, Some(Duplicate::NewName));
        assert_eq!(d.file_exists, Some(FileExists::Rename));
        assert_eq!(d.start, Some(false));
    }

    #[test]
    fn switches_take_any_case_and_every_form() {
        for d in ["/d", "/D", "-d", "--download"] {
            let c = run(&[d, "ftp://example.com/a", "/Q", "--silent"]).unwrap().unwrap();
            assert!(c.quit_after);
            assert_eq!(c.download.unwrap().ask, Some(false));
        }
    }

    #[test]
    fn s_starts_the_queue_alone_or_with_a_download() {
        let c = run(&["/s"]).unwrap().unwrap();
        assert!(c.start_queue && c.download.is_none());
        let c = run(&["/d", "https://example.com/a", "/s"]).unwrap().unwrap();
        assert!(c.start_queue && c.download.is_some());
    }

    #[test]
    fn switches_that_need_d_are_ignored_without_it() {
        assert!(run(&["/q", "/n", "/a", "/h", "/f", "x"]).unwrap().is_none());
        assert!(!run(&["/q", "/s"]).unwrap().unwrap().quit_after);
    }

    #[test]
    fn h_is_accepted_and_does_nothing() {
        let c = run(&["/d", "https://example.com/a", "/h"]).unwrap().unwrap();
        assert!(!c.quit_after && !c.start_queue);
    }

    #[test]
    fn bad_input_is_refused() {
        assert!(run(&["/d"]).is_err(), "no address");
        assert!(run(&["/d", "file:///c:/x"]).is_err(), "not a web address");
        assert!(run(&["/d", "example.com/a"]).is_err(), "no scheme");
        assert!(run(&["/d", "https://example.com/a", "/f", "..\\x"]).is_err(), "a path as the name");
        assert!(run(&["/x"]).is_err(), "unknown switch");
        assert!(run(&["https://example.com/a"]).is_err(), "a bare address");
    }
}
