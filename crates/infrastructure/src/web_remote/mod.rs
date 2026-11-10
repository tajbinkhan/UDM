//! Remote control from a phone on the home network: an HTTPS page the
//! daemon serves while the user has it on in Options (see
//! `docs/remote-control.md`).
//!
//! `WebRemote` follows the settings: it listens on the port and addresses
//! they name while remote control is on, and stops listening as soon as it
//! is turned off. It is the `RemoteListener` the daemon service asks for
//! the window. The page's requests go to the application's `PhoneGate`
//! (pairing and the paired devices' requests); `certificate` makes and
//! keeps the page's self-signed certificate, `devices` keeps the paired
//! devices, `http` reads and writes the requests and `serve` answers them.

pub mod certificate;
pub mod devices;
pub mod http;
pub mod serve;

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use pullbay_application::pairing::PhoneGate;
use pullbay_domain::pairing::{ListenerState, PairingCode, RemoteFault};
use pullbay_domain::ports::RemoteListener;
use pullbay_domain::settings::Settings;
use pullbay_domain::sync::LockExt;
use tracing::{info, warn};

use certificate::Certificate;
use serve::Served;

/// How often the settings are looked at.
const WATCH_EVERY: Duration = Duration::from_millis(500);
/// How long after failing to listen it tries again.
const RETRY_AFTER: Duration = Duration::from_secs(15);

/// Where and on which port to listen.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Wanted {
    port: u16,
    /// Empty: every interface.
    addresses: Vec<IpAddr>,
}

impl Wanted {
    /// What `settings` ask for, if remote control is on.
    fn of(settings: &Settings) -> Option<Wanted> {
        settings.remote_enabled.then(|| Wanted {
            port: settings.remote_port,
            addresses: settings.remote_addresses.iter().filter_map(|a| a.trim().parse().ok()).collect(),
        })
    }

    fn sockets(&self) -> Vec<SocketAddr> {
        if self.addresses.is_empty() {
            vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), self.port)]
        } else {
            self.addresses.iter().map(|&a| SocketAddr::new(a, self.port)).collect()
        }
    }
}

#[derive(Default)]
struct Inner {
    /// What was last applied.
    wanted: Option<Wanted>,
    served: Option<Served>,
    state: ListenerState,
    certificate: Option<Certificate>,
    failed_at: Option<Instant>,
}

/// The remote-control page's listener, following the settings.
pub struct WebRemote {
    /// Where the certificate is kept; without it nothing is served.
    dir: Option<PathBuf>,
    gate: OnceLock<Arc<dyn PhoneGate>>,
    inner: Mutex<Inner>,
}

impl WebRemote {
    /// A listener keeping its certificate in `dir` (`paths::remote_dir`),
    /// not listening yet.
    pub fn new(dir: Option<PathBuf>) -> Self {
        WebRemote { dir, gate: OnceLock::new(), inner: Mutex::default() }
    }

