//! The remote-control page's HTTPS server: its listeners, one thread per
//! connection (at most `MAX_CONNECTIONS`), and what each address answers.
//!
//! The page itself (`/`, `/app.js`, `/app.css`) holds no data and is served
//! to anyone who can reach the port, so a phone can load it to pair. Every
//! `/api/` request but `/api/pair` needs a paired device's credential in
//! its `Authorization` header, which the `PhoneGate` checks; requests from
//! another site's page are refused (`Origin`, `Sec-Fetch-Site`), and a
//! request that changes something must be JSON, which a plain form cannot
//! send. The daemon's token is nowhere here: requests reach the daemon
//! through the gate, inside the process.

use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};
use udm_application::pairing::{PhoneError, PhoneGate, PhoneReply, PhoneRequest};
use udm_application::remote::{Listing, RemoteError};
use udm_domain::address::Address;
use udm_domain::download::{DownloadEntry, Status};
use udm_domain::id::DownloadId;
use udm_domain::pairing::PairingError;

use super::http::{read_request, write_response, ReadError, Request, Response};

/// Connections served at once; more wait for the system to refuse them.
const MAX_CONNECTIONS: usize = 32;
/// Requests on one connection before it is closed.
const MAX_REQUESTS: usize = 200;
/// How long one read may wait.
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a whole request may take to arrive, or a connection may wait
/// for the next.
const REQUEST_DEADLINE: Duration = Duration::from_secs(30);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// How often a listener looks at its stop flag.
const ACCEPT_POLL: Duration = Duration::from_millis(100);

const INDEX: &str = include_str!("page/index.html");
const APP_JS: &str = include_str!("page/app.js");
const APP_CSS: &str = include_str!("page/app.css");

/// Listeners being served, until `stop`.
pub struct Served {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    /// The addresses listened on.
    pub bound: Vec<SocketAddr>,
}

impl Served {
    /// Stop listening and close every connection at its next request.
    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads {
            let _ = t.join();
        }
    }
}

/// Listen on every one of `addresses` and serve the page over TLS with
/// `acceptor`. Fails, listening on none, if any address cannot be had.
pub fn start(
    addresses: &[SocketAddr],
    acceptor: native_tls::TlsAcceptor,
    gate: Arc<dyn PhoneGate>,
) -> Result<Served, (SocketAddr, io::Error)> {
    let mut listeners = Vec::new();
    for &address in addresses {
        let listener = TcpListener::bind(address).map_err(|e| (address, e))?;
        listener.set_nonblocking(true).map_err(|e| (address, e))?;
        listeners.push(listener);
    }
    let stop = Arc::new(AtomicBool::new(false));
    let open = Arc::new(AtomicUsize::new(0));
    let acceptor = Arc::new(acceptor);
    let mut bound = Vec::new();
    let mut threads = Vec::new();
    for listener in listeners {
        bound.push(listener.local_addr().map_err(|e| (addresses[0], e))?);
        let (stop, open, acceptor, gate) =
            (Arc::clone(&stop), Arc::clone(&open), Arc::clone(&acceptor), Arc::clone(&gate));
        threads.push(thread::spawn(move || accept(listener, &stop, &open, &acceptor, &gate)));
    }
    Ok(Served { stop, threads, bound })
}

fn accept(
    listener: TcpListener,
    stop: &Arc<AtomicBool>,
    open: &Arc<AtomicUsize>,
    acceptor: &Arc<native_tls::TlsAcceptor>,
    gate: &Arc<dyn PhoneGate>,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer)) => {
                if open.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                    open.fetch_sub(1, Ordering::SeqCst);
                    continue;
                }
                let (stop, open, acceptor, gate) =
                    (Arc::clone(stop), Arc::clone(open), Arc::clone(acceptor), Arc::clone(gate));
                thread::spawn(move || {
                    connection(stream, peer, &stop, &acceptor, &*gate);
                    open.fetch_sub(1, Ordering::SeqCst);
                });
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL),
            Err(e) => {
                warn!(error = %e, "remote control: accept failed");
                thread::sleep(ACCEPT_POLL);
            }
        }
    }
}

