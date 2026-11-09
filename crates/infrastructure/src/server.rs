//! The daemon's side of its loopback connections.
//!
//! `Server::bind` takes the daemon's address, which is also the
//! single-instance lock, and publishes the token only once it holds it (see
//! `ipc`). `Server::serve` then accepts clients, runs the handshake with
//! each, and reads its frames on a thread of its own. Each client gets one
//! writer thread, so replies and broadcasts never interleave inside a frame.
//!
//! `Clients` is the list of connected clients: which are windows and which
//! the browser extension, and how a message reaches them. It is also the
//! store's `StoreEvents` listener, so every change to the download list goes
//! to every client.
//!
//! Each frame is read as a `Command` (see `wire`) and handed to the
//! application's `Requests` port, which the daemon wires in; its answers
//! come back as `Event`s on a `Reply`, which go out as frames.

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Instant;
use uidm_domain::sync::LockExt;

use tracing::{info, warn};

use uidm_application::daemon::{ClientList, Event, Failure, Reply, Requests};

use crate::app::MAX_MESSAGE_BYTES;
use crate::front::Windows;
use crate::ipc::{self, Handshake, AUTH_TIMEOUT, DAEMON_LABEL, HOST_LABEL};
use crate::protocol::{read_message, write_frame, Incoming, Outgoing};
use crate::state::unix_now;
use crate::store::{Ended, StoreEvents};
use crate::wire;

/// A client's frames, drained by its writer thread.
type Frames = Sender<Vec<u8>>;

/// Frame `msg` and send it on `frames`. False when the client is gone.
fn send_wire(frames: &Frames, msg: &Outgoing) -> bool {
    serde_json::to_vec(msg).is_ok_and(|frame| frames.send(frame).is_ok())
}

/// A reply that sends each event to `frames`.
fn reply_to(frames: Frames) -> Reply {
    Reply::new(move |event| wire::outgoing(event).is_some_and(|msg| send_wire(&frames, &msg)))
}

/// A reply and the receiver of its framed messages, for a client that is
/// not in the list, such as one in a test.
pub fn reply_channel() -> (Reply, Receiver<Vec<u8>>) {
    let (tx, rx) = channel();
    (reply_to(tx), rx)
}

/// The connected clients, and how the store's changes reach them: each
/// change is framed and sent to every client, and how a download ended goes
/// to the listener given at start.
pub struct Clients {
    /// Connected clients by id.
    clients: Mutex<HashMap<u64, Frames>>,
    /// Clients that said they are a desktop window.
    windows: Mutex<HashSet<u64>>,
    /// Clients that said hello as the browser extension does, with no
    /// `client` (the host strips any).
    browsers: Mutex<HashSet<u64>>,
    /// When a browser extension last connected or sent anything, in
    /// seconds since 1970, for Options' link state.
    browser_seen: Mutex<Option<u64>>,
    /// Hears how each download ended.
    ended: Box<dyn Fn(Ended) + Send + Sync>,
    /// Wakes the scheduler when a download ends or a queue changes.
    wake: Box<dyn Fn() + Send + Sync>,
}

/// Lock `m`. Every lock below guards a map or set changed by single inserts
/// and removals, or a plain value, so one poisoned by a panicking thread
/// still holds a usable value.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock_or_recover()
}

impl Clients {
    /// No clients yet. `ended` hears how each download ended, and `wake`
    /// wakes the scheduler.
    pub fn new(ended: Box<dyn Fn(Ended) + Send + Sync>, wake: Box<dyn Fn() + Send + Sync>) -> Self {
        Clients {
            clients: Mutex::default(),
            windows: Mutex::default(),
            browsers: Mutex::default(),
            browser_seen: Mutex::default(),
            ended,
            wake,
        }
    }

    /// Add client `id`. Returns its reply and the receiver of every frame
    /// it is sent, replies and broadcasts alike.
    pub fn connect(&self, id: u64) -> (Reply, Receiver<Vec<u8>>) {
        let (tx, rx) = channel();
        lock(&self.clients).insert(id, tx.clone());
        (reply_to(tx), rx)
    }

    /// Whether no client is connected.
    pub fn is_empty(&self) -> bool {
        lock(&self.clients).is_empty()
    }

    /// Note that client `id` is a desktop window.
    pub fn add_window(&self, id: u64) {
        lock(&self.windows).insert(id);
    }

    /// Whether client `id` is a desktop window.
    pub fn is_window(&self, id: u64) -> bool {
        lock(&self.windows).contains(&id)
    }

