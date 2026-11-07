//! The site grabber: crawl a site from a start page and list the files it
//! links to, such as every PDF on a documentation site.
//!
//! The crawl is polite and bounded: it reads `robots.txt` and skips what it
//! disallows, waits between requests (longer if `Crawl-delay` asks), follows
//! links only so many levels deep, and stops at a page and size budget.
//! Pages are untrusted input: they are parsed with a real HTML parser
//! (html5ever), and only http and https links are followed or listed.
//!
//! The crawler takes its fetch function as an argument, so it is tested
//! without a network; the daemon passes one that uses the engine's client
//! (proxy, logins).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use scraper::{Html, Selector};

use url::Url;

pub use uidm_domain::grab::*;

/// The saved password of the grabber login `user` on the site of `start`,
/// or the site's one saved by an older version.
pub fn project_password(start: &str, user: &str) -> Option<String> {
    crate::net::load_credential(&project_login_target(start, user))
        .or_else(|| crate::net::load_credential(&host_login_target(start)))
}

/// A `Crawl-delay` longer than this is capped, so one site cannot stall a
/// crawl for hours.
const MAX_CRAWL_DELAY: Duration = Duration::from_secs(30);

/// `html`, read from `page` and kept at `here`, with the links that `local`
/// says are kept locally (given the target's address, it answers whether
/// it is a page and so where it is kept) pointing at their copies.
/// Other links stay as they are.
pub fn rewrite_links(html: &str, page: &Url, here: &Path, root: &Path, local: impl Fn(&Url) -> Option<bool>) -> String {
    let doc = Html::parse_document(html);
    let mut replacements: Vec<(String, String)> = Vec::new();
    for (sel, attr) in [
        ("a[href]", "href"),
        ("area[href]", "href"),
        ("img[src]", "src"),
        ("source[src]", "src"),
        ("iframe[src]", "src"),
        ("frame[src]", "src"),
        ("link[href]", "href"),
    ] {
        let Ok(s) = Selector::parse(sel) else { continue };
        for e in doc.select(&s) {
            let Some(value) = e.value().attr(attr) else { continue };
            let Ok(mut target) = page.join(value.trim()) else { continue };
            let fragment = target.fragment().map(|f| format!("#{f}")).unwrap_or_default();
            target.set_fragment(None);
            if let Some(is_page) = local(&target) {
                let link = relative_link(here, &mirror_path(root, &target, is_page));
                replacements.push((value.to_string(), format!("{link}{fragment}")));
            }
        }
    }
    let mut out = html.to_string();
    for (from, to) in replacements {
        for quote in ['"', '\''] {
            out = out.replace(&format!("={quote}{from}{quote}"), &format!("={quote}{to}{quote}"));
        }
    }
    out
}

/// The download for found file `f`: waiting, with the page it was found on
/// as both its referrer and its parent web page, its link text as
/// description when the crawl keeps those, and in the folder the request
/// saves to: under the mirror folder when the crawl keeps one, else one
/// category's folder (from `settings`) or one folder for every file.
/// `login_cookies`, the login page site's cookies, go with a file on that
/// site only (`crawl_cookies`), as a captured download carries the
/// browser's. `None` when the mirror skips existing files and this one
/// is there.
pub fn download_for(
    req: &GrabRequest,
    f: &GrabFile,
    settings: &crate::settings::Settings,
    login_cookies: Option<&str>,
) -> Option<uidm_domain::download::Download> {
    let mut d = uidm_domain::download::Download {
        url: f.url.clone(),
        referrer: Some(f.page.clone()),
        page: Some(f.page.clone()),
        cookies: crawl_cookies(req, &f.url, None, login_cookies),
        start: Some(false),
        description: f.text.clone().filter(|t| req.link_text && !t.is_empty()),
        ..uidm_domain::download::Download::default()
    };
    match &req.save_to {
        SaveTo::Categories => {}
        SaveTo::Category(name) => {
            let name = name.trim();
            d.folder =
                Some(settings.category_folder(name, &crate::settings::default_downloads_dir()).display().to_string());
            d.category = Some(name.to_string());
        }
        SaveTo::Folder(folder) => d.folder = Some(folder.trim().to_string()),
    }
    if let (Some(m), Ok(url)) = (&req.mirror, Url::parse(&f.url)) {
        let path = mirror_path(Path::new(m.folder.trim()), &url, false);
        if path.exists() && !m.overwrite {
            return None;
        }
        d.filename = path.file_name().map(|n| n.to_string_lossy().into_owned());
        d.folder = path.parent().map(|p| p.display().to_string());
        d.file_exists = Some(if m.overwrite {
            crate::settings::FileExists::Overwrite
        } else {
            crate::settings::FileExists::Rename
        });
    } else if let Ok(url) = Url::parse(&f.url) {
        // The site's folders under the chosen folder, or under the
        // category's folder the file would go to by itself.
        let sub = if req.subfolders { relative_subfolder(&req.start, &url) } else { PathBuf::new() };
        if !sub.as_os_str().is_empty() {
            let base = match &d.folder {
                Some(folder) => PathBuf::from(folder),
                None => settings
                    .folders_for(&f.url, &crate::settings::default_downloads_dir())
                    .for_name(&url_file_name(&url)),
            };
            d.folder = Some(base.join(sub).display().to_string());
        }
        if req.overwrite {
            d.file_exists = Some(crate::settings::FileExists::Overwrite);
        }
    }
    // The project's own login goes to the start site's files.
    if let Some(user) = req.login_user.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        if on_start_site(req, &f.url) {
            d.login = Some(uidm_domain::download::DownloadLogin {
                username: user.to_string(),
                password: project_password(&req.start, user),
            });
        }
    }
    Some(d)
}

/// The rules `robots.txt` sets for the crawler.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Robots {
    /// `(allow, path prefix)`; the longest matching prefix wins, `allow` on a
    /// tie.
    rules: Vec<(bool, String)>,
    pub crawl_delay: Option<Duration>,
}

impl Robots {
    /// Read `robots.txt`, keeping the group for `agent` if it has one, or
    /// else the `*` group.
    pub fn parse(text: &str, agent: &str) -> Robots {
        let agent = agent.to_ascii_lowercase();
        // Groups: the agents named, then their rules.
        let mut groups: Vec<(Vec<String>, Robots)> = Vec::new();
        let mut in_agents = false;
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((key, value)) = line.split_once(':') else { continue };
            let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
            match key.as_str() {
                "user-agent" => {
                    if !in_agents {
                        groups.push((Vec::new(), Robots::default()));
                    }
                    in_agents = true;
                    if let Some(g) = groups.last_mut() {
                        g.0.push(value.to_ascii_lowercase());
                    }
                }
                "allow" | "disallow" => {
                    in_agents = false;
                    if let Some(g) = groups.last_mut() {
                        // An empty Disallow allows everything.
                        if !value.is_empty() {
                            g.1.rules.push((key == "allow", value.to_string()));
                        }
                    }
                }
                "crawl-delay" => {
                    in_agents = false;
                    if let (Some(g), Ok(secs)) = (groups.last_mut(), value.parse::<f64>()) {
                        if secs.is_finite() && secs >= 0.0 {
                            g.1.crawl_delay = Some(Duration::from_secs_f64(secs).min(MAX_CRAWL_DELAY));
                        }
                    }
                }
                _ => {}
            }
        }
        let named = groups.iter().find(|(agents, _)| agents.iter().any(|a| a != "*" && agent.contains(a.as_str())));
        let any = groups.iter().find(|(agents, _)| agents.iter().any(|a| a == "*"));
        named.or(any).map(|(_, r)| r.clone()).unwrap_or_default()
    }

    /// Whether the path (with its query) of `url` may be crawled.
    pub fn allows(&self, url: &Url) -> bool {
        let mut path = url.path().to_string();
        if let Some(q) = url.query() {
            path.push('?');
            path.push_str(q);
        }
        let mut best: Option<(usize, bool)> = None;
        for (allow, prefix) in &self.rules {
            if robots_match(prefix, &path) {
                let len = prefix.len();
                if best.is_none_or(|(l, a)| len > l || (len == l && *allow && !a)) {
                    best = Some((len, *allow));
                }
            }
        }
        best.is_none_or(|(_, allow)| allow)
    }
}

