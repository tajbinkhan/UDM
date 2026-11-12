//! The site grabber's model and rules: what to crawl and keep (a
//! `GrabRequest`), which addresses pass its filters, where mirrored pages
//! go, which found file becomes which download, and the saved projects with
//! their schedules. Crawling, reading pages and keeping projects on disk is
//! `udm_infrastructure::grab`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::bytes::ByteCount;
use serde::{Deserialize, Serialize};
use url::Url;

/// What to crawl and how far.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrabRequest {
    pub start: String,
    /// How many links deep to follow from the start page; 0 reads only it.
    pub depth: u32,
    /// Follow links only on the start page's host.
    pub same_site: bool,
    /// File types to keep, such as `pdf`.
    pub types: Vec<String>,
    /// Stop after reading this many pages.
    pub max_pages: u64,
    /// Stop after reading this many bytes of pages.
    pub max_bytes: ByteCount,
    /// Wait at least this long between requests, in milliseconds.
    pub delay_ms: u64,
    /// Explore only pages whose address matches one of these (see
    /// `pattern_matches`); empty explores every page in reach.
    #[serde(default)]
    pub explore_only: Vec<String>,
    /// Never explore pages whose address matches one of these.
    #[serde(default)]
    pub explore_skip: Vec<String>,
    /// Keep only files whose address matches one of these.
    #[serde(default)]
    pub files_only: Vec<String>,
    /// Never keep files whose address matches one of these.
    #[serde(default)]
    pub files_skip: Vec<String>,
    /// With `same_site`, also explore the start domain's subdomains.
    #[serde(default)]
    pub subdomains: bool,
    /// Do not explore above the start page's folder on its site.
    #[serde(default)]
    pub no_parent: bool,
    /// Keep only files of at least this many bytes. Sizes are asked with a
    /// request per file; a file whose size is not known is kept.
    #[serde(default)]
    pub min_size: Option<ByteCount>,
    #[serde(default)]
    pub max_size: Option<ByteCount>,
    /// List a file once when the same name and size turn up at another
    /// address.
    #[serde(default)]
    pub hide_duplicates: bool,
    /// Send the browser's cookies for the start site with its pages, so
    /// pages behind a sign-in form can be read. The browser extension
    /// supplies them when the crawl starts.
    #[serde(default)]
    pub browser_cookies: bool,
    /// Save for offline browsing: files under folders that mirror their
    /// addresses, and maybe the pages too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror: Option<Mirror>,
    /// Read each page as the browser shows it, after its scripts ran, so
    /// links that scripts add are found too. Slower: the browser extension
    /// opens each page in a background tab.
    #[serde(default)]
    pub render_scripts: bool,
    /// When pages are rendered, leave out the pages their scripts open in
    /// new windows; otherwise those are links to follow too.
    #[serde(default = "yes")]
    pub ignore_popups: bool,
    /// How many pages are read at once, 1 to 10.
    #[serde(default = "one")]
    pub explore_at_once: u32,
    /// How many of the found files the queue they go to downloads at once,
    /// 1 to 10.
    #[serde(default = "three")]
    pub download_at_once: u32,
    /// Keep each file's link text, and give it to the download as its
    /// description.
    #[serde(default)]
    pub link_text: bool,
    /// File types never kept, even when `types` takes them (as `*` does).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip_types: Vec<String>,
    /// Keep only files on the start site (and, with `subdomains`, its
    /// subdomains).
    #[serde(default)]
    pub files_this_site: bool,
    /// Where the found files are saved, when the crawl keeps no mirror.
    #[serde(default)]
    pub save_to: SaveTo,
    /// Add each file to this queue as soon as it is found, and start the
    /// queue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_found: Option<crate::id::QueueId>,
    /// Follow links on the start site however deep they go; the page and
    /// size budgets still stop the crawl.
    #[serde(default)]
    pub whole_site: bool,
    /// Without `same_site`, how many links in a row to follow on other
    /// sites. `None` counts them in `depth` with the rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_depth: Option<u32>,
    /// The user name for the start site, when the project keeps its own
    /// login; the password is in Credential Manager (`project_login_target`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_user: Option<String>,
    /// The site's sign-in page. UDM has no browser of its own, so the
    /// user opens this in their browser and signs in there; when the crawl
    /// starts, the browser extension hands over that site's cookies, which
    /// go to that site's pages only (`on_login_site`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_url: Option<String>,
    /// Cookies for the sign-in page's site, pasted by the user, used when
    /// the browser has none to give (no extension, or another browser).
    /// Project files keep them sealed for this user (see
    /// `udm_infrastructure::grab::save_project`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_cookies: Option<String>,
    /// Without a mirror, save each file under subfolders that follow its
    /// address below the start page's folder (`relative_subfolder`).
    #[serde(default)]
    pub subfolders: bool,
    /// Replace files already there, in a mirror or not; otherwise the
    /// mirror skips them and other saves follow Options.
    #[serde(default)]
    pub overwrite: bool,
}

fn yes() -> bool {
    true
}

fn one() -> u32 {
    1
}

fn three() -> u32 {
    3
}

/// Where a crawl's files are saved, when it keeps no mirror.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "to", content = "name")]
pub enum SaveTo {
    /// Each file to the folder of its own category.
    #[default]
    Categories,
    /// Every file to the folder of the category with this name.
    Category(String),
    /// Every file to this folder, a full path, without subfolders.
    Folder(String),
}