    /// Whether a desktop window is connected.
    pub fn window_open(&self) -> bool {
        !lock(&self.windows).is_empty()
    }

    /// Whether a client other than a window is connected: the browser
    /// extension, through its host.
    pub fn extension_connected(&self) -> bool {
        let windows = lock(&self.windows).len();
        lock(&self.clients).len() > windows
    }

    /// Send `event` to the desktop windows only.
    pub fn to_windows(&self, event: Event) {
        let Some(frame) = wire::outgoing(event).and_then(|msg| serde_json::to_vec(&msg).ok()) else {
            return;
        };
        let windows = lock(&self.windows);
        for (_, frames) in lock(&self.clients).iter().filter(|(id, _)| windows.contains(id)) {
            let _ = frames.send(frame.clone());
        }
    }

    /// Send `event` to every client.
    pub fn broadcast(&self, event: Event) {
        if let Some(msg) = wire::outgoing(event) {
            self.wire_broadcast(&msg);
        }
    }

    fn wire_broadcast(&self, msg: &Outgoing) {
        let Ok(frame) = serde_json::to_vec(msg) else {
            return;
        };
        // Dropping a client whose writer is gone keeps the map tidy.
        lock(&self.clients).retain(|_, frames| frames.send(frame.clone()).is_ok());
    }

    /// Whether a browser extension is connected, and when one last was.
    pub fn browser_link(&self) -> Event {
        Event::BrowserLink { connected: !lock(&self.browsers).is_empty(), last_seen: self.browser_seen() }
    }

    /// When a browser extension last connected or sent anything.
    pub fn browser_seen(&self) -> Option<u64> {
        *lock(&self.browser_seen)
    }

    /// Note that client `id` is a browser extension; the windows hear of
    /// it when it is the only one.
    pub fn browser_connected(&self, id: u64) {
        *lock(&self.browser_seen) = Some(unix_now());
        let first = {
            let mut browsers = lock(&self.browsers);
            browsers.insert(id) && browsers.len() == 1
        };
        if first {
            self.to_windows(self.browser_link());
        }
    }

    /// Client `id` sent something: a browser extension is still there.
    pub fn heard_from(&self, id: u64) {
        if lock(&self.browsers).contains(&id) {
            *lock(&self.browser_seen) = Some(unix_now());
        }
    }

    /// Forget client `id`; when it was the last browser extension, the
    /// windows hear when it went.
    pub fn client_gone(&self, id: u64) {
        lock(&self.clients).remove(&id);
        lock(&self.windows).remove(&id);
        let last = {
            let mut browsers = lock(&self.browsers);
            browsers.remove(&id) && browsers.is_empty()
        };
        if last {
            *lock(&self.browser_seen) = Some(unix_now());
            self.to_windows(self.browser_link());
        }
    }

    /// Set when a browser extension was last seen, for tests.
    #[doc(hidden)]
    pub fn set_browser_seen(&self, at: Option<u64>) {
        *lock(&self.browser_seen) = at;
    }
}

impl ClientList for Clients {
    fn is_empty(&self) -> bool {
        Clients::is_empty(self)
    }

    fn window_open(&self) -> bool {
        Clients::window_open(self)
    }

    fn is_window(&self, client: u64) -> bool {
        Clients::is_window(self, client)
    }

    fn add_window(&self, client: u64) {
        Clients::add_window(self, client);
    }

    fn browser_connected(&self, client: u64) {
        Clients::browser_connected(self, client);
    }

    fn browser_link(&self) -> Event {
        Clients::browser_link(self)
    }

    fn broadcast(&self, event: Event) {
        Clients::broadcast(self, event);
    }

    fn to_windows(&self, event: Event) {
        Clients::to_windows(self, event);
    }
}

impl Windows for Clients {
    fn window_open(&self) -> bool {
        Clients::window_open(self)
    }
}

impl StoreEvents for Clients {
    fn announce(&self, event: &Outgoing) {
        self.wire_broadcast(event);
    }

    fn ended(&self, how: Ended) {
        (self.ended)(how);
    }

    fn wake(&self) {
        (self.wake)();
    }
}

/// Handle one frame from client `client`: decode it, note that the client
/// is still there, and hand it to `requests`. A frame that is not a request
/// gets an `error` back.
pub fn receive(clients: &Clients, requests: &dyn Requests, client: u64, raw: &[u8], reply: &Reply) {
    let msg: Incoming = match serde_json::from_slice(raw) {
        Ok(msg) => msg,
        Err(e) => {
            reply.send(Event::Error { error: Failure::InvalidMessage(e.to_string()) });
            return;
        }
    };
    clients.heard_from(client);
    requests.handle(client, wire::command(msg), reply);
}