/// A robots.txt path pattern: a prefix, with `*` for any run of characters
/// and `$` for the end.
fn robots_match(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let pieces: Vec<&str> = pattern.split('*').collect();
    let Some(mut rest) = path.strip_prefix(pieces[0]) else {
        return false;
    };
    for piece in &pieces[1..] {
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    match (anchored, pieces.len()) {
        (false, _) => true,
        (true, 1) => rest.is_empty(),
        (true, _) => path.ends_with(pieces[pieces.len() - 1]),
    }
}

/// The links on a page, resolved against it (or its `<base href>`): those
/// that may be pages, and those that may be files with their link text
/// (the element's text, else its `alt` or `title`).
pub fn links(html: &str, page: &Url) -> (Vec<Url>, Vec<(Url, String)>) {
    let doc = Html::parse_document(html);
    let base = Selector::parse("base[href]")
        .ok()
        .and_then(|s| doc.select(&s).next().and_then(|b| b.value().attr("href")).and_then(|h| page.join(h).ok()))
        .filter(|b| matches!(b.scheme(), "http" | "https"))
        .unwrap_or_else(|| page.clone());
    let pick = |selector: &str, attr: &str| -> Vec<(Url, String)> {
        let Ok(sel) = Selector::parse(selector) else { return Vec::new() };
        doc.select(&sel)
            .filter_map(|e| {
                let mut u = base.join(e.value().attr(attr)?.trim()).ok()?;
                u.set_fragment(None);
                let text: String = e.text().collect::<Vec<_>>().join(" ");
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                let text = if text.is_empty() {
                    e.value().attr("alt").or(e.value().attr("title")).unwrap_or("").trim().to_string()
                } else {
                    text
                };
                Some((u, text.chars().take(200).collect()))
            })
            .filter(|(u, _)| matches!(u.scheme(), "http" | "https"))
            .collect()
    };
    let mut files = pick("a[href]", "href");
    files.extend(pick("area[href]", "href"));
    files.extend(pick("iframe[src], frame[src]", "src"));
    let pages = files.iter().map(|(u, _)| u.clone()).collect();
    for (sel, attr) in [
        ("img[src]", "src"),
        ("source[src]", "src"),
        ("video[src]", "src"),
        ("audio[src]", "src"),
        ("embed[src]", "src"),
        ("object[data]", "data"),
    ] {
        files.extend(pick(sel, attr));
    }
    (pages, files)
}

/// `%APPDATA%\UIDM\grabber`, next to the settings file.
pub fn projects_dir() -> io::Result<PathBuf> {
    crate::paths::grabber_dir()
}

const TEMPLATE_FILE: &str = "template.json";

/// The file a project named `name` is kept in.
fn project_file(dir: &Path, name: &str) -> PathBuf {
    let safe: String =
        name.trim().chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect();
    dir.join(format!("{}.project.json", safe.trim_matches('.')))
}

/// Whether saving a project named `name` in `dir` keeps every other saved
/// project: no file there has that name, or it is the project's own copy,
/// saved under `own`. Names are compared as their files are, and ignoring
/// case as Windows does, so "a:b" and "a_b", or "Docs" and "docs", are the
/// same name.
pub fn name_is_unique(dir: &Path, name: &str, own: Option<&str>) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    let file = project_file(dir, name);
    let same = |a: &Path, b: &Path| a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase();
    if own.is_some_and(|own| same(&project_file(dir, own), &file)) {
        return true;
    }
    match fs::read_dir(dir) {
        Ok(entries) => !entries.flatten().any(|e| same(&e.path(), &file)),
        Err(_) => true,
    }
}

/// The saved projects' names, most recently saved first.
pub fn recent_projects(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(SystemTime, String)> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".project.json"))
        .filter_map(|e| {
            let modified = e.metadata().and_then(|m| m.modified()).ok()?;
            let project: Project = serde_json::from_slice(&fs::read(e.path()).ok()?).ok()?;
            Some((modified, project.name))
        })
        .collect();
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    found.into_iter().map(|(_, n)| n).collect()
}

/// The settings new projects start from: the saved template, or the
/// defaults.
pub fn template(dir: &Path) -> GrabRequest {
    fs::read(dir.join(TEMPLATE_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<GrabRequest>(&b).ok())
        .map(|r| GrabRequest { start: String::new(), ..r })
        .unwrap_or_default()
}

pub fn save_template(dir: &Path, req: &GrabRequest) -> io::Result<()> {
    let req = GrabRequest { start: String::new(), login_cookies: None, ..req.clone() };
    let bytes = serde_json::to_vec_pretty(&req).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    crate::settings::write_atomic(&dir.join(TEMPLATE_FILE), &bytes)
}

const FILTERS_FILE: &str = "filters.json";

/// The saved filters, or the defaults before any were saved.
pub fn load_filters(dir: &Path) -> Vec<Filter> {
    fs::read(dir.join(FILTERS_FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(default_filters)
}

pub fn save_filters(dir: &Path, filters: &[Filter]) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(filters).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    crate::settings::write_atomic(&dir.join(FILTERS_FILE), &bytes)
}

/// The folder the named templates are kept in.
fn templates_dir(dir: &Path) -> PathBuf {
    dir.join("templates")
}

/// The built-in templates, then the user's by name.
pub fn templates(dir: &Path) -> Vec<Template> {
    let mut saved: Vec<Template> = fs::read_dir(templates_dir(dir))
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
                .filter_map(|e| serde_json::from_slice::<Template>(&fs::read(e.path()).ok()?).ok())
                .collect()
        })
        .unwrap_or_default();
    saved.sort_by_key(|t| t.name.to_lowercase());
    let mut all = built_in_templates();
    all.extend(saved);
    all
}

/// The file the template named `name` is kept in.
fn template_file(dir: &Path, name: &str) -> PathBuf {
    let file = project_file(&templates_dir(dir), name);
    file.with_file_name(file.file_name().unwrap_or_default().to_string_lossy().replace(".project.json", ".json"))
}

/// Save `req` as the template named `name`, without its start page.
pub fn save_named_template(dir: &Path, name: &str, req: &GrabRequest) -> io::Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "A template needs a name."));
    }
    if built_in_templates().iter().any(|t| t.name.eq_ignore_ascii_case(name)) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "A template that comes with UIDM has that name."));
    }
    let t = Template {
        name: name.to_string(),
        request: GrabRequest { start: String::new(), login_user: None, login_cookies: None, ..req.clone() },
        built_in: false,
    };
    let bytes = serde_json::to_vec_pretty(&t).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    crate::settings::write_atomic(&template_file(dir, name), &bytes)
}

/// Delete the user's template named `name`.
pub fn delete_template(dir: &Path, name: &str) -> io::Result<()> {
    fs::remove_file(template_file(dir, name))
}

/// A page waiting to be read: its address, how many links from the start
/// page it is, and how many of those in a row were on other sites.
struct Queued {
    url: Url,
    depth: u32,
    off_site: u32,
}

