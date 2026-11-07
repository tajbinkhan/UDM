//! `Crawler`, the `SiteCrawler` adapter: one crawl with the engine, the
//! settings' proxy and logins (and the project's own login for its start
//! site), the browser's cookies and rendered pages when the request asks,
//! and short waits so a stop is noticed quickly.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tracing::{info, warn};
use uidm_domain::download::Download;
use uidm_domain::grab::{GrabRequest, Outcome, Page};
use uidm_domain::ports::{Browser, SiteCrawler};

use crate::engine;
use crate::grab;
use crate::live_settings::LiveSettings;
use crate::net::Login;
use crate::transfers::Transfers;

/// How the site grabber names itself to servers.
const GRABBER_AGENT: &str = "Mozilla/5.0 (compatible; UIDM site grabber; obeys robots.txt)";

/// Crawls with the engine.
pub struct Crawler {
    transfers: Arc<Transfers>,
    settings: Arc<LiveSettings>,
    browser: Arc<dyn Browser>,
}

impl Crawler {
    pub fn new(transfers: Arc<Transfers>, settings: Arc<LiveSettings>, browser: Arc<dyn Browser>) -> Self {
        Crawler { transfers, settings, browser }
    }
}

/// Keep `page` under the request's mirror folder, when it keeps pages.
fn mirror(req: &GrabRequest, page: &Page) {
    if let Some(m) = req.mirror.as_ref().filter(|m| m.pages) {
        if let Err(e) = grab::save_page(req, m, page) {
            warn!(error = %e, "could not save");
        }
    }
}

impl SiteCrawler for Crawler {
    fn crawl(
        &self,
        req: &GrabRequest,
        login: Option<&str>,
        stop: &AtomicBool,
        progress: &mut dyn FnMut(&Outcome),
    ) -> Outcome {
        let settings = self.settings.lock().clone();
        let start = Download { url: req.start.clone(), ..Download::default() };
        let mut net = self.transfers.net_for(&start, &settings);
        if let (Some(user), Ok(url)) =
            (req.login_user.as_deref().map(str::trim).filter(|u| !u.is_empty()), url::Url::parse(&req.start))
        {
            if let Some(host) = url.host_str() {
                net.logins.push(Login {
                    site: host.to_string(),
                    username: user.to_string(),
                    password: grab::project_password(&req.start, user).unwrap_or_default(),
                    // Given for this project's start site.
                    plain: true,
                });
            }
        }
        let headers = [("User-Agent".to_string(), GRABBER_AGENT.to_string())];
        let cookies = if req.browser_cookies { self.browser.cookies(&req.start) } else { None };
        // Each site's cookies go to that site's pages only.
        let page_headers = |url: &str| {
            let mut h = headers.to_vec();
            if let Some(c) = uidm_domain::grab::crawl_cookies(req, url, cookies.as_deref(), login) {
                h.push(("Cookie".to_string(), c));
            }
            h
        };
        // Rendering needs the browser extension connected.
        let render = AtomicBool::new(req.render_scripts && self.browser.connected());
        let fetch = |url: &str| {
            // A page as the browser shows it, when asked and the browser
            // can; otherwise as the server sends it.
            let rendering = render.load(Ordering::Relaxed) && !url.ends_with("/robots.txt");
            let rendered = rendering.then(|| self.browser.render(url)).flatten();
            // A browser that does not answer once is not asked again.
            if rendering && rendered.is_none() {
                info!(url = %url, "the browser did not render the page; reading pages as sent");
                render.store(false, Ordering::Relaxed);
            }
            if let Some(mut shown) = rendered {
                // Pages the page's scripts opened in new windows are links
                // like the rest, unless the crawl ignores them.
                if !req.ignore_popups {
                    for p in shown.popups.iter().filter(|p| p.starts_with("http://") || p.starts_with("https://")) {
                        shown.html.push_str(&format!("<a href=\"{}\"></a>", p.replace('"', "%22")));
                    }
                }
                let mut body = shown.html.into_bytes();
                body.truncate(uidm_domain::grab::MAX_PAGE_BYTES as usize);
                let page = Page { url: shown.url, content_type: Some("text/html".into()), body };
                mirror(req, &page);
                return Ok(page);
            }
            let page = engine::fetch(url, &page_headers(url), &net, uidm_domain::grab::MAX_PAGE_BYTES)
                .map(|f| Page { url: f.url, content_type: f.content_type, body: f.body })
                .map_err(|e| e.to_string())?;
            mirror(req, &page);
            Ok(page)
        };
        // Wait in short steps, so stopping is quick.
        let wait = |d: Duration| {
            let until = Instant::now() + d;
            while Instant::now() < until && !stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(100).min(until - Instant::now()));
            }
        };
        // A file's size, for the size filters and duplicates.
        let size = |url: &str| engine::inspect(url, None, &page_headers(url), &net).ok().and_then(|i| i.total);
        grab::crawl(req, fetch, size, wait, stop, progress)
    }
}