/// The daemon's listening socket and the token clients must prove they
/// hold.
pub struct Server {
    listener: TcpListener,
    token: Arc<str>,
}

impl Server {
    /// Bind the daemon's address and publish a fresh token. `None` when the
    /// address is taken, which means another daemon is running.
    pub fn bind() -> io::Result<Option<Server>> {
        let addr = ipc::daemon_addr();
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => l,
            Err(e) => {
                info!(addr = %addr, error = %e, "address unavailable; another daemon is likely running");
                return Ok(None);
            }
        };
        info!(addr = %addr, "listening");
        // Publish the token only after winning the bind. A daemon that lost
        // the bind must not overwrite the running daemon's token.
        let token = ipc::new_token()?;
        ipc::write_token(&token)?;
        Ok(Some(Server { listener, token: token.into() }))
    }

    /// Serve clients until the listener fails, each on its own thread.
    pub fn serve(self, clients: Arc<Clients>, requests: Arc<dyn Requests>) {
        static NEXT_CLIENT: AtomicU64 = AtomicU64::new(1);
        for conn in self.listener.incoming() {
            match conn {
                Ok(stream) => {
                    let id = NEXT_CLIENT.fetch_add(1, Ordering::Relaxed);
                    let (clients, requests, token) =
                        (Arc::clone(&clients), Arc::clone(&requests), Arc::clone(&self.token));
                    thread::spawn(move || handle_client(stream, id, &clients, &*requests, &token));
                }
                Err(e) => warn!(error = %e, "accept failed"),
            }
        }
    }
}

/// Serve client `id` until it disconnects. It joins `clients`, and so hears
/// broadcasts, only after it authenticates.
fn handle_client(mut stream: TcpStream, id: u64, clients: &Clients, requests: &dyn Requests, token: &str) {
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".into());
    if let Err(e) = authenticate(&mut stream, token) {
        warn!(client = id, peer = %peer, error = %e, "client rejected");
        return;
    }
    info!(client = id, peer = %peer, "client connected");

    let mut writer_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            warn!(client = id, error = %e, "client error");
            return;
        }
    };
    let (reply, frames) = clients.connect(id);

    // One writer per client keeps its frames whole even when replies and
    // broadcasts race.
    let writer = thread::spawn(move || {
        while let Ok(frame) = frames.recv() {
            if write_frame(&mut writer_stream, &frame).is_err() {
                break;
            }
        }
    });

    let mut reader = stream;
    loop {
        match read_message(&mut reader, MAX_MESSAGE_BYTES) {
            Ok(Some(raw)) => receive(clients, requests, id, &raw, &reply),
            Ok(None) => break, // clean disconnect
            Err(e) => {
                warn!(client = id, error = %e, "client read error");
                break;
            }
        }
    }

    // Removing the list's reply and dropping ours leaves the writer's
    // channel with no senders, so it drains what is queued and ends.
    // Downloads this client started keep running.
    clients.client_gone(id);
    requests.gone(id);
    drop(reply);
    let _ = writer.join();
    info!(client = id, "client disconnected");
}

/// Run the daemon side of the handshake described in `ipc`.
fn authenticate(stream: &mut TcpStream, token: &str) -> io::Result<()> {
    // The whole handshake gets AUTH_TIMEOUT, not each read of it.
    let until = Instant::now() + AUTH_TIMEOUT;
    let client_nonce = match ipc::recv_handshake(&mut ipc::Deadline::new(stream, until))? {
        Handshake::AuthHello { client_nonce } => client_nonce,
        _ => return Err(denied("first frame was not auth_hello")),
    };
    let server_nonce = ipc::new_nonce()?;
    let proof = ipc::proof(token, DAEMON_LABEL, &client_nonce, &server_nonce);
    ipc::send_handshake(stream, &Handshake::AuthChallenge { server_nonce: server_nonce.clone(), proof })?;

    let expected = ipc::proof(token, HOST_LABEL, &client_nonce, &server_nonce);
    match ipc::recv_handshake(&mut ipc::Deadline::new(stream, until))? {
        Handshake::Auth { proof } if ipc::ct_eq(proof.as_bytes(), expected.as_bytes()) => {}
        _ => return Err(denied("missing or wrong auth proof")),
    }
    ipc::send_handshake(stream, &Handshake::AuthOk)?;

    stream.set_read_timeout(None)
}