/// The Credential Manager entry for the grabber login `user` on the site
/// of `start`, so two projects on one site keep their own accounts.
pub fn project_login_target(start: &str, user: &str) -> String {
    format!("{}/{}", host_login_target(start), user.trim())
}

/// The entry older versions kept one grabber password per site under.
pub fn host_login_target(start: &str) -> String {
    let host =
        Url::parse(start.trim()).ok().and_then(|u| u.host_str().map(str::to_ascii_lowercase)).unwrap_or_default();
    format!("UDM/grabber/{host}")
}

/// Where and how a crawl is saved for offline browsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mirror {
    /// The folder the site's folders are made under, a full path.
    pub folder: String,
    /// Save the pages read too, with their links to saved files and pages
    /// changed to the local copies.
    #[serde(default)]
    pub pages: bool,
    /// Replace files already there; otherwise they are skipped.
    #[serde(default)]
    pub overwrite: bool,
}

/// Pages a crawl should never open, so it does not sign itself out.
pub const LOGOUT_PATTERNS: [&str; 5] = ["*logout*", "*logoff*", "*log-out*", "*signout*", "*sign-out*"];

pub const DEPTH_RANGE: (u32, u32) = (0, 10);

/// Pages read at once, and downloads run at once.
pub const AT_ONCE_RANGE: (u32, u32) = (1, 10);

pub const MAX_PAGES_RANGE: (u64, u64) = (1, 10_000);

pub const MAX_MB_RANGE: (u64, u64) = (1, 2_000);

pub const DELAY_MS_RANGE: (u64, u64) = (200, 60_000);

/// Most bytes read from one page.
pub const MAX_PAGE_BYTES: ByteCount = ByteCount::mib(5);

/// The name the crawler gives in `robots.txt` matching.
pub const USER_AGENT: &str = "UDM";

impl Default for GrabRequest {
    fn default() -> Self {
        GrabRequest {
            start: String::new(),
            depth: 2,
            same_site: true,
            types: vec!["pdf".into()],
            max_pages: 200,
            max_bytes: ByteCount::mib(50),
            delay_ms: 1000,
            explore_only: Vec::new(),
            explore_skip: LOGOUT_PATTERNS.iter().map(|p| p.to_string()).collect(),
            files_only: Vec::new(),
            files_skip: Vec::new(),
            subdomains: false,
            no_parent: false,
            min_size: None,
            max_size: None,
            hide_duplicates: true,
            browser_cookies: false,
            mirror: None,
            render_scripts: false,
            ignore_popups: true,
            explore_at_once: 1,
            download_at_once: 3,
            link_text: false,
            skip_types: Vec::new(),
            files_this_site: false,
            save_to: SaveTo::Categories,
            download_found: None,
            whole_site: false,
            other_depth: None,
            login_user: None,
            login_url: None,
            login_cookies: None,
            subfolders: false,
            overwrite: false,
        }
    }
}

/// Why a grabber request cannot run. The window shows these, translated.
///
/// The variant names are part of the wire format (the daemon's
/// `error_kind`): renaming one changes what clients read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum InvalidGrab {
    #[error("The start page must be a full web address.")]
    StartNotAddress,
    #[error("The start page must start with http:// or https://.")]
    StartNotWeb,
    #[error("The depth must be between {lo} and {hi}.", lo = DEPTH_RANGE.0, hi = DEPTH_RANGE.1)]
    Depth,
    #[error("Files at once must be between {lo} and {hi}.", lo = AT_ONCE_RANGE.0, hi = AT_ONCE_RANGE.1)]
    AtOnce,
    #[error("Choose the category to save the files in.")]
    NoCategory,
    #[error("The folder to save the files in must be a full path: {0}")]
    FolderNotFull(String),
    #[error("The page budget must be between {lo} and {hi}.", lo = MAX_PAGES_RANGE.0, hi = MAX_PAGES_RANGE.1)]
    Pages,
    #[error("The size budget must be between {lo} and {hi} MB.", lo = MAX_MB_RANGE.0, hi = MAX_MB_RANGE.1)]
    Size,
    #[error("The wait between pages must be between {lo} and {hi} ms.", lo = DELAY_MS_RANGE.0, hi = DELAY_MS_RANGE.1)]
    Delay,
    #[error("Choose at least one file type to keep.")]
    NoTypes,
    #[error("The login page must be a full web address.")]
    LoginNotAddress,
    #[error("The login page must start with http:// or https://.")]
    LoginNotWeb,
    #[error("The folder to keep the site in must be a full path: {0}")]
    MirrorNotFull(String),
    #[error("The smallest file size is larger than the largest.")]
    SizesReversed,
}