/// Crawl from `req.start`. `fetch` reads a page (at most `MAX_PAGE_BYTES`),
/// up to `req.explore_at_once` pages at a time on their own threads; `size`
/// asks a file's size, for the size filters and duplicates; `wait` sleeps
/// between requests (tests pass a no-op); `progress` hears how the crawl
/// goes after each page. `cancel` stops the crawl between requests.
pub fn crawl(
    req: &GrabRequest,
    fetch: impl Fn(&str) -> Result<Page, String> + Sync,
    mut size: impl FnMut(&str) -> Option<u64>,
    mut wait: impl FnMut(Duration),
    cancel: &AtomicBool,
    mut progress: impl FnMut(&Outcome),
) -> Outcome {
    let mut out = Outcome::default();
    let Ok(mut start) = Url::parse(req.start.trim()) else {
        out.stopped = Some("The start page is not a web address.".into());
        return out;
    };
    start.set_fragment(None);
    let start_host = start.host_str().unwrap_or("").to_ascii_lowercase();
    // The domain whose subdomains count as the same site: the start host
    // without a leading www.
    let domain = start_host.strip_prefix("www.").unwrap_or(&start_host).to_string();
    let on_site = |u: &Url| {
        let host = u.host_str().unwrap_or("").to_ascii_lowercase();
        host == start_host || (req.subdomains && (host == domain || host.ends_with(&format!(".{domain}"))))
    };
    // A page on the internet may link to this computer or the local
    // network (a router's page, a local service); the crawl goes there only
    // when it started there.
    let local_start = local_host(&start);
    let reachable = |u: &Url| local_start || !local_host(u);
    // The folder the crawl may not go above.
    let start_dir = start.path().rsplit_once('/').map_or("/".to_string(), |(dir, _)| format!("{dir}/"));
    // The files kept so far, by name (as indexes into `out.files`), to spot
    // duplicates, and those whose size was asked.
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    let mut asked: HashSet<usize> = HashSet::new();
    let types = Types::of(req);
    let delay = Duration::from_millis(req.delay_ms);
    let at_once = req.explore_at_once.clamp(AT_ONCE_RANGE.0, AT_ONCE_RANGE.1) as usize;

    let mut robots: HashMap<String, Robots> = HashMap::new();
    let mut queue: VecDeque<Queued> = VecDeque::from([Queued { url: start.clone(), depth: 0, off_site: 0 }]);
    let mut seen_pages: HashSet<String> = HashSet::from([start.to_string()]);
    let mut seen_files: HashSet<String> = HashSet::new();
    let mut first = true;
    let mut skipped_by_robots = 0u64;

    'crawl: while !queue.is_empty() {
        if cancel.load(Ordering::Relaxed) {
            out.stopped = Some("Stopped.".into());
            break;
        }
        if out.pages >= req.max_pages {
            out.stopped = Some(format!("Stopped at the budget of {} pages.", req.max_pages));
            break;
        }
        if out.bytes >= req.max_bytes {
            out.stopped = Some(format!("Stopped at the budget of {} MB.", req.max_bytes / (1024 * 1024)));
            break;
        }
        // The next pages robots.txt lets the crawl read, as many as are
        // read at once, within the page budget.
        let room = (req.max_pages - out.pages) as usize;
        let mut batch: Vec<Queued> = Vec::new();
        let mut pause = delay;
        while batch.len() < at_once.min(room) {
            let Some(next) = queue.pop_front() else { break };
            let origin = next.url.origin().ascii_serialization();
            if !robots.contains_key(&origin) {
                if !first {
                    wait(delay);
                }
                first = false;
                if cancel.load(Ordering::Relaxed) {
                    out.stopped = Some("Stopped.".into());
                    break 'crawl;
                }
                let rules = match fetch(&format!("{origin}/robots.txt")) {
                    Ok(page) => Robots::parse(&String::from_utf8_lossy(&page.body), USER_AGENT),
                    Err(_) => Robots::default(),
                };
                robots.insert(origin.clone(), rules);
            }
            let rules = &robots[&origin];
            if !rules.allows(&next.url) {
                skipped_by_robots += 1;
                continue;
            }
            pause = rules.crawl_delay.map_or(pause, |d| d.max(pause));
            batch.push(next);
        }
        if batch.is_empty() {
            continue;
        }
        if !first {
            wait(pause);
        }
        first = false;
        if cancel.load(Ordering::Relaxed) {
            out.stopped = Some("Stopped.".into());
            break;
        }
        let fetched: Vec<Result<Page, String>> = if batch.len() == 1 {
            vec![fetch(batch[0].url.as_str())]
        } else {
            std::thread::scope(|s| {
                let fetch = &fetch;
                let reading: Vec<_> = batch.iter().map(|q| s.spawn(move || fetch(q.url.as_str()))).collect();
                reading
                    .into_iter()
                    .map(|r| r.join().unwrap_or_else(|_| Err("the page could not be read".into())))
                    .collect()
            })
        };

        for (Queued { url, depth, off_site }, page) in batch.into_iter().zip(fetched) {
            let Ok(page) = page else { continue };
            out.pages += 1;
            out.bytes += page.body.len() as u64;
            let html = page.content_type.as_deref().is_none_or(|t| t.to_ascii_lowercase().contains("html"));
            if html {
                let here = Url::parse(&page.url).unwrap_or(url.clone());
                let text = String::from_utf8_lossy(&page.body);
                let (pages, files) = links(&text, &here);
                for (file, link_text) in files {
                    let Some(ext) = url_extension(&file) else { continue };
                    let name = url_file_name(&file);
                    if PAGE_EXTENSIONS.contains(&ext.as_str()) && !types.named(&name, &ext) {
                        continue;
                    }
                    if !seen_files.insert(file.to_string()) {
                        continue;
                    }
                    out.explored += 1;
                    if !types.keeps(&name, &ext)
                        || !passes(file.as_str(), &req.files_only, &req.files_skip)
                        || (req.files_this_site && !on_site(&file))
                        || !reachable(&file)
                    {
                        continue;
                    }
                    // Ask the size only when a filter needs it, or to compare
                    // with files of the same name kept earlier (whose sizes
                    // are asked then too).
                    let twins = by_name.get(&name).cloned().unwrap_or_default();
                    let compare = req.hide_duplicates && !twins.is_empty();
                    let need = req.min_size.is_some() || req.max_size.is_some() || compare;
                    if need && cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    if compare {
                        for &i in &twins {
                            if out.files[i].size.is_none() && asked.insert(i) {
                                wait(delay);
                                out.files[i].size = size(&out.files[i].url);
                            }
                        }
                    }
                    let known = if need {
                        wait(delay);
                        size(file.as_str())
                    } else {
                        None
                    };
                    if let Some(n) = known {
                        if req.min_size.is_some_and(|lo| n < lo) || req.max_size.is_some_and(|hi| n > hi) {
                            continue;
                        }
                        if compare && twins.iter().any(|&i| out.files[i].size == Some(n)) {
                            continue;
                        }
                    }
                    let at = out.files.len();
                    if need {
                        asked.insert(at);
                    }
                    by_name.entry(name.clone()).or_default().push(at);
                    out.files.push(GrabFile {
                        url: file.to_string(),
                        name,
                        page: here.to_string(),
                        size: known,
                        text: (req.link_text && !link_text.is_empty()).then_some(link_text),
                    });
                }
                for next in pages {
                    if !reachable(&next) {
                        continue;
                    }
                    let here_site = on_site(&next);
                    let exact = next.host_str().unwrap_or("").eq_ignore_ascii_case(&start_host);
                    // How far it may go: on the start site, `depth` links (or
                    // any number for the whole site); elsewhere, `other_depth`
                    // links in a row, or `depth` in all without it.
                    let in_reach = if here_site {
                        req.whole_site || depth < req.depth
                    } else if req.same_site {
                        false
                    } else {
                        match req.other_depth {
                            Some(n) => off_site < n,
                            None => depth < req.depth,
                        }
                    };
                    let below = !req.no_parent || !exact || next.path().starts_with(&start_dir);
                    let page_like = url_extension(&next).is_none_or(|e| PAGE_EXTENSIONS.contains(&e.as_str()));
                    let wanted = passes(next.as_str(), &req.explore_only, &req.explore_skip);
                    if in_reach && below && page_like && wanted && seen_pages.insert(next.to_string()) {
                        queue.push_back(Queued {
                            url: next,
                            depth: depth.saturating_add(1),
                            off_site: if here_site { 0 } else { off_site + 1 },
                        });
                    }
                }
            }
            progress(&out);
        }
    }
    if out.stopped.is_none() && skipped_by_robots > 0 {
        let pages = match skipped_by_robots {
            1 => "1 page was".to_string(),
            n => format!("{n} pages were"),
        };
        out.stopped = Some(format!("{pages} skipped because robots.txt asks crawlers to stay out."));
    }
    out
}