fn denied(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, why.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uidm_application::daemon::Command;

    struct Echo(Mutex<Vec<u64>>);

    impl Requests for Echo {
        fn handle(&self, _client: u64, command: Command, reply: &Reply) {
            if matches!(command, Command::Ping) {
                reply.send(Event::Pong);
            }
        }

        fn gone(&self, client: u64) {
            lock(&self.0).push(client);
        }
    }

    fn clients() -> Clients {
        Clients::new(Box::new(|_| {}), Box::new(|| {}))
    }

    fn got(frames: &Receiver<Vec<u8>>) -> Vec<Outgoing> {
        frames.try_iter().filter_map(|f| serde_json::from_slice(&f).ok()).collect()
    }

    #[test]
    fn a_frame_that_is_not_a_request_gets_an_error() {
        let (clients, handler) = (clients(), Echo(Mutex::default()));
        let (reply, frames) = reply_channel();
        receive(&clients, &handler, 1, b"{\"op\":\"nonsense\"}", &reply);
        receive(&clients, &handler, 1, br#"{"op":"ping"}"#, &reply);
        let got = got(&frames);
        assert!(matches!(&got[0], Outgoing::Error { message, .. } if message.starts_with("invalid message")));
        assert!(matches!(got[1], Outgoing::Pong));
    }

    #[test]
    fn only_windows_hear_what_is_sent_to_windows() {
        let clients = clients();
        let (_window, window_frames) = clients.connect(1);
        let (_browser, browser_frames) = clients.connect(2);
        clients.add_window(1);
        assert!(clients.extension_connected());
        clients.to_windows(Event::Show);
        clients.broadcast(Event::Pong);
        assert_eq!(got(&window_frames).len(), 2);
        assert!(matches!(got(&browser_frames)[..], [Outgoing::Pong]));
    }

    #[test]
    fn a_client_whose_writer_is_gone_is_dropped_on_broadcast() {
        let clients = clients();
        let (_reply, frames) = clients.connect(1);
        drop(frames);
        clients.broadcast(Event::Pong);
        assert!(clients.is_empty());
    }

    /// A server with token "right" on a free port, and its clients and
    /// handler.
    fn start() -> (std::net::SocketAddr, Arc<Clients>, Arc<Echo>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = Server { listener, token: "right".into() };
        let clients = Arc::new(clients());
        let handler = Arc::new(Echo(Mutex::default()));
        {
            let (clients, handler) = (Arc::clone(&clients), Arc::clone(&handler) as Arc<dyn Requests>);
            thread::spawn(move || server.serve(clients, handler));
        }
        (addr, clients, handler)
    }

    /// Connect to `addr` and answer the challenge with `token`'s proof.
    fn knock(addr: std::net::SocketAddr, token: &str) -> TcpStream {
        let mut stream = TcpStream::connect(addr).unwrap();
        ipc::send_handshake(&mut stream, &Handshake::AuthHello { client_nonce: "n".into() }).unwrap();
        let Handshake::AuthChallenge { server_nonce, .. } = ipc::recv_handshake(&mut stream).unwrap() else {
            panic!("no challenge");
        };
        let proof = ipc::proof(token, HOST_LABEL, "n", &server_nonce);
        ipc::send_handshake(&mut stream, &Handshake::Auth { proof }).unwrap();
        stream
    }

    #[test]
    fn a_client_without_the_token_is_turned_away() {
        let (addr, clients, handler) = start();
        let mut stream = knock(addr, "wrong");
        assert!(ipc::recv_handshake(&mut stream).is_err(), "the daemon closes the connection");
        assert!(clients.is_empty());
        assert!(lock(&handler.0).is_empty(), "a rejected client never joined");
    }

    #[test]
    fn a_client_with_the_token_is_served_until_it_goes() {
        let (addr, clients, handler) = start();
        let mut stream = knock(addr, "right");
        assert!(matches!(ipc::recv_handshake(&mut stream).unwrap(), Handshake::AuthOk));
        write_frame(&mut stream, br#"{"op":"ping"}"#).unwrap();
        let frame = read_message(&mut stream, MAX_MESSAGE_BYTES).unwrap().unwrap();
        assert!(matches!(serde_json::from_slice(&frame).unwrap(), Outgoing::Pong));
        assert!(!clients.is_empty());

        drop(stream);
        let until = Instant::now() + std::time::Duration::from_secs(5);
        while lock(&handler.0).is_empty() {
            assert!(Instant::now() < until, "the handler never heard the client go");
            thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(clients.is_empty());
    }
}