fn connection(
    stream: TcpStream,
    peer: SocketAddr,
    stop: &AtomicBool,
    acceptor: &native_tls::TlsAcceptor,
    gate: &dyn PhoneGate,
) {
    let ready = stream
        .set_nonblocking(false)
        .and_then(|()| stream.set_read_timeout(Some(READ_TIMEOUT)))
        .and_then(|()| stream.set_write_timeout(Some(WRITE_TIMEOUT)));
    if ready.is_err() {
        return;
    }
    let _ = stream.set_nodelay(true);
    let mut tls = match acceptor.accept(stream) {
        Ok(tls) => tls,
        // A browser that has not accepted the certificate yet drops the
        // handshake; that is no error worth a line.
        Err(e) => {
            tracing::debug!(peer = %peer, error = %e, "remote control: no TLS handshake");
            return;
        }
    };
    let mut buffer = Vec::new();
    for served in 0..MAX_REQUESTS {
        let request = match read_request(&mut tls, &mut buffer, Instant::now() + REQUEST_DEADLINE) {
            Ok(r) => r,
            Err(ReadError::TooLarge) => {
                let _ = write_response(&mut tls, &error(413, "too_large"), false, false);
                break;
            }
            Err(ReadError::Bad(_)) => {
                let _ = write_response(&mut tls, &error(400, "bad_request"), false, false);
                break;
            }
            Err(_) => break,
        };
        // Turned off meanwhile: nothing more is answered.
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let response = route(&request, peer.ip(), gate);
        let keep = request.keep_alive && served + 1 < MAX_REQUESTS;
        if write_response(&mut tls, &response, keep, request.method == "HEAD").is_err() || !keep {
            break;
        }
    }
    let _ = tls.shutdown();
}

fn error(status: u16, kind: &str) -> Response {
    Response::json(status, &json!({ "error": kind }))
}

/// The answer to `request` from `peer`.
pub fn route(request: &Request, peer: IpAddr, gate: &dyn PhoneGate) -> Response {
    let get = matches!(request.method.as_str(), "GET" | "HEAD");
    match request.path.as_str() {
        "/" | "/index.html" if get => Response::new(200, "text/html; charset=utf-8", INDEX),
        "/app.js" if get => Response::new(200, "text/javascript; charset=utf-8", APP_JS),
        "/app.css" if get => Response::new(200, "text/css; charset=utf-8", APP_CSS),
        "/" | "/index.html" | "/app.js" | "/app.css" => error(405, "method"),
        path if path.starts_with("/api/") => api(request, path, peer, gate),
        _ => error(404, "not_found"),
    }
}

/// Whether `request` comes from the page itself, not another site's page
/// (a cross-site request forgery) or a frame.
fn same_origin(request: &Request) -> bool {
    if let Some(site) = request.header("sec-fetch-site") {
        if site != "same-origin" && site != "none" {
            return false;
        }
    }
    match (request.header("origin"), request.header("host")) {
        (None, _) => true,
        (Some(origin), Some(host)) => origin.eq_ignore_ascii_case(&format!("https://{host}")),
        (Some(_), None) => false,
    }
}