/// The login cookies a project file keeps for this user: sealed (see
/// `dpapi`) and base64-encoded, so a copied project file does not carry the
/// sign-in. Cookies that cannot be read back, such as from another computer,
/// are dropped.
fn seal_cookies(request: &mut serde_json::Value) {
    use base64::Engine;
    let Some(cookies) = request.get("login_cookies").and_then(|c| c.as_str()) else {
        return;
    };
    match crate::dpapi::protect(cookies.as_bytes()) {
        Ok(sealed) => {
            request["login_cookies"] = base64::engine::general_purpose::STANDARD.encode(sealed).into();
        }
        Err(_) => {
            if let Some(map) = request.as_object_mut() {
                map.remove("login_cookies");
            }
        }
    }
}

/// The reverse of `seal_cookies`.
fn unseal_cookies(request: &mut serde_json::Value) {
    use base64::Engine;
    let Some(sealed) = request.get("login_cookies").and_then(|c| c.as_str()) else {
        return;
    };
    let plain = base64::engine::general_purpose::STANDARD
        .decode(sealed)
        .ok()
        .and_then(|b| crate::dpapi::unprotect(&b).ok())
        .and_then(|b| String::from_utf8(b).ok());
    match plain {
        Some(p) => request["login_cookies"] = p.into(),
        None => {
            if let Some(map) = request.as_object_mut() {
                map.remove("login_cookies");
            }
        }
    }
}

/// The [`GrabProjects`](uidm_domain::ports::GrabProjects) port over the
/// projects folder.
pub struct ProjectFiles(pub PathBuf);

impl uidm_domain::ports::GrabProjects for ProjectFiles {
    fn names(&self) -> Vec<String> {
        recent_projects(&self.0)
    }

    fn load(&self, name: &str) -> Option<Project> {
        load_project(&self.0, name).ok()
    }

    fn save(&self, project: &Project) -> Result<(), uidm_domain::ports::FilesError> {
        save_project(project, &self.0).map_err(|e| uidm_domain::ports::FilesError(e.to_string()))
    }
}

/// Save project `p` in `dir`, its login cookies sealed.
pub fn save_project(p: &Project, dir: &Path) -> io::Result<()> {
    if p.name.trim().is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "A project needs a name."));
    }
    let invalid = |e| io::Error::new(io::ErrorKind::InvalidData, e);
    let mut value = serde_json::to_value(p).map_err(invalid)?;
    seal_cookies(&mut value["request"]);
    let bytes = serde_json::to_vec_pretty(&value).map_err(invalid)?;
    crate::settings::write_atomic(&project_file(dir, &p.name), &bytes)
}

/// The project named `name` in `dir`.
pub fn load_project(dir: &Path, name: &str) -> io::Result<Project> {
    load_project_file(&project_file(dir, name))
}

