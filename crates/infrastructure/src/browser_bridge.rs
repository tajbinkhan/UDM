//! `BrowserBridge`, the `Browser` adapter: it asks the browser extension
//! by broadcasting a request to the clients (only the extension answers)
//! and waits for the answer the daemon hands back.

use pullbay_domain::sync::LockExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use pullbay_application::daemon::Event;
use pullbay_domain::grab::RenderedPage;
use pullbay_domain::ports::Browser;
use tracing::info;

use crate::server::Clients;

/// How long the browser has to send its cookies.
const COOKIES_WAIT: Duration = Duration::from_secs(5);
/// How long the browser has to render a page. The extension gives up on a
/// page after 30 seconds itself.
const RENDER_WAIT: Duration = Duration::from_secs(40);

/// Requests waiting for the browser, by request number.
type Waits<T> = Mutex<HashMap<u64, Sender<Option<T>>>>;

/// Asks the browser extension through the connected clients.
pub struct BrowserBridge {
    clients: Arc<Clients>,
    next: AtomicU64,
    cookies: Waits<String>,
    pages: Waits<RenderedPage>,
}

/// Lock `m`. The maps change by single inserts and removals, so one a
/// panicking thread left behind is still usable.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock_or_recover()
}

impl BrowserBridge {
    pub fn new(clients: Arc<Clients>) -> Self {
        BrowserBridge { clients, next: AtomicU64::new(1), cookies: Mutex::default(), pages: Mutex::default() }
    }

    /// Send the request `ask` makes of a fresh request number and wait up
    /// to `wait` for its answer in `waits`.
    fn ask<T>(&self, waits: &Waits<T>, wait: Duration, ask: impl FnOnce(u64) -> Event) -> Option<T> {
        let request = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        lock(waits).insert(request, tx);
        self.clients.broadcast(ask(request));
        let got = rx.recv_timeout(wait).ok().flatten();
        lock(waits).remove(&request);
        got
    }
}

impl Browser for BrowserBridge {
    fn connected(&self) -> bool {
        self.clients.extension_connected()
    }

    fn cookies(&self, url: &str) -> Option<String> {
        // Without the extension nobody answers: no need to wait.
        if !self.connected() {
            return None;
        }
        let got =
            self.ask(&self.cookies, COOKIES_WAIT, |request| Event::CookiesWanted { request, url: url.to_string() });
        info!(found = got.is_some(), "browser cookies");
        got.filter(|c| !c.is_empty())
    }

    fn render(&self, url: &str) -> Option<RenderedPage> {
        self.ask(&self.pages, RENDER_WAIT, |request| Event::RenderPage { request, url: url.to_string() })
    }

    fn answered_cookies(&self, request: u64, cookies: Option<String>) {
        if let Some(tx) = lock(&self.cookies).remove(&request) {
            let _ = tx.send(cookies);
        }
    }

    fn answered_render(&self, request: u64, page: Option<RenderedPage>) {
        if let Some(tx) = lock(&self.pages).remove(&request) {
            let _ = tx.send(page);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pullbay_application::daemon::ClientId;
    use std::thread;

    use crate::protocol::Outgoing;

    fn clients() -> Arc<Clients> {
        Arc::new(Clients::new(Box::new(|_| {}), Box::new(|| {})))
    }

    #[test]
    fn without_the_extension_nobody_is_asked() {
        let clients = clients();
        let (_window, frames) = clients.connect(ClientId::new(1));
        clients.add_window(ClientId::new(1));
        let bridge = BrowserBridge::new(clients);
        assert_eq!(bridge.cookies("https://site.net/"), None);
        assert!(frames.try_recv().is_err(), "nothing was sent");
    }

    #[test]
    fn the_browsers_answer_comes_back_to_the_asker() {
        let clients = clients();
        let (_extension, frames) = clients.connect(ClientId::new(2));
        let bridge = Arc::new(BrowserBridge::new(clients));
        let asker = Arc::clone(&bridge);
        let asking = thread::spawn(move || asker.cookies("https://site.net/"));
        let frame = frames.recv_timeout(Duration::from_secs(5)).unwrap();
        let Ok(Outgoing::CookiesWanted { request, url }) = serde_json::from_slice(&frame) else {
            panic!("not a cookie request");
        };
        assert_eq!(url, "https://site.net/");
        // An answer to another request is not this one's.
        bridge.answered_cookies(request + 100, Some("x=1".into()));
        bridge.answered_cookies(request, Some("sid=7".into()));
        assert_eq!(asking.join().unwrap(), Some("sid=7".into()));

        let asker = Arc::clone(&bridge);
        let rendering = thread::spawn(move || asker.render("https://site.net/app"));
        let frame = frames.recv_timeout(Duration::from_secs(5)).unwrap();
        let Ok(Outgoing::RenderPage { request, .. }) = serde_json::from_slice(&frame) else {
            panic!("not a render request");
        };
        let page = RenderedPage { html: "<a>".into(), url: "https://site.net/app#x".into(), popups: Vec::new() };
        bridge.answered_render(request, Some(page.clone()));
        assert_eq!(rendering.join().unwrap(), Some(page));
    }
}