    /// Its state. Every change replaces a field whole, so a lock a panic
    /// poisoned still holds a usable value.
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock_or_recover()
    }

    /// Hand the page's requests to `gate` from now on. Until then nothing
    /// is served.
    pub fn serve_with(&self, gate: Arc<dyn PhoneGate>) {
        let _ = self.gate.set(gate);
    }

    /// Listen as `settings` say: start, move or stop. A listener that
    /// failed is tried again after a while.
    pub fn apply(&self, settings: &Settings) {
        let wanted = Wanted::of(settings);
        let mut inner = self.inner();
        let retry = inner.served.is_none() && inner.failed_at.is_some_and(|at| at.elapsed() >= RETRY_AFTER);
        if inner.wanted == wanted && !retry {
            return;
        }
        if let Some(served) = inner.served.take() {
            served.stop();
            info!("remote control stopped listening");
        }
        let fingerprint = inner.state.fingerprint.take();
        inner.state = ListenerState { fingerprint, ..ListenerState::default() };
        inner.failed_at = None;
        inner.wanted = wanted.clone();
        let (Some(wanted), Some(gate)) = (wanted, self.gate.get()) else {
            return;
        };
        if let Err(fault) = self.listen(&mut inner, &wanted, Arc::clone(gate)) {
            warn!(fault = ?fault, "remote control could not listen");
            inner.state.fault = Some(fault);
            inner.failed_at = Some(Instant::now());
        }
    }

    fn listen(&self, inner: &mut Inner, wanted: &Wanted, gate: Arc<dyn PhoneGate>) -> Result<(), RemoteFault> {
        let dir = self.dir.as_ref().ok_or_else(|| RemoteFault::Certificate("no data folder".into()))?;
        let certificate = match inner.certificate.clone() {
            Some(c) => c,
            None => {
                let mut names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
                for address in primary_address().into_iter().chain(wanted.addresses.iter().copied()) {
                    let name = address.to_string();
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
                let now = crate::state::unix_now();
                Certificate::load_or_make(dir, &names, now).map_err(|e| RemoteFault::Certificate(e.to_string()))?
            }
        };
        inner.state.fingerprint = Some(certificate.fingerprint.clone());
        inner.certificate = Some(certificate.clone());
        let identity = certificate.identity().map_err(|e| RemoteFault::Certificate(e.to_string()))?;
        let acceptor = native_tls::TlsAcceptor::builder(identity)
            .min_protocol_version(Some(native_tls::Protocol::Tlsv12))
            .build()
            .map_err(|e| RemoteFault::Certificate(e.to_string()))?;
        let served = serve::start(&wanted.sockets(), acceptor, gate).map_err(|(at, e)| fault(at, &e))?;
        serve::log_listening(&served.bound);
        inner.state.listening = served.bound.iter().map(|a| a.to_string()).collect();
        inner.state.page = page_host(&served.bound).map(|host| format!("https://{host}/"));
        inner.served = Some(served);
        Ok(())
    }

    /// Apply the settings `settings` returns now and whenever they change,
    /// on a thread of its own.
    pub fn watch(self: &Arc<Self>, settings: impl Fn() -> Settings + Send + 'static) {
        let remote = Arc::clone(self);
        thread::spawn(move || loop {
            remote.apply(&settings());
            thread::sleep(WATCH_EVERY);
        });
    }

    /// Stop listening, as when remote control is turned off.
    pub fn stop(&self) {
        self.apply(&Settings { remote_enabled: false, ..Settings::default() });
    }
}

impl RemoteListener for WebRemote {
    fn state(&self) -> ListenerState {
        self.inner().state.clone()
    }

    fn link(&self, code: &PairingCode) -> Option<String> {
        // In the fragment, which the browser does not send to the server,
        // so the code is in no request line or log; the page reads it.
        self.inner().state.page.as_ref().map(|page| format!("{page}#pair={}", code.as_str()))
    }
}

/// The fault that `e` was when listening at `at`.
fn fault(at: SocketAddr, e: &std::io::Error) -> RemoteFault {
    let address = at.to_string();
    match e.kind() {
        std::io::ErrorKind::AddrInUse => RemoteFault::PortInUse { address },
        std::io::ErrorKind::AddrNotAvailable => RemoteFault::AddressUnavailable { address },
        std::io::ErrorKind::PermissionDenied => RemoteFault::NoPermission { address },
        _ => RemoteFault::Other(format!("{address}: {e}")),
    }
}

/// The address and port a phone on the network opens: the first address
/// listened on that is not the loopback, or this computer's own address on
/// the network when listening on every interface.
fn page_host(bound: &[SocketAddr]) -> Option<String> {
    let at = bound.iter().find(|a| !a.ip().is_loopback()).or(bound.first())?;
    let ip =
        if at.ip().is_unspecified() { primary_address().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST)) } else { at.ip() };
    Some(SocketAddr::new(ip, at.port()).to_string())
}