/// A project file from anywhere, such as one copied from another
/// computer; read and checked as one from the projects folder is.
pub fn load_project_file(path: &Path) -> io::Result<Project> {
    let bytes = fs::read(path)?;
    // A project holds its login's password and, once unsealed, its cookies;
    // serde's message could quote either.
    let invalid = |e: serde_json::Error| crate::logging::json_error(&e);
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).map_err(invalid)?;
    if let Some(request) = value.get_mut("request") {
        unseal_cookies(request);
    }
    let mut p: Project = serde_json::from_value(value).map_err(invalid)?;
    if std::mem::take(&mut p.download_new) && p.run == RunAction::Explore {
        p.run = RunAction::ExploreAndDownload;
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn robots_rules() {
        let text = "
# comment
User-agent: Googlebot
Disallow: /

User-agent: *
Disallow: /private/
Allow: /private/open.html
Disallow: /*.cgi$
Disallow: /tmp*/x
Crawl-delay: 2.5
";
        let r = Robots::parse(text, USER_AGENT);
        let ok = |p: &str| r.allows(&Url::parse(&format!("https://s{p}")).unwrap());
        assert!(ok("/docs/a.pdf"));
        assert!(!ok("/private/secret.html"));
        assert!(ok("/private/open.html"));
        assert!(!ok("/run.cgi"));
        assert!(ok("/run.cgi?x=1"));
        assert!(!ok("/tmp1/x/y"));
        assert_eq!(r.crawl_delay, Some(Duration::from_millis(2500)));

        let named = Robots::parse("User-agent: UIDM\nDisallow: /\n\nUser-agent: *\nDisallow:\n", USER_AGENT);
        assert!(!named.allows(&Url::parse("https://s/a").unwrap()));
        let empty = Robots::parse("User-agent: *\nDisallow:\n", USER_AGENT);
        assert!(empty.allows(&Url::parse("https://s/a").unwrap()));
        let huge = Robots::parse("User-agent: *\nCrawl-delay: 99999\n", USER_AGENT);
        assert_eq!(huge.crawl_delay, Some(MAX_CRAWL_DELAY));
    }

    #[test]
    fn links_are_resolved_and_only_web() {
        let page = Url::parse("https://s/docs/index.html").unwrap();
        let html = r#"<html><head><title>x</title></head><body>
            <a href="guide.pdf">Guide</a>
            <a href="/files/b.PDF#page=2">B</a>
            <a href="javascript:alert(1)">no</a>
            <a href="mailto:a@b">no</a>
            <a href="data:text/html,x">no</a>
            <a href="sub/">Sub</a>
            <img src="//cdn.s/pic.png">
            <script>var a = '<a href="hidden.pdf">';</script>
            <a href = "  spaced.pdf  ">
        </body></html>"#;
        let (pages, files) = links(html, &page);
        let pages: Vec<String> = pages.iter().map(Url::to_string).collect();
        assert!(pages.contains(&"https://s/docs/guide.pdf".to_string()));
        assert!(pages.contains(&"https://s/files/b.PDF".to_string()));
        assert!(pages.contains(&"https://s/docs/sub/".to_string()));
        assert!(pages.contains(&"https://s/docs/spaced.pdf".to_string()));
        assert!(!pages.iter().any(|p| p.contains("hidden")), "script text is not markup");
        assert!(pages.iter().all(|p| p.starts_with("https://")));
        assert!(files.iter().any(|(f, _)| f.as_str() == "https://cdn.s/pic.png"));
        let text = |url: &str| files.iter().find(|(f, _)| f.as_str() == url).map(|(_, t)| t.as_str());
        assert_eq!(text("https://s/docs/guide.pdf"), Some("Guide"));
        let titled = links(r#"<a href="a.pdf" title="The A file"></a><img src="p.png" alt="A picture">"#, &page).1;
        assert_eq!(titled[0].1, "The A file");
        assert_eq!(titled[1].1, "A picture");

        let based = links(r#"<base href="https://other/x/"><a href="y.pdf">"#, &page).0;
        assert_eq!(based[0].as_str(), "https://other/x/y.pdf");
    }

    /// A small site: `/` links to two sections, each to files and deeper
    /// pages.
    fn site(url: &str) -> Result<Page, String> {
        let html = |body: &str| {
            Ok(Page {
                url: url.to_string(),
                content_type: Some("text/html; charset=utf-8".into()),
                body: body.as_bytes().to_vec(),
            })
        };
        match url {
            "https://s/robots.txt" => Ok(Page {
                url: url.into(),
                content_type: Some("text/plain".into()),
                body: b"User-agent: *\nDisallow: /secret/\n".to_vec(),
            }),
            "https://s/" => html(
                r#"<a href="/a/">A</a><a href="/b/">B</a><a href="/secret/">S</a><a href="https://elsewhere/">E</a><a href="/top.pdf">t</a>"#,
            ),
            "https://s/a/" => {
                html(r#"<a href="one.pdf">1</a><a href="one.zip">z</a><a href="deep/">d</a><a href="/">home</a>"#)
            }
            "https://s/b/" => html(r#"<a href="two.pdf">2</a><a href="https://cdn.s/three%20x.pdf">3</a>"#),
            "https://s/a/deep/" => html(r#"<a href="four.pdf">4</a>"#),
            "https://s/secret/" => html(r#"<a href="hidden.pdf">h</a>"#),
            "https://elsewhere/" => html(r#"<a href="far.pdf">f</a>"#),
            _ => Err("404".into()),
        }
    }

    fn request(depth: u32) -> GrabRequest {
        GrabRequest { start: "https://s/".into(), depth, ..GrabRequest::default() }
    }

    #[test]
    fn crawls_within_depth_robots_and_site() {
        let fetched = std::sync::Mutex::new(Vec::new());
        let waits = RefCell::new(0);
        let out = crawl(
            &request(1),
            |u| {
                fetched.lock().unwrap().push(u.to_string());
                site(u)
            },
            |_| None,
            |_| *waits.borrow_mut() += 1,
            &AtomicBool::new(false),
            |_| {},
        );
        let names: Vec<&str> = out.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["top.pdf", "one.pdf", "two.pdf", "three x.pdf"]);
        assert_eq!(out.files[1].page, "https://s/a/");
        assert_eq!(out.pages, 3, "/, /a/, /b/");
        let fetched = fetched.into_inner().unwrap();
        assert!(!fetched.iter().any(|u| u.contains("secret")), "robots.txt is obeyed");
        assert!(!fetched.iter().any(|u| u.contains("elsewhere")), "same site only");
        assert!(!fetched.iter().any(|u| u.contains("deep")), "depth 1");
        assert!(!fetched.iter().any(|u| u.ends_with(".pdf")), "files are listed, not fetched");
        assert_eq!(fetched.iter().filter(|u| u.ends_with("robots.txt")).count(), 1);
        assert_eq!(*waits.borrow(), fetched.len() - 1, "a wait before every request but the first");
        assert!(out.stopped.unwrap().contains("robots.txt"));

        let deeper = crawl(&request(2), site, |_| None, |_| {}, &AtomicBool::new(false), |_| {});
        assert!(deeper.files.iter().any(|f| f.name == "four.pdf"));
        let anywhere = crawl(
            &GrabRequest { same_site: false, ..request(1) },
            site,
            |_| None,
            |_| {},
            &AtomicBool::new(false),
            |_| {},
        );
        assert!(anywhere.files.iter().any(|f| f.name == "far.pdf"));
    }

    #[test]
    fn budgets_and_cancel_stop_a_crawl() {
        let out =
            crawl(&GrabRequest { max_pages: 2, ..request(3) }, site, |_| None, |_| {}, &AtomicBool::new(false), |_| {});
        assert_eq!(out.pages, 2);
        assert!(out.stopped.unwrap().contains("2 pages"));

        let cancel = AtomicBool::new(false);
        let out = crawl(
            &request(3),
            site,
            |_| None,
            |_| {},
            &cancel,
            |o| {
                if o.pages == 1 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        );
        assert_eq!(out.pages, 1);
        assert_eq!(out.stopped.as_deref(), Some("Stopped."));
    }

    #[test]
    fn pages_can_be_read_several_at_once() {
        let none = AtomicBool::new(false);
        let one = crawl(&request(2), site, |_| None, |_| {}, &none, |_| {});
        let waits = RefCell::new(0);
        let many = crawl(
            &GrabRequest { explore_at_once: 3, ..request(2) },
            site,
            |_| None,
            |_| *waits.borrow_mut() += 1,
            &none,
            |_| {},
        );
        let names = |o: &Outcome| {
            let mut n: Vec<String> = o.files.iter().map(|f| f.name.clone()).collect();
            n.sort();
            n
        };
        assert_eq!(names(&one), names(&many));
        assert_eq!(one.pages, many.pages);
        // robots.txt, then /, then /a/ and /b/ together, then /a/deep/.
        assert_eq!(*waits.borrow(), 3, "one wait per batch of pages");
        let budget = crawl(
            &GrabRequest { explore_at_once: 5, max_pages: 2, ..request(2) },
            site,
            |_| None,
            |_| {},
            &none,
            |_| {},
        );
        assert_eq!(budget.pages, 2, "a batch stays within the page budget");
    }

    #[test]
    fn types_skipped_types_link_texts_and_this_site_only() {
        let none = AtomicBool::new(false);
        let every =
            GrabRequest { types: vec!["*".into()], skip_types: vec!["zip".into()], link_text: true, ..request(1) };
        let out = crawl(&every, site, |_| None, |_| {}, &none, |_| {});
        let names: Vec<&str> = out.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["top.pdf", "one.pdf", "two.pdf", "three x.pdf"], "every type but zip, and no pages");
        assert_eq!(out.files[1].text.as_deref(), Some("1"));
        assert!(out.explored >= 5, "the zip was looked at: {}", out.explored);
        let here = GrabRequest { files_this_site: true, ..request(1) };
        let out = crawl(&here, site, |_| None, |_| {}, &none, |_| {});
        assert!(!out.files.iter().any(|f| f.url.contains("cdn.s")), "cdn.s is another site");
        assert!(out.files.iter().all(|f| f.text.is_none()), "no texts unless asked");
    }

    #[test]
    fn the_whole_site_and_levels_on_other_sites() {
        let none = AtomicBool::new(false);
        let whole = crawl(&GrabRequest { whole_site: true, ..request(0) }, site, |_| None, |_| {}, &none, |_| {});
        assert!(whole.files.iter().any(|f| f.name == "four.pdf"), "depth 0, but the whole site");
        assert!(!whole.files.iter().any(|f| f.name == "far.pdf"), "and only the site");
        let other = GrabRequest { same_site: false, other_depth: Some(1), ..request(0) };
        let out = crawl(&other, site, |_| None, |_| {}, &none, |_| {});
        assert!(out.files.iter().any(|f| f.name == "far.pdf"), "one level on another site");
        assert!(!out.files.iter().any(|f| f.name == "one.pdf"), "depth 0 on the start site");
        let none_other = GrabRequest { same_site: false, other_depth: Some(0), ..request(1) };
        let out = crawl(&none_other, site, |_| None, |_| {}, &none, |_| {});
        assert!(!out.files.iter().any(|f| f.name == "far.pdf"));
    }

    #[test]
    fn found_files_go_to_one_category_or_one_folder() {
        let file = GrabFile {
            url: "https://s/a/one.pdf".into(),
            name: "one.pdf".into(),
            page: "https://s/a/".into(),
            size: None,
            text: Some("The manual".into()),
        };
        let settings = crate::settings::Settings {
            download_dir: Some(std::env::temp_dir().join("uidm-grab-dl").display().to_string()),
            ..crate::settings::Settings::default()
        };
        let d = download_for(&request(1), &file, &settings, None).unwrap();
        assert_eq!((d.referrer.as_deref(), d.page.as_deref()), (Some("https://s/a/"), Some("https://s/a/")));
        assert_eq!((d.category, d.folder, d.description), (None, None, None));
        let req = GrabRequest { save_to: SaveTo::Category("Music".into()), link_text: true, ..request(1) };
        let d = download_for(&req, &file, &settings, None).unwrap();
        assert_eq!((d.category.as_deref(), d.description.as_deref()), (Some("Music"), Some("The manual")));
        let music = settings.category_folder("Music", &crate::settings::default_downloads_dir()).display().to_string();
        assert_eq!(d.folder.as_deref(), Some(music.as_str()), "a pdf saved to Music goes to Music's folder");
        let req = GrabRequest { save_to: SaveTo::Folder(r"D:\grabbed".into()), ..request(1) };
        assert_eq!(download_for(&req, &file, &settings, None).unwrap().folder.as_deref(), Some(r"D:\grabbed"));
        assert!(GrabRequest { save_to: SaveTo::Folder("rel".into()), ..request(1) }.validate().is_err());
        assert!(GrabRequest { save_to: SaveTo::Category(" ".into()), ..request(1) }.validate().is_err());
        assert!(GrabRequest { explore_at_once: 11, ..request(1) }.validate().is_err());
        let json = serde_json::to_string(&SaveTo::Folder("D:\\x".into())).unwrap();
        assert_eq!(json, r#"{"to":"folder","name":"D:\\x"}"#);
        let old: GrabRequest = serde_json::from_str(r#"{"start":"https://s/","depth":1,"same_site":true,"types":["pdf"],"max_pages":5,"max_bytes":1048576,"delay_ms":1000}"#).unwrap();
        assert_eq!(
            (old.explore_at_once, old.download_at_once, old.ignore_popups, old.save_to),
            (1, 3, true, SaveTo::Categories)
        );
    }

    #[test]
    fn masks_match_whole_names_and_filters_add_to_lists() {
        assert!(name_matches("photo_*.jpg", "Photo_12.JPG"));
        assert!(!name_matches("photo_*.jpg", "cover.jpg"), "the name part counts");
        assert!(!name_matches("photo_*.jpg", "photo_1.jpg.html"));
        assert!(name_matches("img??.png", "img01.png") && !name_matches("img??.png", "img1.png"));
        assert!(name_matches("*.tar.gz", "src.tar.gz") && name_matches("a*b*c", "aXbYbZc"));
        assert_eq!(mask_entry(" *.JPG "), "jpg");
        assert_eq!(mask_entry(".pdf"), "pdf");
        assert_eq!(mask_entry("*.*"), "*");
        assert_eq!(mask_entry("Photo_*.jpg"), "photo_*.jpg");
        for t in ["tar.gz", ".TAR.GZ", "*.tar.gz"] {
            assert_eq!(mask_entry(t), "*.tar.gz", "{t}");
        }
        assert!(name_matches(&mask_entry(".tar.gz"), "src-1.0.tar.gz"));
        let masks = |m: &str| Filter { name: "x".into(), mask: m.into() }.types();
        assert_eq!(
            add_masks("pdf, zip", &masks("*.jpg;*.PDF;photo_*.png")),
            "pdf, zip, jpg, photo_*.png",
            "added, without repeats"
        );
        assert_eq!(add_masks("", &masks("*.gif")), "gif");
        assert_eq!(add_masks("*.pdf; pdf", &[]), "pdf");
        let none = AtomicBool::new(false);
        let names = |req: &GrabRequest| -> Vec<String> {
            crawl(req, site, |_| None, |_| {}, &none, |_| {}).files.into_iter().map(|f| f.name).collect()
        };
        // Only the name mask keeps a file: `one.zip` is not a pdf.
        assert_eq!(names(&GrabRequest { types: vec!["ONE.*".into()], ..request(1) }), ["one.pdf", "one.zip"]);
        // Bare extensions and `*.` masks still keep by type, as old projects saved them.
        let old = names(&request(1));
        assert_eq!(names(&GrabRequest { types: vec!["*.pdf".into()], ..request(1) }), old);
        assert_eq!(old, ["top.pdf", "one.pdf", "two.pdf", "three x.pdf"]);
        // A name mask leaves files out too, on the decoded name.
        let skip = GrabRequest { skip_types: vec!["t*.pdf".into(), "three ?.pdf".into()], ..request(1) };
        assert_eq!(names(&skip), ["one.pdf"]);
        let req = GrabRequest { types: vec!["one.*".into()], ..request(1) };
        assert_eq!(kept_locally(&req, &Url::parse("https://s/a/one.zip").unwrap()), Some(false));
        assert_eq!(kept_locally(&req, &Url::parse("https://cdn.x/two.zip").unwrap()), None);
    }

    #[test]
    fn filters_narrow_what_is_explored_and_kept() {
        let none = AtomicBool::new(false);
        let skip_b = GrabRequest { explore_skip: vec!["*/b/*".into()], ..request(1) };
        let out = crawl(&skip_b, site, |_| None, |_| {}, &none, |_| {});
        assert!(!out.files.iter().any(|f| f.name == "two.pdf"), "/b/ was not explored");
        let only_one = GrabRequest { files_only: vec!["one".into()], ..request(1) };
        let names: Vec<String> =
            crawl(&only_one, site, |_| None, |_| {}, &none, |_| {}).files.into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["one.pdf"]);
        let out = crawl(
            &GrabRequest { start: "https://s/a/".into(), no_parent: true, ..request(2) },
            site,
            |_| None,
            |_| {},
            &none,
            |_| {},
        );
        assert!(
            out.files.iter().any(|f| f.name == "four.pdf") && !out.files.iter().any(|f| f.name == "two.pdf"),
            "stays below /a/"
        );
    }

    #[test]
    fn sizes_filter_and_hide_duplicates() {
        let none = AtomicBool::new(false);
        let sizes = |u: &str| -> Option<u64> {
            match u.rsplit('/').next()? {
                "one.pdf" => Some(500),
                "two.pdf" => Some(5000),
                "top.pdf" => Some(50),
                _ => None,
            }
        };
        let req = GrabRequest { min_size: Some(100), max_size: Some(1000), ..request(1) };
        let names: Vec<String> =
            crawl(&req, site, sizes, |_| {}, &none, |_| {}).files.into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["one.pdf", "three x.pdf"], "unknown sizes stay");

        // The same name and size at two addresses shows once.
        let twice = |url: &str| -> Result<Page, String> {
            let body = match url {
                "https://s/" => r#"<a href="/x/a.pdf">1</a><a href="/y/a.pdf">2</a><a href="/z/a.pdf">3</a>"#,
                _ => return Err("404".into()),
            };
            Ok(Page { url: url.into(), content_type: Some("text/html".into()), body: body.as_bytes().to_vec() })
        };
        let size = |u: &str| Some(if u.contains("/z/") { 9 } else { 7 });
        let out = crawl(&request(0), twice, size, |_| {}, &none, |_| {});
        let urls: Vec<&str> = out.files.iter().map(|f| f.url.as_str()).collect();
        assert_eq!(urls, ["https://s/x/a.pdf", "https://s/z/a.pdf"]);
    }

    #[test]
    fn logout_pages_are_skipped_by_default() {
        let out = crawl(
            &request(1),
            |u| {
                if u == "https://s/" {
                    Ok(Page {
                        url: u.into(),
                        content_type: Some("text/html".into()),
                        body: br#"<a href="/account/logout">out</a><a href="/a/">a</a>"#.to_vec(),
                    })
                } else if u.contains("logout") {
                    panic!("opened the logout page");
                } else {
                    site(u)
                }
            },
            |_| None,
            |_| {},
            &AtomicBool::new(false),
            |_| {},
        );
        assert!(out.pages >= 2);
        assert!(on_start_site(&request(1), "https://s/a/"));
        assert!(!on_start_site(&request(1), "https://cdn.s/a"));
        assert!(on_start_site(&GrabRequest { subdomains: true, ..request(1) }, "https://cdn.s/a"));
    }

    #[test]
    fn projects_and_the_template_are_saved_and_loaded() {
        let dir = std::env::temp_dir().join(format!("uidm_grab_projects_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert!(recent_projects(&dir).is_empty());
        assert_eq!(template(&dir), GrabRequest::default());
        let p = Project {
            name: "Docs: manuals".into(),
            request: GrabRequest { depth: 3, ..request(1) },
            files: vec![GrabFile {
                url: "https://s/a.pdf".into(),
                name: "a.pdf".into(),
                page: "https://s/".into(),
                size: None,
                text: None,
            }],
            checked: vec![false],
            schedule: None,
            run: RunAction::Explore,
            download_new: false,
            when_done: uidm_domain::queue::WhenDone::Nothing,
            open_when_done: None,
            force_close: true,
            queue: uidm_domain::queue::MAIN_QUEUE,
            seen: Vec::new(),
            last_scheduled_day: None,
            last_scheduled_start: None,
            kind: uidm_domain::queue::QueueKind::Synchronize,
        };
        save_project(&p, &dir).unwrap();
        assert_eq!(load_project(&dir, "Docs: manuals").unwrap(), p);
        std::thread::sleep(Duration::from_millis(20));
        save_project(&Project { name: "Second".into(), ..p.clone() }, &dir).unwrap();
        assert_eq!(recent_projects(&dir), ["Second", "Docs: manuals"], "most recent first");
        save_template(&dir, &GrabRequest { max_pages: 7, ..request(1) }).unwrap();
        let t = template(&dir);
        assert_eq!((t.max_pages, t.start.as_str()), (7, ""), "a template has no start page");
        assert!(save_project(&Project { name: " ".into(), ..p }, &dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_files_load_from_anywhere_and_names_are_checked() {
        let dir = std::env::temp_dir().join(format!("uidm_grab_elsewhere_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = Project {
            name: "Docs".into(),
            request: request(1),
            files: Vec::new(),
            checked: Vec::new(),
            schedule: None,
            run: RunAction::Explore,
            download_new: false,
            when_done: uidm_domain::queue::WhenDone::Nothing,
            open_when_done: None,
            force_close: false,
            queue: uidm_domain::queue::MAIN_QUEUE,
            seen: Vec::new(),
            last_scheduled_day: None,
            last_scheduled_start: None,
            kind: uidm_domain::queue::QueueKind::OneTime,
        };
        assert!(name_is_unique(&dir, "Docs", None), "nothing saved yet");
        save_project(&p, &dir).unwrap();
        // A copy kept elsewhere, from an older version that said
        // download_new, loads and moves it into `run` as the folder's do.
        let elsewhere = dir.join("copied from work.json");
        let mut old = serde_json::to_value(&p).unwrap();
        old["download_new"] = true.into();
        fs::write(&elsewhere, serde_json::to_vec(&old).unwrap()).unwrap();
        let loaded = load_project_file(&elsewhere).unwrap();
        assert_eq!(loaded, Project { run: RunAction::ExploreAndDownload, ..p.clone() });
        fs::write(&elsewhere, b"{\"not\": \"a project\"}").unwrap();
        assert_eq!(load_project_file(&elsewhere).unwrap_err().kind(), io::ErrorKind::InvalidData);
        // serde would quote the value it choked on, which can be a secret.
        fs::write(&elsewhere, br#"{"name": "p", "request": {"start": "https://s/", "depth": "password=hunter2"}}"#)
            .unwrap();
        let e = load_project_file(&elsewhere).unwrap_err();
        assert!(!e.to_string().contains("hunter2"), "{e}");
        assert!(load_project_file(&dir.join("missing.json")).is_err());

        assert!(!name_is_unique(&dir, "Docs", None), "another project has it");
        assert!(!name_is_unique(&dir, "docs", None), "names are compared as files are");
        assert!(name_is_unique(&dir, "Docs", Some("Docs")), "its own saved copy");
        assert!(!name_is_unique(&dir, "Docs", Some("Other")));
        assert!(name_is_unique(&dir, "Manuals", Some("Docs")));
        assert!(!name_is_unique(&dir, " ", None), "a project needs a name");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn named_templates_are_saved_listed_and_deleted() {
        let dir = std::env::temp_dir().join(format!("uidm_grab_templates_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let names = |dir: &Path| templates(dir).into_iter().map(|t| t.name).collect::<Vec<_>>();
        let built_in = built_in_templates().len();
        assert_eq!(templates(&dir).len(), built_in);
        assert!(templates(&dir).iter().all(|t| t.built_in && t.request.validate().is_err()), "no start page");
        save_named_template(&dir, "My PDFs", &GrabRequest { depth: 4, ..request(1) }).unwrap();
        save_named_template(&dir, "a: zips", &request(1)).unwrap();
        assert_eq!(names(&dir)[built_in..], ["a: zips", "My PDFs"]);
        let mine = templates(&dir).into_iter().find(|t| t.name == "My PDFs").unwrap();
        assert_eq!((mine.request.depth, mine.request.start.as_str(), mine.built_in), (4, "", false));
        assert!(save_named_template(&dir, "pictures from a site", &request(1)).is_err());
        assert!(save_named_template(&dir, " ", &request(1)).is_err());
        delete_template(&dir, "My PDFs").unwrap();
        assert_eq!(names(&dir).len(), built_in + 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn filters_turn_masks_into_types_and_are_kept() {
        let f = |mask: &str| Filter { name: "x".into(), mask: mask.into() }.types();
        assert_eq!(f("*.JPG; *.png,gif"), ["jpg", "png", "gif"]);
        assert_eq!(f("*.*"), ["*"]);
        assert_eq!(f("*"), ["*"]);
        assert!(f(" ; ").is_empty());
        assert_eq!(f("Photo_*.JPG;*.tar.gz;*.jp?"), ["photo_*.jpg", "*.tar.gz", "*.jp?"], "name masks stay whole");
        let dir = std::env::temp_dir().join(format!("uidm_grab_filters_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(load_filters(&dir), default_filters());
        let mine = vec![Filter { name: "Manuals".into(), mask: "*.pdf".into() }];
        save_filters(&dir, &mine).unwrap();
        assert_eq!(load_filters(&dir), mine);
        // Every default keeps something a crawl accepts.
        assert!(default_filters().iter().all(|f| GrabRequest { types: f.types(), ..request(1) }.validate().is_ok()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn older_projects_that_added_new_files_now_download_them() {
        let dir = std::env::temp_dir().join(format!("uidm_grab_old_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let json = r#"{"name":"old","request":{"start":"https://s/","depth":1,"same_site":true,"types":["pdf"],"max_pages":5,"max_bytes":1048576,"delay_ms":1000},"download_new":true}"#;
        fs::write(project_file(&dir, "old"), json).unwrap();
        let p = load_project(&dir, "old").unwrap();
        assert_eq!(p.run, RunAction::ExploreAndDownload);
        assert!(!serde_json::to_string(&p).unwrap().contains("download_new"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_run_adds_only_what_earlier_runs_did_not_find() {
        let file = |u: &str| GrabFile {
            url: u.into(),
            name: u.rsplit('/').next().unwrap().into(),
            page: "https://s/".into(),
            size: None,
            text: None,
        };
        let mut p: Project = serde_json::from_str(r#"{"name":"p","request":{"start":"https://s/","depth":1,"same_site":true,"types":["pdf"],"max_pages":5,"max_bytes":1048576,"delay_ms":1000}}"#).unwrap();
        assert_eq!(p.queue, uidm_domain::queue::MAIN_QUEUE, "an older project file loads");
        let first = p.record_run(vec![file("https://s/a.pdf"), file("https://s/b.pdf")]);
        assert_eq!(first.len(), 2);
        let second = p.record_run(vec![file("https://s/b.pdf"), file("https://s/c.pdf")]);
        assert_eq!(second.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["c.pdf"]);
        assert_eq!(p.checked, [false, true], "only the new one is ticked");
        assert_eq!(p.seen.len(), 3);
    }

    #[test]
    fn a_synchronizing_project_checks_what_earlier_runs_found() {
        let file = |u: &str| GrabFile {
            url: u.into(),
            name: u.rsplit('/').next().unwrap().into(),
            page: "https://s/".into(),
            size: None,
            text: None,
        };
        let mut p: Project = serde_json::from_str(r#"{"name":"p","request":{"start":"https://s/","depth":1,"same_site":true,"types":["pdf"],"max_pages":5,"max_bytes":1048576,"delay_ms":1000},"run":"explore_and_download"}"#).unwrap();
        assert_eq!(p.kind, uidm_domain::queue::QueueKind::OneTime, "an older project file is one-time");
        p.record_run(vec![file("https://s/a.pdf"), file("https://s/b.pdf")]);
        let now = [file("https://s/b.pdf"), file("https://s/c.pdf")];
        assert!(p.to_resync(&now).is_empty(), "a one-time project checks nothing");
        p.kind = uidm_domain::queue::QueueKind::Synchronize;
        assert_eq!(p.to_resync(&now), ["https://s/b.pdf"], "only what an earlier run found");
        p.record_run(now.to_vec());
        p.run = RunAction::DownloadChecked;
        assert_eq!(p.to_resync(&[]), ["https://s/c.pdf"], "the ticked files");
        p.run = RunAction::Explore;
        assert!(p.to_resync(&now).is_empty(), "exploring downloads nothing");
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"kind\":\"synchronize\""), "the type is saved: {json}");
    }

    #[test]
    fn saved_pages_link_to_the_local_copies() {
        let root = Path::new(r"C:\site");
        let page = Url::parse("https://s/docs/index.html").unwrap();
        let here = mirror_path(root, &page, true);
        let req = GrabRequest { start: "https://s/".into(), ..GrabRequest::default() };
        let html = r##"<a href="guide.pdf">g</a> <a href='/top/'>t</a> <a href="https://other/x.pdf">o</a> <a href="#part">p</a> <img src="pic.png">"##;
        let out = rewrite_links(html, &page, &here, root, |u| kept_locally(&req, u));
        assert!(out.contains(r#"href="guide.pdf""#), "same folder");
        assert!(out.contains("href='../top/index.html'"));
        assert!(out.contains(r#"href="../../other/x.pdf""#), "a kept file on another site");
        assert!(out.contains(r#"src="pic.png""#), "not a kept type: untouched");
        assert!(out.contains(r##"href="index.html#part""##));

        let file = GrabFile {
            url: "https://s/docs/guide.pdf".into(),
            name: "guide.pdf".into(),
            page: page.to_string(),
            size: None,
            text: None,
        };
        let settings = crate::settings::Settings::default();
        let dir = std::env::temp_dir().join(format!("uidm_mirror_{}", std::process::id()));
        let mirrored = GrabRequest {
            mirror: Some(Mirror { folder: dir.display().to_string(), pages: true, overwrite: false }),
            ..req.clone()
        };
        let d = download_for(&mirrored, &file, &settings, None).unwrap();
        assert_eq!(d.folder.as_deref(), Some(dir.join("s").join("docs").display().to_string().as_str()));
        assert_eq!(d.filename.as_deref(), Some("guide.pdf"));
        std::fs::create_dir_all(dir.join("s").join("docs")).unwrap();
        std::fs::write(dir.join("s").join("docs").join("guide.pdf"), b"x").unwrap();
        assert!(download_for(&mirrored, &file, &settings, None).is_none(), "an existing file is skipped");
        let replace =
            GrabRequest { mirror: Some(Mirror { overwrite: true, ..mirrored.mirror.clone().unwrap() }), ..req.clone() };
        assert!(download_for(&replace, &file, &settings, None).is_some());
        assert_eq!(download_for(&req, &file, &settings, None).unwrap().folder, None, "no mirror: the category decides");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pasted_cookies_are_cleaned_and_kept_sealed() {
        assert_eq!(clean_cookies("Cookie: a=1;\n b=2 ;"), Some("a=1; b=2".into()));
        assert_eq!(clean_cookies("  \n"), None);
        let req = GrabRequest {
            start: "https://s/".into(),
            login_url: Some("https://s/login".into()),
            login_cookies: Some("session=topsecret".into()),
            ..GrabRequest::default()
        };
        let dir = std::env::temp_dir().join(format!("uidm-grab-cookie-project-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let project: Project = serde_json::from_value(serde_json::json!({ "name": "Sealed", "request": req })).unwrap();
        save_project(&project, &dir).unwrap();
        let file = fs::read_to_string(project_file(&dir, "Sealed")).unwrap();
        assert!(!file.contains("topsecret"), "the project file does not keep them readable");
        assert_eq!(load_project(&dir, "Sealed").unwrap().request.login_cookies, req.login_cookies);
        // Sealed elsewhere: dropped, not an error.
        let sealed = file[file.find("\"login_cookies\": \"").unwrap() + 18..].split('"').next().unwrap().to_string();
        fs::write(project_file(&dir, "Sealed"), file.replace(&sealed, "AAAA")).unwrap();
        assert_eq!(load_project(&dir, "Sealed").unwrap().request.login_cookies, None);
        let _ = fs::remove_dir_all(&dir);
        // Templates never keep them.
        let dir = std::env::temp_dir().join(format!("uidm-grab-cookie-template-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        save_template(&dir, &req).unwrap();
        assert_eq!(template(&dir).login_cookies, None);
        assert_eq!(template(&dir).login_url.as_deref(), Some("https://s/login"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn normal_saves_keep_relative_subfolders_and_can_overwrite() {
        let start = "https://s/docs/";
        let at = |u: &str| relative_subfolder(start, &Url::parse(u).unwrap());
        assert_eq!(at("https://s/docs/a.pdf"), PathBuf::new());
        assert_eq!(at("https://s/docs/v1/en/a.pdf"), Path::new("v1").join("en"));
        assert_eq!(
            at("https://s/other/a.pdf"),
            Path::new("s").join("other"),
            "outside the start folder: host and path"
        );
        assert_eq!(at("https://cdn.net/x/a.pdf"), Path::new("cdn.net").join("x"));
        let root = std::env::temp_dir().join("uidm-grab-sub");
        let settings = crate::settings::Settings {
            download_dir: Some(root.display().to_string()),
            ..crate::settings::Settings::default()
        };
        let file = GrabFile {
            url: "https://s/docs/v1/a.pdf".into(),
            name: "a.pdf".into(),
            page: start.into(),
            size: None,
            text: None,
        };
        let folder = root.join("grabbed");
        let req = GrabRequest {
            start: start.into(),
            save_to: SaveTo::Folder(folder.display().to_string()),
            ..GrabRequest::default()
        };
        let d = download_for(&req, &file, &settings, None).unwrap();
        assert_eq!((d.folder.clone(), d.file_exists), (Some(folder.display().to_string()), None), "off by default");
        let req = GrabRequest { subfolders: true, overwrite: true, ..req };
        let d = download_for(&req, &file, &settings, None).unwrap();
        assert_eq!(d.folder, Some(folder.join("v1").display().to_string()));
        assert_eq!(d.file_exists, Some(crate::settings::FileExists::Overwrite));
        // By category: under the folder the file's category has.
        let req = GrabRequest { save_to: SaveTo::Categories, ..req };
        let d = download_for(&req, &file, &settings, None).unwrap();
        let category = settings.folders_for(&file.url, &crate::settings::default_downloads_dir()).for_name("a.pdf");
        assert_eq!(d.folder, Some(category.join("v1").display().to_string()));
        // A file in the start folder keeps the category's own choice.
        let top = GrabFile { url: "https://s/docs/b.pdf".into(), ..file };
        assert_eq!(download_for(&req, &top, &settings, None).unwrap().folder, None);
    }

    #[test]
    fn found_files_on_the_login_site_carry_its_cookies() {
        let settings = crate::settings::Settings::default();
        let req = GrabRequest {
            start: "https://docs.example.com/".into(),
            login_url: Some("https://example.com/login".into()),
            ..GrabRequest::default()
        };
        let file = |url: &str| GrabFile {
            url: url.into(),
            name: "a.pdf".into(),
            page: req.start.clone(),
            size: None,
            text: None,
        };
        let cookies = |url: &str| download_for(&req, &file(url), &settings, Some("s=2")).unwrap().cookies;
        assert_eq!(cookies("https://example.com/files/a.pdf"), Some("s=2".into()));
        assert_eq!(cookies("https://docs.example.com/a.pdf"), None, "the start site is another host");
        assert_eq!(cookies("https://cdn.other.net/a.pdf"), None);
        assert_eq!(download_for(&req, &file("https://example.com/a.pdf"), &settings, None).unwrap().cookies, None);
    }
}
