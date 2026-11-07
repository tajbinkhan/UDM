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
//!
//! `parse_sent` reads the JSON another program hands to `uidm-host send`:
//! one link with what IDM's `SendLinkToIDM2` takes, or a list of links with
//! one referer, as `SendLinksArray` takes them. The README shows its fields.

use std::path::Path;

use serde::Deserialize;

use uidm_domain::download::CommandLine;

use uidm_domain::download::{Download, DownloadLogin, PageLink};
use uidm_domain::settings::{Duplicate, FileExists};

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
        Some(url) => Some(download(url, folder, filename, silent, add, cwd, "/p")?),
    };
    let quit_after = quit && download.is_some();
    if download.is_none() && !start_queue {
        return Ok(None);
    }
    Ok(Some(CommandLine { download, start_queue, quit_after }))
}

/// A download of `url` into `folder` (relative to `cwd`) as `filename`.
/// `silent` asks nothing (IDM's `/n`) and `add` only adds it to the main
/// queue (`/a`). `folder_hint` names how the caller gives the folder, for
/// the error about a path given as the name.
fn download(
    url: String,
    folder: Option<String>,
    filename: Option<String>,
    silent: bool,
    add: bool,
    cwd: &Path,
    folder_hint: &str,
) -> Result<Download, String> {
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
            return Err(format!("{name} is not a file name; give the folder with {folder_hint}"));
        }
    }
    Ok(Download {
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

/// What another program sends with `uidm-host send`: one link to add, or
/// links for the link picker.
#[derive(Debug)]
// It lives only while it is sent, as `protocol::Incoming` does.
#[allow(clippy::large_enum_variant)]
pub enum Sent {
    /// One link, added as the browser extension adds one.
    One(Download),
    /// Links to list in the link picker, all from the page `referer`, which
    /// is empty when none was given.
    Many { referer: String, links: Vec<PageLink> },
}

/// IDM's `SendLinkToIDM` flag that adds a link without asking anything.
const FLAG_SILENT: u32 = 1;
/// IDM's `SendLinkToIDM` flag that adds a link to the queue only.
const FLAG_ADD_ONLY: u32 = 2;

/// One link, with what IDM's `SendLinkToIDM2` takes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OneLink {
    url: String,
    #[serde(default, alias = "referrer")]
    referer: Option<String>,
    #[serde(default)]
    cookies: Option<String>,
    /// A form body, sent with a POST.
    #[serde(default)]
    post_data: Option<String>,
    /// The form body's type; a form's usual one when missing.
    #[serde(default)]
    content_type: Option<String>,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    user_agent: Option<String>,
    /// Add it with no dialog.
    #[serde(default)]
    silent: bool,
    /// Add it to the main queue without starting it.
    #[serde(default)]
    add_only: bool,
    /// IDM's flags: 1 is `silent`, 2 is `add_only`.
    #[serde(default)]
    flags: u32,
}

/// Links for the picker, with what IDM's `SendLinksArray` takes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkList {
    #[serde(default, alias = "referrer")]
    referer: Option<String>,
    links: Vec<ListedLink>,
}

/// A link in a list: its address alone, or an object.
#[derive(Deserialize)]
#[serde(untagged)]
enum ListedLink {
    Url(String),
    Link(LinkObject),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkObject {
    url: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    cookies: Option<String>,
}

/// The form body's type when the sender gives none.
const FORM_TYPE: &str = "application/x-www-form-urlencoded";

/// Read what a program sent to `uidm-host send`: a JSON object with `url`
/// for one link, or with `links` for the picker. `cwd` resolves a relative
/// folder. An empty string counts as a missing field, as IDM's COM calls
/// pass empty strings for what they leave out.
pub fn parse_sent(json: &str, cwd: &Path) -> Result<Sent, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("not valid JSON: {e}"))?;
    if !value.is_object() {
        return Err("expected a JSON object with \"url\" or \"links\"".into());
    }
    if value.get("links").is_some() {
        let list: LinkList = serde_json::from_value(value).map_err(|e| format!("bad link list: {e}"))?;
        return many(list);
    }
    let one: OneLink = serde_json::from_value(value).map_err(|e| format!("bad link: {e}"))?;
    one_link(one, cwd)
}