#[derive(Deserialize)]
struct PairBody {
    code: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct AddBody {
    url: String,
}

#[derive(Deserialize)]
struct IdBody {
    id: u64,
    #[serde(default)]
    delete_files: bool,
}

fn api(request: &Request, path: &str, peer: IpAddr, gate: &dyn PhoneGate) -> Response {
    if !same_origin(request) {
        return error(403, "cross_origin");
    }
    let post = request.method == "POST";
    if post && !request.header("content-type").is_some_and(|t| t.to_ascii_lowercase().starts_with("application/json")) {
        return error(415, "not_json");
    }
    let body = |r: &Request| serde_json::from_slice::<serde_json::Value>(&r.body).ok();
    if path == "/api/pair" {
        if !post {
            return error(405, "method");
        }
        let Some(pair) = body(request).and_then(|v| serde_json::from_value::<PairBody>(v).ok()) else {
            return error(400, "bad_request");
        };
        return match gate.pair(&pair.code, &pair.name, peer) {
            Ok(credential) => {
                Response::json(200, &json!({ "device": credential.device.get(), "credential": credential.to_text() }))
            }
            Err(e) => refused(&PhoneError::Pairing(e)),
        };
    }
    let Some(credential) = request.header("authorization").and_then(|a| a.strip_prefix("Bearer ")) else {
        return error(401, "not_paired");
    };
    let phone = match (request.method.as_str(), path) {
        ("GET", "/api/downloads") => PhoneRequest::List,
        ("POST", "/api/add") => match body(request).and_then(|v| serde_json::from_value::<AddBody>(v).ok()) {
            Some(add) => match Address::parse(add.url.trim()) {
                Ok(url) => PhoneRequest::Add(url),
                Err(_) => return error(400, "not_a_link"),
            },
            None => return error(400, "bad_request"),
        },
        ("POST", "/api/pause" | "/api/resume" | "/api/remove") => {
            let Some(target) = body(request).and_then(|v| serde_json::from_value::<IdBody>(v).ok()) else {
                return error(400, "bad_request");
            };
            let id = DownloadId::new(target.id);
            match path {
                "/api/pause" => PhoneRequest::Pause(id),
                "/api/resume" => PhoneRequest::Resume(id),
                _ => PhoneRequest::Remove { id, delete_files: target.delete_files },
            }
        }
        ("POST", "/api/unpair") => PhoneRequest::Unpair,
        (_, "/api/downloads" | "/api/add" | "/api/pause" | "/api/resume" | "/api/remove" | "/api/unpair") => {
            return error(405, "method")
        }
        _ => return error(404, "not_found"),
    };
    match gate.run(credential, peer, phone) {
        Ok(PhoneReply::Listing(listing)) => Response::json(200, &listing_json(&listing)),
        Ok(PhoneReply::Added(id)) => Response::json(200, &json!({ "id": id.get() })),
        Ok(PhoneReply::Done) => Response::json(200, &json!({})),
        Err(e) => refused(&e),
    }
}

/// A refusal as the page reads it: a kind it translates, and for the
/// daemon's own refusals its (English) words.
fn refused(e: &PhoneError) -> Response {
    let (status, kind) = match e {
        PhoneError::Pairing(PairingError::WrongCode) => (403, "wrong_code"),
        PhoneError::Pairing(PairingError::TooManyTries { wait }) => {
            let mut r = Response::json(429, &json!({ "error": "too_many_tries", "wait": wait }));
            r.retry_after = Some(*wait);
            return r;
        }
        PhoneError::Pairing(PairingError::NotPaired | PairingError::NoDevice(_)) => (401, "not_paired"),
        PhoneError::Pairing(PairingError::TooManyDevices) => (409, "too_many_devices"),
        PhoneError::Pairing(PairingError::NotListening) => (503, "off"),
        PhoneError::Pairing(PairingError::Store(_)) => (500, "store"),
        PhoneError::Remote(RemoteError::NoDownload(_)) => (404, "no_download"),
        PhoneError::Remote(RemoteError::NotRunning(_)) => (409, "not_running"),
        PhoneError::Remote(RemoteError::Refused(why)) => {
            return Response::json(409, &json!({ "error": "refused", "message": why.to_string() }));
        }
        PhoneError::Remote(RemoteError::NoQueue(_)) => (409, "refused"),
        PhoneError::Remote(_) => (503, "daemon"),
    };
    error(status, kind)
}

/// The list as the page shows it: each download's name, server, state and
/// progress. Not its full address, folder, referrer, login or anything
/// else the daemon keeps about it.
pub fn listing_json(listing: &Listing) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = listing.downloads.iter().map(row).collect();
    json!({ "downloads": rows })
}

fn row(e: &DownloadEntry) -> serde_json::Value {
    let url = e.url.as_str();
    let name = e
        .path
        .as_deref()
        .and_then(|p| p.rsplit(['/', '\\']).next())
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| udm_domain::filename::name_from_url(url).as_str().to_string());
    let host = url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();
    json!({
        "id": e.id.get(),
        "name": name,
        "host": host,
        "status": status(e.status),
        "done": e.done.get(),
        "total": e.total.map(|t| t.get()),
        "error": e.error.as_deref().map(|why| crate::logging::redact(why).into_owned()),
    })
}

fn status(s: Status) -> &'static str {
    match s {
        Status::Queued => "queued",
        Status::Downloading => "downloading",
        Status::Done => "done",
        Status::Failed => "failed",
        Status::Canceled => "canceled",
        Status::Interrupted => "interrupted",
        Status::Confirming => "confirming",
        Status::Waiting => "waiting",
    }
}

/// Log that the page listens at `bound`.
pub fn log_listening(bound: &[SocketAddr]) {
    let at: Vec<String> = bound.iter().map(|a| a.to_string()).collect();
    info!(addresses = %at.join(", "), "remote control listening");
}