impl GrabRequest {
    pub fn validate(&self) -> Result<(), InvalidGrab> {
        let start = Url::parse(self.start.trim()).map_err(|_| InvalidGrab::StartNotAddress)?;
        if !matches!(start.scheme(), "http" | "https") {
            return Err(InvalidGrab::StartNotWeb);
        }
        let in_range = |v: u64, (lo, hi): (u64, u64)| (lo..=hi).contains(&v);
        let depth_range = (DEPTH_RANGE.0 as u64, DEPTH_RANGE.1 as u64);
        if !in_range(self.depth as u64, depth_range)
            || self.other_depth.is_some_and(|d| !in_range(d as u64, depth_range))
        {
            return Err(InvalidGrab::Depth);
        }
        let at_once = (AT_ONCE_RANGE.0 as u64, AT_ONCE_RANGE.1 as u64);
        if !in_range(self.explore_at_once as u64, at_once) || !in_range(self.download_at_once as u64, at_once) {
            return Err(InvalidGrab::AtOnce);
        }
        match &self.save_to {
            SaveTo::Category(name) if name.trim().is_empty() => return Err(InvalidGrab::NoCategory),
            SaveTo::Folder(folder) if !Path::new(folder.trim()).is_absolute() => {
                return Err(InvalidGrab::FolderNotFull(folder.clone()));
            }
            _ => {}
        }
        if !in_range(self.max_pages, MAX_PAGES_RANGE) {
            return Err(InvalidGrab::Pages);
        }
        let mb = self.max_bytes.whole_mib();
        if !in_range(mb, MAX_MB_RANGE) {
            return Err(InvalidGrab::Size);
        }
        if !in_range(self.delay_ms, DELAY_MS_RANGE) {
            return Err(InvalidGrab::Delay);
        }
        if self.types.is_empty() {
            return Err(InvalidGrab::NoTypes);
        }
        if let Some(login) = &self.login_url {
            let page = Url::parse(login.trim()).map_err(|_| InvalidGrab::LoginNotAddress)?;
            if !matches!(page.scheme(), "http" | "https") {
                return Err(InvalidGrab::LoginNotWeb);
            }
        }
        if let Some(m) = self.mirror.as_ref().filter(|m| !Path::new(m.folder.trim()).is_absolute()) {
            return Err(InvalidGrab::MirrorNotFull(m.folder.clone()));
        }
        if let (Some(lo), Some(hi)) = (self.min_size, self.max_size) {
            if lo > hi {
                return Err(InvalidGrab::SizesReversed);
            }
        }
        Ok(())
    }
}

/// Whether `url` matches `pattern`, ignoring case: `*` matches any run of
/// characters, a pattern without a scheme also matches without one, and a
/// pattern without `*` matches anywhere in the address (`/docs/` keeps
/// every address with /docs/ in it).
pub fn pattern_matches(pattern: &str, url: &str) -> bool {
    let p = pattern.trim().to_ascii_lowercase();
    let u = url.trim().to_ascii_lowercase();
    if p.is_empty() {
        return false;
    }
    if !p.contains('*') {
        return u.contains(&p);
    }
    let bare = u.split_once("://").map_or(u.as_str(), |(_, rest)| rest);
    glob(&p, &u) || (!p.contains("://") && glob(&p, bare))
}