/// This computer's address on its network: the one the system would send
/// from to the internet. Nothing is sent; a UDP socket only picks a route.
pub fn primary_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    // A documentation address (TEST-NET-1): routed like any other, never
    // contacted.
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    socket.local_addr().ok().map(|a| a.ip()).filter(|ip| !ip.is_unspecified())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::atomic::{AtomicU64, Ordering};

    use pullbay_application::daemon::{ClientId, Command, Event, Failure, Reply, Requests};
    use pullbay_application::pairing::{PairingDesk, Phone};
    use pullbay_domain::download::{DownloadEntry, Status};
    use pullbay_domain::id::DownloadId;
    use pullbay_domain::ports::{PairedDevices, Timestamps};
    use pullbay_domain::queue::Queue;
    use serde_json::{json, Value};

    use super::devices::{DeviceFile, OsKeys};

    /// Stands for the daemon's token and every other secret a row can
    /// hold; nothing the page answers may carry it.
    const PLANTED: &str = "planted-value-the-page-must-never-show";

    /// A clock the tests move.
    #[derive(Default)]
    struct Clock(AtomicU64);

    impl Timestamps for Clock {
        fn unix_now(&self) -> u64 {
            1_750_000_000 + self.0.load(Ordering::SeqCst)
        }
    }

    /// The daemon as the page reaches it: two downloads, one running, whose
    /// rows carry secrets the page must not show.
    #[derive(Default)]
    struct Daemon {
        handled: Mutex<Vec<Command>>,
    }

    fn rows() -> Vec<DownloadEntry> {
        let row = |id: u64, status: Status, done: u64| {
            serde_json::from_value::<DownloadEntry>(json!({
                "id": id, "url": format!("https://someone:{PLANTED}@dl.example/files/{id}.iso?sig={PLANTED}"),
                "status": status, "connections": 4, "done": done, "total": 1000, "resumable": true,
                "path": format!("/home/someone/Downloads/{id}.iso"), "referrer": PLANTED,
                "description": PLANTED, "login_user": PLANTED, "partial": format!("/tmp/{PLANTED}"),
            }))
            .unwrap()
        };
        vec![row(1, Status::Downloading, 500), row(2, Status::Canceled, 200)]
    }

    impl Requests for Daemon {
        fn handle(&self, _client: ClientId, command: Command, reply: &Reply) {
            self.handled.lock().unwrap().push(command.clone());
            match command {
                Command::Ping => {
                    reply.send(Event::Pong);
                }
                Command::List => {
                    reply.send(Event::Downloads { items: rows() });
                    reply.send(Event::Queues { items: vec![Queue::main()] });
                }
                Command::Add(d) => {
                    reply.send(Event::Ack { id: DownloadId::new(3), url: d.url, status: "accepted".into() });
                }
                Command::Retry { id } => {
                    reply.send(Event::Ack { id: DownloadId::new(id.get() + 10), url: "u".into(), status: "ok".into() });
                }
                Command::Remove { id, .. } if id.get() == 1 => {
                    reply.send(Event::Error { error: Failure::Other("it is running".into()) });
                }
                _ => {}
            }
        }

        fn gone(&self, _client: ClientId) {}
    }

    struct Rig {
        remote: Arc<WebRemote>,
        desk: Arc<PairingDesk>,
        daemon: Arc<Daemon>,
        clock: Arc<Clock>,
        devices: PathBuf,
        dir: PathBuf,
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            self.remote.stop();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn settings(on: bool) -> Settings {
        Settings {
            remote_enabled: on,
            remote_port: 0,
            remote_addresses: vec!["127.0.0.1".into()],
            ..Settings::default()
        }
    }

    fn rig(name: &str) -> Rig {
        let dir = std::env::temp_dir().join(format!("pullbay-web-remote-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let devices = dir.join("devices.bin");
        let clock = Arc::new(Clock::default());
        let desk = Arc::new(PairingDesk::open(
            Arc::new(DeviceFile::at(devices.clone())),
            Arc::new(OsKeys),
            Arc::clone(&clock) as _,
        ));
        let daemon = Arc::new(Daemon::default());
        let ids = AtomicU64::new(1 << 40);
        let phone = Phone::new(
            Arc::clone(&desk),
            Arc::clone(&daemon) as _,
            Box::new(move || ClientId::new(ids.fetch_add(1, Ordering::SeqCst))),
        );
        let remote = Arc::new(WebRemote::new(Some(dir.clone())));
        remote.serve_with(Arc::new(phone));
        remote.apply(&settings(true));
        Rig { remote, desk, daemon, clock, devices, dir }
    }

    impl Rig {
        fn address(&self) -> String {
            self.remote.state().listening[0].clone()
        }
    }

    /// One HTTPS request; the status, the head and the body.
    fn send(at: &str, raw: &str) -> (u16, String, String) {
        let connector = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .build()
            .unwrap();
        let tcp = TcpStream::connect(at).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        let mut tls = connector.connect("localhost", tcp).unwrap();
        tls.write_all(raw.as_bytes()).unwrap();
        let mut out = Vec::new();
        let _ = tls.read_to_end(&mut out);
        let text = String::from_utf8_lossy(&out).into_owned();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let status = head.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        (status, head.to_string(), body.to_string())
    }

    fn get(at: &str, path: &str, credential: Option<&str>) -> (u16, Value) {
        let auth = credential.map(|c| format!("Authorization: Bearer {c}\r\n")).unwrap_or_default();
        let (status, _, body) =
            send(at, &format!("GET {path} HTTP/1.1\r\nHost: {at}\r\n{auth}Connection: close\r\n\r\n"));
        (status, serde_json::from_str(&body).unwrap_or(Value::Null))
    }

    fn post(at: &str, path: &str, credential: Option<&str>, body: &Value) -> (u16, Value) {
        let auth = credential.map(|c| format!("Authorization: Bearer {c}\r\n")).unwrap_or_default();
        let body = body.to_string();
        let raw = format!(
            "POST {path} HTTP/1.1\r\nHost: {at}\r\nOrigin: https://{at}\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (status, _, body) = send(at, &raw);
        (status, serde_json::from_str(&body).unwrap_or(Value::Null))
    }

    fn pair(rig: &Rig) -> String {
        let code = rig.desk.start().unwrap().code;
        let at = rig.address();
        let (status, answer) = post(&at, "/api/pair", None, &json!({ "code": code.as_str(), "name": "Test phone" }));
        assert_eq!(status, 200, "{answer}");
        answer["credential"].as_str().unwrap().to_string()
    }

    #[test]
    fn nothing_listens_until_turned_on_and_turning_off_stops_it() {
        let remote = WebRemote::new(None);
        remote.apply(&Settings::default());
        assert_eq!(remote.state(), ListenerState::default());

        let rig = rig("onoff");
        let at = rig.address();
        assert!(at.starts_with("127.0.0.1:"));
        let state = rig.remote.state();
        assert!(state.page.as_deref().is_some_and(|p| p.starts_with("https://127.0.0.1:")));
        assert_eq!(state.fingerprint.as_ref().map(String::len), Some(95));
        let code = rig.desk.start().unwrap().code;
        let link = rig.remote.link(&code).unwrap();
        assert!(link.ends_with(&format!("/#pair={}", code.as_str())), "the code is in the fragment only");

        rig.remote.apply(&settings(false));
        assert!(rig.remote.state().listening.is_empty());
        assert!(rig.remote.link(&code).is_none());
        assert!(TcpStream::connect(&at).is_err(), "nothing listens once it is off");
    }

    #[test]
    fn a_port_in_use_is_reported() {
        let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let rig = rig("taken");
        let busy = Settings { remote_port: taken.local_addr().unwrap().port(), ..settings(true) };
        rig.remote.apply(&busy);
        assert!(matches!(rig.remote.state().fault, Some(RemoteFault::PortInUse { .. })));
    }

    #[test]
    fn the_page_is_served_with_its_safety_headers() {
        let rig = rig("page");
        let at = rig.address();
        let (status, head, body) = send(&at, &format!("GET / HTTP/1.1\r\nHost: {at}\r\nConnection: close\r\n\r\n"));
        assert_eq!(status, 200);
        assert!(body.contains("<script src=\"/app.js\""));
        for header in [
            "Content-Security-Policy: default-src 'none'",
            "frame-ancestors 'none'",
            "Cache-Control: no-store",
            "X-Content-Type-Options: nosniff",
        ] {
            assert!(head.contains(header), "{header}");
        }
        assert!(!head.to_ascii_lowercase().contains("access-control-allow-origin"));
        assert_eq!(send(&at, &format!("GET /app.js HTTP/1.1\r\nHost: {at}\r\n\r\n")).0, 200);
        assert_eq!(send(&at, &format!("GET /secret HTTP/1.1\r\nHost: {at}\r\n\r\n")).0, 404);
        assert_eq!(send(&at, &format!("DELETE / HTTP/1.1\r\nHost: {at}\r\n\r\n")).0, 405);
    }

    #[test]
    fn nothing_but_pairing_answers_without_a_paired_credential() {
        let rig = rig("unpaired");
        let at = rig.address();
        assert_eq!(get(&at, "/api/downloads", None).0, 401);
        assert_eq!(get(&at, "/api/downloads", Some("1.abc")).0, 401);
        assert_eq!(get(&at, "/api/downloads", Some(&format!("1.{}", "0".repeat(64)))).0, 401);
        assert_eq!(post(&at, "/api/add", None, &json!({"url": "https://x/y"})).0, 401);
        assert_eq!(post(&at, "/api/pause", Some("garbage"), &json!({"id": 1})).0, 401);
        assert!(rig.daemon.handled.lock().unwrap().is_empty(), "nothing reached the daemon");
    }

    #[test]
    fn a_phone_pairs_once_and_wrong_or_expired_codes_are_refused() {
        let rig = rig("pair");
        let at = rig.address();
        let (status, answer) = post(&at, "/api/pair", None, &json!({"code": "AAAA-BBBB", "name": "x"}));
        assert_eq!((status, answer["error"].as_str()), (403, Some("wrong_code")));

        let code = rig.desk.start().unwrap().code;
        rig.clock.0.store(301, Ordering::SeqCst);
        let (status, _) = post(&at, "/api/pair", None, &json!({"code": code.as_str()}));
        assert_eq!(status, 403, "an expired code");

        let credential = pair(&rig);
        assert_eq!(get(&at, "/api/downloads", Some(&credential)).0, 200);
        let devices = DeviceFile::at(rig.devices.clone()).load().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name.as_str(), "Test phone");
        assert!(!std::fs::read(&rig.devices).unwrap().windows(10).any(|w| w == &credential.as_bytes()[2..12]));
    }

    #[test]
    fn wrong_codes_and_credentials_are_rate_limited() {
        let rig = rig("limits");
        let at = rig.address();
        let code = rig.desk.start().unwrap().code;
        for _ in 0..5 {
            assert_eq!(post(&at, "/api/pair", None, &json!({"code": "0000"})).0, 403);
        }
        let (status, answer) = post(&at, "/api/pair", None, &json!({"code": code.as_str()}));
        assert_eq!((status, answer["error"].as_str()), (429, Some("too_many_tries")), "even the right code");
        assert!(answer["wait"].as_u64().is_some_and(|w| w > 0));
        for _ in 0..10 {
            assert_eq!(get(&at, "/api/downloads", Some("1.00")).0, 401);
        }
        assert_eq!(get(&at, "/api/downloads", Some("1.00")).0, 429);
    }

    #[test]
    fn each_action_goes_to_the_daemon_and_files_stay_unless_asked() {
        let rig = rig("actions");
        let at = rig.address();
        let me = pair(&rig);
        let (status, list) = get(&at, "/api/downloads", Some(&me));
        assert_eq!(status, 200);
        assert_eq!(
            list["downloads"][0],
            json!({
                "id": 1, "name": "1.iso", "host": "dl.example", "status": "downloading",
                "done": 500, "total": 1000, "error": null,
            })
        );
        assert_eq!(
            post(&at, "/api/add", Some(&me), &json!({"url": "https://dl.example/new.iso"})),
            (200, json!({"id": 3}))
        );
        assert_eq!(post(&at, "/api/add", Some(&me), &json!({"url": "file:///etc/passwd"})).0, 400);
        assert_eq!(post(&at, "/api/pause", Some(&me), &json!({"id": 1})).0, 200);
        assert_eq!(post(&at, "/api/pause", Some(&me), &json!({"id": 2})), (409, json!({"error": "not_running"})));
        assert_eq!(post(&at, "/api/resume", Some(&me), &json!({"id": 2})).0, 200);
        assert_eq!(post(&at, "/api/remove", Some(&me), &json!({"id": 2})).0, 200);
        let (status, refused) = post(&at, "/api/remove", Some(&me), &json!({"id": 1}));
        assert_eq!((status, refused["error"].as_str()), (409, Some("refused")));
        assert_eq!(post(&at, "/api/remove", Some(&me), &json!({"id": 9})).0, 404);
        assert_eq!(post(&at, "/api/remove", Some(&me), &json!({"id": 2, "delete_files": true})).0, 200);

        let handled = rig.daemon.handled.lock().unwrap().clone();
        let add = handled.iter().find_map(|c| match c {
            Command::Add(d) => Some(d.clone()),
            _ => None,
        });
        let add = add.unwrap();
        assert_eq!((add.url.as_str(), add.folder, add.filename), ("https://dl.example/new.iso", None, None));
        assert!(handled.iter().any(|c| matches!(c, Command::Cancel { id } if id.get() == 1)));
        assert!(handled.iter().any(|c| matches!(c, Command::Retry { id } if id.get() == 2)));
        let removes: Vec<bool> = handled
            .iter()
            .filter_map(|c| match c {
                Command::Remove { delete_files, .. } => Some(*delete_files),
                _ => None,
            })
            .collect();
        assert_eq!(removes, [false, false, true], "the files went only when asked");
    }

    #[test]
    fn a_removed_device_is_refused_at_once() {
        let rig = rig("removed");
        let at = rig.address();
        let me = pair(&rig);
        let other = pair(&rig);
        assert_eq!(get(&at, "/api/downloads", Some(&me)).0, 200);
        rig.desk.remove(pullbay_domain::id::DeviceId::new(1)).unwrap();
        assert_eq!(get(&at, "/api/downloads", Some(&me)).0, 401);
        assert_eq!(post(&at, "/api/unpair", Some(&other), &json!({})).0, 200);
        assert_eq!(get(&at, "/api/downloads", Some(&other)).0, 401);
        assert!(rig.desk.devices().is_empty());
    }

    #[test]
    fn other_sites_pages_and_forms_are_refused() {
        let rig = rig("csrf");
        let at = rig.address();
        let me = pair(&rig);
        let body = r#"{"url":"https://dl.example/x"}"#;
        let raw = |headers: &str| {
            format!(
                "POST /api/add HTTP/1.1\r\nHost: {at}\r\nAuthorization: Bearer {me}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        };
        assert_eq!(send(&at, &raw("Origin: https://evil.example\r\nContent-Type: application/json\r\n")).0, 403);
        assert_eq!(send(&at, &raw("Sec-Fetch-Site: cross-site\r\nContent-Type: application/json\r\n")).0, 403);
        assert_eq!(send(&at, &raw("Content-Type: application/x-www-form-urlencoded\r\n")).0, 415);
        assert_eq!(send(&at, &raw("Content-Type: application/json\r\n")).0, 200);
    }

    #[test]
    fn no_response_carries_the_token_a_secret_or_the_pairing_code() {
        let rig = rig("secrets");
        let at = rig.address();
        let code = rig.desk.start().unwrap().code;
        let body = json!({ "code": code.as_str(), "name": "p" }).to_string();
        let (_, head, paired) = send(&at, &format!(
            "POST /api/pair HTTP/1.1\r\nHost: {at}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
        let credential = serde_json::from_str::<Value>(&paired).unwrap()["credential"].as_str().unwrap().to_string();
        let mut seen = vec![head, paired.replace(&credential, "")];
        for path in ["/", "/app.js", "/app.css", "/api/downloads"] {
            let auth = format!("Authorization: Bearer {credential}\r\n");
            let (_, head, body) =
                send(&at, &format!("GET {path} HTTP/1.1\r\nHost: {at}\r\n{auth}Connection: close\r\n\r\n"));
            seen.push(head);
            seen.push(body);
        }
        for text in seen {
            assert!(!text.contains(PLANTED), "{text}");
            assert!(!text.contains(code.as_str()));
            assert!(!text.contains("/home/someone"), "no folders");
        }
    }

    /// The certificate passes a verifying client that trusts it, by name.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_certificate_verifies_for_its_names() {
        let rig = rig("verify");
        let at = rig.address();
        let pem = std::fs::read(rig.dir.join("certificate.json")).unwrap();
        let pem: Value = serde_json::from_slice(&pem).unwrap();
        let root = native_tls::Certificate::from_pem(pem["pem"].as_str().unwrap().as_bytes()).unwrap();
        let connector = native_tls::TlsConnector::builder().add_root_certificate(root).build().unwrap();
        let tcp = TcpStream::connect(&at).unwrap();
        let mut tls = connector.connect("localhost", tcp).unwrap();
        tls.write_all(format!("GET / HTTP/1.1\r\nHost: {at}\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
        let mut out = String::new();
        let _ = tls.read_to_string(&mut out);
        assert!(out.starts_with("HTTP/1.1 200"));
    }
}