fn given(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty())
}

fn one_link(l: OneLink, cwd: &Path) -> Result<Sent, String> {
    let silent = l.silent || l.flags & FLAG_SILENT != 0;
    let add = l.add_only || l.flags & FLAG_ADD_ONLY != 0;
    let mut d = download(l.url.trim().to_string(), given(l.folder), given(l.filename), silent, add, cwd, "\"folder\"")?;
    d.referrer = given(l.referer);
    d.cookies = given(l.cookies);
    d.user_agent = given(l.user_agent);
    if let Some(body) = l.post_data {
        use base64::Engine as _;
        d.method = Some("POST".into());
        d.body = Some(base64::engine::general_purpose::STANDARD.encode(body.as_bytes()));
        let kind = given(l.content_type).unwrap_or_else(|| FORM_TYPE.to_string());
        d.headers.insert("Content-Type".into(), kind);
    }
    match (given(l.user), l.password) {
        (Some(username), password) => {
            d.login = Some(DownloadLogin { username, password: Some(password.unwrap_or_default()) })
        }
        (None, Some(p)) if !p.is_empty() => return Err("a password needs a user".into()),
        (None, _) => {}
    }
    Ok(Sent::One(d))
}

fn many(list: LinkList) -> Result<Sent, String> {
    if list.links.is_empty() {
        return Err("the link list is empty".into());
    }
    let links = list
        .links
        .into_iter()
        .map(|l| {
            let link = match l {
                ListedLink::Url(url) => PageLink { url, ..PageLink::default() },
                ListedLink::Link(o) => PageLink { url: o.url, text: o.text, cookies: given(o.cookies), image: false },
            };
            let url = link.url.trim().to_string();
            if !uidm_domain::batch::is_web_url(&url) || url::Url::parse(&url).is_err() {
                return Err(format!("{url} is not an http or https address"));
            }
            Ok(PageLink { url, ..link })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if links.len() > uidm_domain::batch::MAX_BATCH {
        return Err(format!("too many links: {}, at most {}", links.len(), uidm_domain::batch::MAX_BATCH));
    }
    Ok(Sent::Many { referer: given(list.referer).unwrap_or_default(), links })
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

    fn sent(json: &str) -> Result<Sent, String> {
        parse_sent(json, &cwd())
    }

    fn one(json: &str) -> Download {
        match sent(json).unwrap() {
            Sent::One(d) => d,
            other => panic!("expected one link, got {other:?}"),
        }
    }

    #[test]
    fn one_link_carries_everything_send_link_to_idm_takes() {
        use base64::Engine as _;
        let folder = cwd().join("dl");
        let json = serde_json::json!({
            "url": "https://example.com/get",
            "referer": "https://example.com/page",
            "cookies": "a=1; b=2",
            "post_data": "id=7&x=y",
            "user": "me",
            "password": "secret",
            "folder": folder.display().to_string(),
            "filename": "file.zip",
            "user_agent": "Tool/1.0",
        });
        let d = one(&json.to_string());
        assert_eq!(d.url, "https://example.com/get");
        assert_eq!(d.referrer.as_deref(), Some("https://example.com/page"));
        assert_eq!(d.cookies.as_deref(), Some("a=1; b=2"));
        assert_eq!(d.method.as_deref(), Some("POST"));
        let body = base64::engine::general_purpose::STANDARD.decode(d.body.unwrap()).unwrap();
        assert_eq!(body, b"id=7&x=y");
        assert_eq!(d.headers.get("Content-Type").map(String::as_str), Some(FORM_TYPE));
        let login = d.login.unwrap();
        assert_eq!((login.username.as_str(), login.password.as_deref()), ("me", Some("secret")));
        assert_eq!(d.folder.as_deref(), Some(folder.display().to_string().as_str()));
        assert_eq!(d.filename.as_deref(), Some("file.zip"));
        assert_eq!(d.user_agent.as_deref(), Some("Tool/1.0"));
        // No flags: the settings decide on the dialog, and it starts.
        assert_eq!((d.ask, d.start), (None, None));
    }

    #[test]
    fn a_bare_link_is_a_get_with_nothing_else() {
        let d = one(r#"{"url":"https://example.com/a.zip","referrer":"","cookies":"","user":"","folder":""}"#);
        assert_eq!(d.method, None);
        assert!(d.headers.is_empty());
        assert_eq!((d.referrer, d.cookies, d.login.is_none(), d.folder), (None, None, true, None));
    }

    #[test]
    fn the_flags_skip_the_dialog_or_only_add() {
        for json in [r#"{"url":"https://e.com/a","silent":true}"#, r#"{"url":"https://e.com/a","flags":1}"#] {
            let d = one(json);
            assert_eq!(d.ask, Some(false));
            assert_eq!(d.duplicate, Some(Duplicate::NewName));
            assert_eq!(d.file_exists, Some(FileExists::Rename));
            assert_eq!(d.start, None);
        }
        for json in [r#"{"url":"https://e.com/a","add_only":true}"#, r#"{"url":"https://e.com/a","flags":2}"#] {
            let d = one(json);
            assert_eq!((d.start, d.ask), (Some(false), None));
        }
        let d = one(r#"{"url":"https://e.com/a","flags":3}"#);
        assert_eq!((d.start, d.ask), (Some(false), Some(false)));
    }

    #[test]
    fn a_post_may_name_its_body_type() {
        let d = one(r#"{"url":"https://e.com/a","post_data":"{}","content_type":"application/json"}"#);
        assert_eq!(d.headers.get("Content-Type").map(String::as_str), Some("application/json"));
        // An empty body is still a POST.
        assert_eq!(one(r#"{"url":"https://e.com/a","post_data":""}"#).method.as_deref(), Some("POST"));
    }

    #[test]
    fn a_relative_folder_is_under_the_senders_folder() {
        assert_eq!(
            one(r#"{"url":"https://e.com/a","folder":"sub"}"#).folder,
            Some(cwd().join("sub").display().to_string())
        );
    }

    #[test]
    fn a_list_opens_the_picker_with_one_referer() {
        let json = r#"{"referer":"https://e.com/page","links":[
            "https://e.com/a.zip",
            {"url":"https://e.com/b.zip","text":"B","cookies":"c=1"}
        ]}"#;
        let Sent::Many { referer, links } = sent(json).unwrap() else { panic!("expected a list") };
        assert_eq!(referer, "https://e.com/page");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0], PageLink { url: "https://e.com/a.zip".into(), ..PageLink::default() });
        assert_eq!(
            links[1],
            PageLink { url: "https://e.com/b.zip".into(), text: "B".into(), cookies: Some("c=1".into()), image: false }
        );
        // The referer may be left out.
        let Sent::Many { referer, .. } = sent(r#"{"links":["http://e.com/a"]}"#).unwrap() else { panic!() };
        assert_eq!(referer, "");
    }

    #[test]
    fn bad_sends_are_refused() {
        assert!(sent("not json").is_err());
        assert!(sent(r#"["https://e.com/a"]"#).is_err(), "not an object");
        assert!(sent(r#"{"referer":"https://e.com/"}"#).is_err(), "no url");
        assert!(sent(r#"{"url":"file:///c:/x"}"#).is_err(), "not a web address");
        assert!(sent(r#"{"url":"https://e.com/a","filename":"..\\x"}"#).is_err(), "a path as the name");
        assert!(sent(r#"{"url":"https://e.com/a","password":"p"}"#).is_err(), "a password with no user");
        assert!(sent(r#"{"url":"https://e.com/a","refferer":"x"}"#).is_err(), "a misspelled field");
        assert!(sent(r#"{"links":[]}"#).is_err(), "an empty list");
        assert!(sent(r#"{"links":["ftp://e.com/a"]}"#).is_err(), "the picker takes web links only");
        assert!(sent(r#"{"links":[{"url":"https://e.com/a","size":1}]}"#).is_err(), "an unknown link field");
        assert!(sent(r#"{"links":["https://e.com/a"],"silent":true}"#).is_err(), "flags are for one link");
    }
}