/// `*` matches any run of characters; the rest must match exactly.
fn glob(pattern: &str, text: &str) -> bool {
    let pieces: Vec<&str> = pattern.split('*').collect();
    let Some(mut rest) = text.strip_prefix(pieces[0]) else {
        return false;
    };
    let last = pieces.len() - 1;
    for (i, piece) in pieces.iter().enumerate().skip(1) {
        if i == last {
            return rest.ends_with(piece);
        }
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    rest.is_empty()
}

/// One path part made safe for a Windows file name.
fn safe_part(part: &str) -> String {
    let decoded = percent_encoding::percent_decode_str(part).decode_utf8_lossy();
    let clean: String =
        decoded.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    let clean = clean.trim_end_matches(['.', ' ']).to_string();
    if clean.is_empty() || clean == "." || clean == ".." {
        "_".to_string()
    } else {
        crate::filename::not_a_device(clean)
    }
}

/// Where `url` is kept under the mirror folder `root`: the host, then its
/// path. A folder address gets `index.html`; a page gets `.html` added when
/// its name does not end in it, so a browser opens it as a page; a query
/// adds a short hash to the name, so `?page=2` does not overwrite `?page=1`.
pub fn mirror_path(root: &Path, url: &Url, page: bool) -> PathBuf {
    let mut path = root.join(safe_part(url.host_str().unwrap_or("site")));
    let segments: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
    let (dirs, last) = match segments.split_last() {
        Some((last, dirs)) => (dirs, *last),
        None => (&[][..], ""),
    };
    for d in dirs.iter().filter(|d| !d.is_empty()) {
        path.push(safe_part(d));
    }
    let mut name = if last.is_empty() { "index.html".to_string() } else { safe_part(last) };
    if let Some(q) = url.query() {
        let mut hash: u32 = 0x811c_9dc5;
        for b in q.bytes() {
            hash = (hash ^ b as u32).wrapping_mul(0x0100_0193);
        }
        name = match name.rsplit_once('.') {
            Some((stem, ext)) => format!("{stem}_{hash:08x}.{ext}"),
            None => format!("{name}_{hash:08x}"),
        };
    }
    let lower = name.to_ascii_lowercase();
    if page && !lower.ends_with(".html") && !lower.ends_with(".htm") {
        name.push_str(".html");
    }
    path.join(name)
}

/// The path from the file `from` to the file `to`, with `/`, for a link.
pub fn relative_link(from: &Path, to: &Path) -> String {
    let from: Vec<_> = from.parent().map(|p| p.components().collect()).unwrap_or_default();
    let to: Vec<_> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_string(); from.len() - common];
    parts.extend(to[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    let link = parts.join("/");
    // Spaces and the like as a browser expects them in a link.
    link.replace('%', "%25").replace(' ', "%20").replace('#', "%23")
}

/// One entry of a type list or filter mask, as `GrabRequest::types` keeps
/// it: lower case; `*` or `*.*` for every type; a bare extension (`jpg`
/// from `jpg`, `.jpg` or `*.jpg`), which is how older projects saved
/// their lists; or else a mask on the whole file name, such as
/// `photo_*.jpg` or `*.tar.gz`. A type of several parts without wildcards
/// (`tar.gz`, `.tar.gz`) becomes the mask `*.tar.gz`, since no file is
/// named just that. Empty for an empty entry.
pub fn mask_entry(part: &str) -> String {
    let t = part.trim().to_lowercase();
    if t == "*" || t == "*.*" {
        return "*".to_string();
    }
    let rest = t.strip_prefix("*.").or_else(|| t.strip_prefix('.')).unwrap_or(&t);
    if !rest.is_empty() && !rest.contains(['*', '?']) {
        return if rest.contains('.') { format!("*.{rest}") } else { rest.to_string() };
    }
    t
}

/// `field` (a type list as typed) with `masks` added after what it holds,
/// leaving out those it already has, so picking a filter adds to the list
/// as IDM's does rather than replacing it.
pub fn add_masks(field: &str, masks: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    for part in field.split([',', ';']).flat_map(str::split_whitespace).chain(masks.iter().map(String::as_str)) {
        let m = mask_entry(part);
        if !m.is_empty() && !out.contains(&m) {
            out.push(m);
        }
    }
    out.join(", ")
}

/// Whether file name `name` matches `mask`, ignoring case: `*` matches any
/// run of characters and `?` any one; the rest must match exactly.
pub fn name_matches(mask: &str, name: &str) -> bool {
    let p: Vec<char> = mask.to_lowercase().chars().collect();
    let t: Vec<char> = name.to_lowercase().chars().collect();
    // Classic backtracking: on a mismatch, let the last `*` take one more
    // character.
    let (mut i, mut j) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while j < t.len() {
        if i < p.len() && (p[i] == '?' || p[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some((i, j));
            i += 1;
        } else if let Some((si, sj)) = star {
            i = si + 1;
            j = sj + 1;
            star = Some((si, sj + 1));
        } else {
            return false;
        }
    }
    p[i..].iter().all(|c| *c == '*')
}

/// The file types a request keeps and skips (see `mask_entry`): bare
/// extensions, where `*` keeps every type that is not a page, and masks on
/// the whole file name.
pub struct Types {
    keep: HashSet<String>,
    skip: HashSet<String>,
    keep_names: Vec<String>,
    skip_names: Vec<String>,
}

impl Types {
    pub fn of(req: &GrabRequest) -> Types {
        let split = |list: &[String]| -> (HashSet<String>, Vec<String>) {
            let mut exts = HashSet::new();
            let mut names = Vec::new();
            for m in list.iter().map(|t| mask_entry(t)).filter(|t| !t.is_empty()) {
                if m == "*" || !m.contains(['*', '?', '.']) {
                    exts.insert(m);
                } else {
                    names.push(m);
                }
            }
            (exts, names)
        };
        let (keep, keep_names) = split(&req.types);
        let (skip, skip_names) = split(&req.skip_types);
        Types { keep, skip, keep_names, skip_names }
    }

    /// Whether a file named `name` with extension `ext` is kept.
    pub fn keeps(&self, name: &str, ext: &str) -> bool {
        !self.skip.contains(ext)
            && !self.skip_names.iter().any(|m| name_matches(m, name))
            && (self.named(name, ext) || (self.keep.contains("*") && !PAGE_EXTENSIONS.contains(&ext)))
    }

    /// Whether the keep list names this file itself, by its extension or a
    /// mask on its name (not just through `*`), as a page must be to be
    /// kept as a file.
    pub fn named(&self, name: &str, ext: &str) -> bool {
        self.keep.contains(ext) || self.keep_names.iter().any(|m| name_matches(m, name))
    }
}

/// The last part of `url`'s path, decoded: the file's name.
pub fn url_file_name(url: &Url) -> String {
    url.path_segments()
        .and_then(|mut s| s.next_back())
        .map(|n| percent_encoding::percent_decode_str(n).decode_utf8_lossy().into_owned())
        .unwrap_or_default()
}

/// Whether a crawl that saves pages keeps `url` locally, and if so whether
/// as a page: pages on the start site, and files of the kept types.
pub fn kept_locally(req: &GrabRequest, url: &Url) -> Option<bool> {
    let ext = url_extension(url);
    if ext.as_ref().is_some_and(|e| Types::of(req).keeps(&url_file_name(url), e)) {
        return Some(false);
    }
    let page_like = ext.is_none_or(|e| PAGE_EXTENSIONS.contains(&e.as_str()));
    (page_like && on_start_site(req, url.as_str())).then_some(true)
}

/// Whether a page at `url` belongs to the start site of `req`, which is
/// what browser cookies are sent to: the start host, and with `subdomains`
/// the start domain's other hosts.
pub fn on_start_site(req: &GrabRequest, url: &str) -> bool {
    on_site(&req.start, req.subdomains, url)
}

/// Whether a page at `url` belongs to the site of `req`'s login page, which
/// is what that page's cookies are sent to: its host, and with `subdomains`
/// its domain's other hosts. False without a login page.
pub fn on_login_site(req: &GrabRequest, url: &str) -> bool {
    req.login_url.as_deref().is_some_and(|login| on_site(login, req.subdomains, url))
}

/// Whether `url` is on the host of `site`, or with `subdomains` on its
/// domain (without `www.`) or one of the domain's subdomains.
fn on_site(site: &str, subdomains: bool, url: &str) -> bool {
    let host = |u: &str| Url::parse(u.trim()).ok().and_then(|u| u.host_str().map(str::to_ascii_lowercase));
    let (Some(start), Some(here)) = (host(site), host(url)) else {
        return false;
    };
    let domain = start.strip_prefix("www.").unwrap_or(&start);
    here == start || (subdomains && (here == domain || here.ends_with(&format!(".{domain}"))))
}

/// The `Cookie` header a crawl for `req` sends with a request to `url`:
/// `start` (the browser's cookies for the start site) on the start site,
/// and `login` (the login page site's cookies) on that site, joined when
/// both apply. `None` when neither does, so no other host gets them.
pub fn crawl_cookies(req: &GrabRequest, url: &str, start: Option<&str>, login: Option<&str>) -> Option<String> {
    let parts: Vec<&str> = [start.filter(|_| on_start_site(req, url)), login.filter(|_| on_login_site(req, url))]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// Pasted cookies as a `Cookie` header: a leading `Cookie:` and line
/// breaks (as copied from a browser's developer tools) are dropped.
pub fn clean_cookies(text: &str) -> Option<String> {
    let text = text.trim();
    let text = match text.get(..7) {
        Some(head) if head.eq_ignore_ascii_case("cookie:") => &text[7..],
        _ => text,
    };
    let parts: Vec<&str> = text.split([';', '\n', '\r']).map(str::trim).filter(|p| !p.is_empty()).collect();
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// The folders, under where it is saved, that keep a file at `url` in
/// the place it has on its site: the folders below the start page's
/// folder when it is under it on the same host, otherwise its host then
/// every folder of its address. Empty for a file in the start folder.
pub fn relative_subfolder(start: &str, url: &Url) -> PathBuf {
    let dirs = |u: &Url| -> Vec<String> {
        let mut parts: Vec<String> = u.path_segments().map(|s| s.map(str::to_string).collect()).unwrap_or_default();
        parts.pop();
        parts.retain(|p| !p.is_empty());
        parts
    };
    let mut path = PathBuf::new();
    let start = Url::parse(start.trim()).ok();
    match start.filter(|s| s.host_str() == url.host_str()) {
        Some(start) if dirs(url).starts_with(&dirs(&start)) => {
            for d in &dirs(url)[dirs(&start).len()..] {
                path.push(safe_part(d));
            }
        }
        _ => {
            path.push(safe_part(url.host_str().unwrap_or("site")));
            for d in dirs(url) {
                path.push(safe_part(&d));
            }
        }
    }
    path
}

/// Whether `url` is on this computer or the local network: `localhost`, or
/// a loopback, private, link-local or unique-local address.
pub fn local_host(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(d)) => {
            let d = d.to_ascii_lowercase();
            d == "localhost" || d.ends_with(".localhost") || d.ends_with(".local")
        }
        Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified(),
        Some(url::Host::Ipv6(ip)) => {
            let first = ip.segments()[0];
            ip.is_loopback()
                || ip.is_unspecified()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || ip.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback() || v4.is_private() || v4.is_link_local())
        }
        None => false,
    }
}

/// Whether `url` passes an include list (empty passes all) and an exclude
/// list.
pub fn passes(url: &str, only: &[String], skip: &[String]) -> bool {
    (only.is_empty() || only.iter().any(|p| pattern_matches(p, url))) && !skip.iter().any(|p| pattern_matches(p, url))
}

/// A file the crawl found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrabFile {
    pub url: String,
    /// Its file name, from the address.
    pub name: String,
    /// The page that links to it.
    pub page: String,
    /// Its size, when it was asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<ByteCount>,
    /// The text of the link to it, when the crawl keeps link texts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// A fetched page.
pub struct Page {
    /// The address after redirects; relative links resolve against it.
    pub url: String,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// A page as the browser showed it after its scripts ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub html: String,
    /// Its address then.
    pub url: String,
    /// The addresses its scripts opened in new windows.
    pub popups: Vec<String>,
}

/// The lower-case extension of a URL's last path segment.
pub fn url_extension(url: &Url) -> Option<String> {
    let last = url.path_segments()?.next_back()?;
    let (_, ext) = last.rsplit_once('.')?;
    (!ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// Extensions of pages worth reading for more links; a link with no
/// extension (`/docs/`) is read too.
pub const PAGE_EXTENSIONS: &[&str] = &["html", "htm", "xhtml", "php", "asp", "aspx", "jsp", "shtml", "cgi"];

/// A saved grabber project: its settings, and what its last crawl found
/// with the boxes ticked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub request: GrabRequest,
    #[serde(default)]
    pub files: Vec<GrabFile>,
    /// Ticked files, one flag per entry of `files`.
    #[serde(default)]
    pub checked: Vec<bool>,
    /// When the daemon runs it by itself: once, or on chosen days, with an
    /// optional stop time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<crate::queue::Schedule>,
    /// What a scheduled run does.
    #[serde(default)]
    pub run: RunAction,
    /// Files from older project files said `ExploreAndDownload` this way;
    /// `Project::load` moves it into `run`.
    #[serde(default, skip_serializing)]
    pub download_new: bool,
    /// The queue a scheduled run downloads in.
    #[serde(default = "main_queue")]
    pub queue: crate::id::QueueId,
    /// What happens once a scheduled run is done: after the crawl, or once
    /// the queue has downloaded what the run added.
    #[serde(default, skip_serializing_if = "is_nothing")]
    pub when_done: crate::queue::WhenDone,
    /// A file to open once a scheduled run is done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_when_done: Option<String>,
    /// Shutting down when done closes programs that would hold it up, as a
    /// queue's `force_close` does.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force_close: bool,
    /// Every file address earlier runs found, so a run adds only new ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seen: Vec<String>,
    /// The day of the scheduled run last started, so it starts only once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_scheduled_day: Option<i64>,
    /// When the schedule last started it, in minutes since 1970 local time,
    /// for starting it again every so often.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_scheduled_start: Option<i64>,
    /// One-time downloading adds only what no earlier run found; periodic
    /// synchronization also checks the files earlier runs downloaded and
    /// downloads again the ones whose server copy changed.
    #[serde(default, skip_serializing_if = "crate::queue::QueueKind::is_one_time")]
    pub kind: crate::queue::QueueKind,
}

fn main_queue() -> crate::id::QueueId {
    crate::queue::MAIN_QUEUE
}

fn is_nothing(w: &crate::queue::WhenDone) -> bool {
    *w == crate::queue::WhenDone::Nothing
}

/// What a project's scheduled run does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunAction {
    /// Crawl and keep what it found, for a look later.
    #[default]
    Explore,
    /// Crawl, add the files no earlier run found to the queue, and start it.
    ExploreAndDownload,
    /// Do not crawl: add the files ticked in the project to the queue and
    /// start it.
    DownloadChecked,
}

impl RunAction {
    pub const ALL: [RunAction; 3] = [RunAction::Explore, RunAction::ExploreAndDownload, RunAction::DownloadChecked];

    pub fn label(self) -> &'static str {
        match self {
            RunAction::Explore => "Explore only",
            RunAction::ExploreAndDownload => "Explore, then download the new files",
            RunAction::DownloadChecked => "Download the ticked files, without exploring",
        }
    }
}

impl Project {
    /// The day whose scheduled run is due at `now`, if any.
    pub fn due(&self, now: crate::queue::Moment) -> Option<i64> {
        self.schedule.as_ref()?.due(now, self.last_scheduled_day, self.last_scheduled_start)
    }

    /// Note that the scheduled run of `day` starts at `now`, so it is not
    /// started again (also after a restart, once saved).
    pub fn start_scheduled_run(&mut self, day: i64, now: crate::queue::Moment) {
        self.last_scheduled_day = Some(day);
        self.last_scheduled_start = Some(now.total());
    }

    /// Take a run's findings as the project's list, and return those no
    /// earlier run found.
    pub fn record_run(&mut self, found: Vec<GrabFile>) -> Vec<GrabFile> {
        let seen: HashSet<String> = self.seen.iter().cloned().collect();
        let new: Vec<GrabFile> = found.iter().filter(|f| !seen.contains(&f.url)).cloned().collect();
        self.seen.extend(new.iter().map(|f| f.url.clone()));
        self.checked = found.iter().map(|f| !seen.contains(&f.url)).collect();
        self.files = found;
        new
    }

    /// The addresses a scheduled run checks for changes when the project
    /// synchronizes: those of `found` an earlier run found too (call it
    /// before `record_run`), or for a run that does not crawl, the ticked
    /// files. A one-time project checks none.
    pub fn to_resync(&self, found: &[GrabFile]) -> Vec<String> {
        if !self.kind.is_sync() {
            return Vec::new();
        }
        match self.run {
            RunAction::Explore => Vec::new(),
            RunAction::DownloadChecked => self.checked_files().into_iter().map(|f| f.url).collect(),
            RunAction::ExploreAndDownload => {
                let seen: HashSet<&str> = self.seen.iter().map(String::as_str).collect();
                found.iter().filter(|f| seen.contains(f.url.as_str())).map(|f| f.url.clone()).collect()
            }
        }
    }

    /// The files ticked in the project.
    pub fn checked_files(&self) -> Vec<GrabFile> {
        self.files.iter().zip(&self.checked).filter(|(_, c)| **c).map(|(f, _)| f.clone()).collect()
    }
}

/// Whether `current` holds changes its saved copy does not: it differs
/// from `saved`, the project as last saved or opened. A project never saved
/// counts once it has a start page; until then there is nothing to keep.
pub fn unsaved(current: &Project, saved: Option<&Project>) -> bool {
    match saved {
        Some(saved) => current != saved,
        None => !current.request.start.trim().is_empty(),
    }
}

/// A named template: the settings a new project can start from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub request: GrabRequest,
    /// Shipped with UDM, so it cannot be deleted.
    #[serde(skip)]
    pub built_in: bool,
}

/// The templates UDM ships, files of one kind from a site: each one's
/// name, file types and depth.
pub const BUILT_IN_TEMPLATES: [(&str, &[&str], u32); 5] = [
    ("Pictures from a site", &["jpg", "jpeg", "png", "gif", "webp", "bmp"], 2),
    ("Audio files from a site", &["mp3", "m4a", "ogg", "flac", "wav", "aac", "opus"], 2),
    ("Video files from a site", &["mp4", "mkv", "webm", "mov", "avi", "wmv"], 2),
    ("Documents from a site", &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "txt"], 3),
    ("Archives and programs from a site", &["zip", "rar", "7z", "gz", "exe", "msi"], 2),
];

/// `BUILT_IN_TEMPLATES` as templates.
pub fn built_in_templates() -> Vec<Template> {
    BUILT_IN_TEMPLATES
        .iter()
        .map(|(name, types, depth)| Template {
            name: name.to_string(),
            request: GrabRequest {
                types: types.iter().map(|t| t.to_string()).collect(),
                depth: *depth,
                ..GrabRequest::default()
            },
            built_in: true,
        })
        .collect()
}

/// A named filter for the files a crawl keeps: a name and a mask such as
/// `*.jpg;*.png`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    pub name: String,
    pub mask: String,
}

impl Filter {
    /// The masks the filter names, as `GrabRequest::types` takes them (see
    /// `mask_entry`): `*.jpg` becomes `jpg`, `*` or `*.*` keeps every type,
    /// and a mask with a name part such as `photo_*.jpg` stays whole.
    pub fn types(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for t in self.mask.split([';', ',', ' ']).map(mask_entry).filter(|p| !p.is_empty()) {
            if !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }
}

/// The filters a new installation starts with.
pub fn default_filters() -> Vec<Filter> {
    let f = |name: &str, mask: &str| Filter { name: name.to_string(), mask: mask.to_string() };
    vec![
        f("All files", "*.*"),
        f("Documents", "*.pdf;*.doc;*.docx;*.xls;*.xlsx;*.ppt;*.pptx;*.odt;*.txt"),
        f("Pictures", "*.jpg;*.jpeg;*.png;*.gif;*.webp;*.bmp"),
        f("Audio", "*.mp3;*.m4a;*.ogg;*.flac;*.wav;*.aac"),
        f("Video", "*.mp4;*.mkv;*.webm;*.mov;*.avi;*.wmv"),
        f("Archives", "*.zip;*.rar;*.7z;*.gz;*.tar"),
        f("Programs", "*.exe;*.msi;*.apk"),
    ]
}

/// How a crawl went, so far or in the end.
#[derive(Debug, Default)]
pub struct Outcome {
    /// The files kept, in the order they were found.
    pub files: Vec<GrabFile>,
    /// Pages read.
    pub pages: u64,
    /// Bytes of pages read.
    pub bytes: ByteCount,
    /// Links to files looked at, kept or not.
    pub explored: u64,
    /// Why it stopped before reading everything in reach, if it did.
    pub stopped: Option<String>,
}

/// The name a project's file is kept under, before `.project.json`: the
/// project's name with each character a file name cannot hold made `_`.
pub fn project_file_stem(name: &str) -> String {
    let safe: String =
        name.trim().chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect();
    safe.trim_matches('.').to_string()
}

/// Whether saving a project named `name` keeps every one of the saved
/// `projects`: none has that name, or it is the project's own copy, saved
/// as `own`. Names are compared as their files are, ignoring case as
/// Windows does, so "a:b" and "a_b", or "Docs" and "docs", are one name.
pub fn project_name_is_unique(projects: &[String], name: &str, own: Option<&str>) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    let key = |n: &str| project_file_stem(n).to_lowercase();
    let wanted = key(name);
    if own.is_some_and(|own| key(own) == wanted) {
        return true;
    }
    !projects.iter().any(|p| key(p) == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project file's byte limits are plain numbers, read and written
    /// back unchanged.
    #[test]
    fn byte_limits_keep_their_file_shape() {
        let mut old = serde_json::to_value(GrabRequest::default()).unwrap();
        old["max_bytes"] = 52_428_800.into();
        old["min_size"] = 100.into();
        old["max_size"] = 1_000_000.into();
        let req: GrabRequest = serde_json::from_value(old.clone()).unwrap();
        assert_eq!((req.max_bytes, req.min_size), (ByteCount::mib(50), Some(ByteCount::new(100))));
        assert_eq!(serde_json::to_value(&req).unwrap(), old);
        let file = r#"{"url":"u","name":"a.pdf","page":"p","size":5000}"#;
        let found: GrabFile = serde_json::from_str(file).unwrap();
        assert_eq!(serde_json::to_string(&found).unwrap(), file);
    }

    #[test]
    fn a_project_name_is_compared_as_its_file() {
        let saved = vec!["Docs".to_string(), "a:b".to_string()];
        assert!(!project_name_is_unique(&saved, "docs", None), "case does not matter");
        assert!(!project_name_is_unique(&saved, "a_b", None), "a:b is kept as a_b");
        assert!(project_name_is_unique(&saved, "docs", Some("Docs")), "its own copy");
        assert!(project_name_is_unique(&saved, "Other", None));
        assert!(!project_name_is_unique(&saved, "  ", None));
    }

    fn request(depth: u32) -> GrabRequest {
        GrabRequest { start: "https://s/".into(), depth, ..GrabRequest::default() }
    }

    #[test]
    fn patterns_match_addresses() {
        assert!(pattern_matches("*/docs/*", "https://s/docs/a.pdf"));
        assert!(pattern_matches("/docs/", "https://s/docs/a.pdf"), "no * matches anywhere");
        assert!(!pattern_matches("*/docs/*", "https://s/blog/a.pdf"));
        assert!(pattern_matches("s/*.PDF", "https://s/x/a.pdf"), "no scheme, any case");
        assert!(pattern_matches("https://s/*", "https://s/a"));
        assert!(!pattern_matches("http://s/*", "https://s/a"));
        assert!(!pattern_matches("", "https://s/a"));
    }

    #[test]
    fn a_project_is_unsaved_once_it_differs_from_its_copy() {
        let p = Project {
            name: String::new(),
            request: GrabRequest::default(),
            files: Vec::new(),
            checked: Vec::new(),
            schedule: None,
            run: RunAction::Explore,
            download_new: false,
            when_done: crate::queue::WhenDone::Nothing,
            open_when_done: None,
            force_close: false,
            queue: crate::queue::MAIN_QUEUE,
            seen: Vec::new(),
            last_scheduled_day: None,
            last_scheduled_start: None,
            kind: crate::queue::QueueKind::OneTime,
        };
        assert!(!unsaved(&p, None), "a new, empty project has nothing to keep");
        let started = Project { request: request(1), ..p.clone() };
        assert!(unsaved(&started, None), "never saved");
        assert!(!unsaved(&started, Some(&started)));
        let crawled = Project {
            files: vec![GrabFile {
                url: "https://s/a.pdf".into(),
                name: "a.pdf".into(),
                page: "https://s/".into(),
                size: None,
                text: None,
            }],
            checked: vec![true],
            ..started.clone()
        };
        assert!(unsaved(&crawled, Some(&started)), "a crawl found new files");
        assert!(unsaved(&Project { name: "Renamed".into(), ..started.clone() }, Some(&started)));
    }

    #[test]
    fn mirror_paths_follow_addresses() {
        let root = Path::new(r"C:\site");
        let at = |u: &str, page: bool| mirror_path(root, &Url::parse(u).unwrap(), page);
        assert_eq!(at("https://s.com/docs/a%20b.pdf", false), root.join("s.com").join("docs").join("a b.pdf"));
        assert_eq!(at("https://s.com/docs/", true), root.join("s.com").join("docs").join("index.html"));
        assert_eq!(at("https://s.com/", true), root.join("s.com").join("index.html"));
        assert_eq!(at("https://s.com/list.php", true), root.join("s.com").join("list.php.html"));
        assert_ne!(at("https://s.com/list.php?page=1", true), at("https://s.com/list.php?page=2", true));
        assert_eq!(at("https://s.com/a/../b:c.pdf", false), root.join("s.com").join("b_c.pdf"));
    }

    #[test]
    fn requests_are_checked() {
        assert!(request(2).validate().is_ok());
        assert!(GrabRequest { start: "ftp://s/".into(), ..request(2) }.validate().is_err());
        assert!(GrabRequest { start: "s/".into(), ..request(2) }.validate().is_err());
        assert!(request(11).validate().is_err());
        assert!(GrabRequest { max_pages: 0, ..request(1) }.validate().is_err());
        assert!(GrabRequest { delay_ms: 0, ..request(1) }.validate().is_err());
        assert!(GrabRequest { types: vec![], ..request(1) }.validate().is_err());
    }

    #[test]
    fn login_page_cookies_go_to_its_site_only() {
        let req = GrabRequest {
            start: "https://docs.example.com/guide/".into(),
            login_url: Some("https://example.com/login".into()),
            ..GrabRequest::default()
        };
        assert!(req.validate().is_ok());
        let both = |url: &str| crawl_cookies(&req, url, Some("a=1"), Some("s=2"));
        assert_eq!(both("https://example.com/files/"), Some("s=2".into()), "the login site gets its cookies");
        assert_eq!(both("https://docs.example.com/guide/x"), Some("a=1".into()), "the start site gets its own");
        assert_eq!(both("https://cdn.other.net/x"), None, "another host gets none");
        assert_eq!(both("https://evil.example.com.other.net/"), None);
        assert_eq!(
            crawl_cookies(&req, "https://shop.example.com/", None, Some("s=2")),
            None,
            "not a subdomain without the option"
        );
        let wide = GrabRequest { subdomains: true, ..req.clone() };
        assert_eq!(crawl_cookies(&wide, "https://shop.example.com/", None, Some("s=2")), Some("s=2".into()));
        // One site for both: the cookies are joined.
        let same = GrabRequest { start: "https://example.com/".into(), ..req.clone() };
        assert_eq!(crawl_cookies(&same, "https://example.com/a", Some("a=1"), Some("s=2")), Some("a=1; s=2".into()));
        let none = GrabRequest { login_url: None, ..req.clone() };
        assert_eq!(crawl_cookies(&none, "https://example.com/files/", None, Some("s=2")), None);
        assert!(GrabRequest { login_url: Some("example.com/login".into()), ..req.clone() }.validate().is_err());
        assert!(GrabRequest { login_url: Some("ftp://example.com/".into()), ..req }.validate().is_err());
    }
}
