//! UDM native-messaging host: the process the browser launches.
//!
//! It is a thin relay. It connects to the daemon (starting it if needed), then
//! copies frames both ways without parsing them: browser stdin -> daemon, and
//! daemon -> browser stdout. Both sides use the same 4-byte little-endian
//! length prefix plus UTF-8 JSON, so a frame passes through unchanged.
//!
//! When the browser closes the port the host exits, but the daemon and its
//! downloads keep running. When the daemon goes away while the browser
//! stays (an update, a crash), the host connects again on the browser's
//! next request and sends it to the new daemon (see [`relay`]).
//!
//! stdout carries protocol frames only; logs go to `udm-host.log` in the
//! data folder (see `udm_infrastructure::logging`).
//!
//! A CLI test mode runs a download in-process, without a browser or daemon:
//!   udm-host download <url> [output_dir]
//!
//! Other programs send links to UDM with
//!   udm-host send <file.json | ->
//! which reads one link, or a list of links for the link picker, as JSON
//! (see `udm_infrastructure::cli::parse_sent`) and hands it to the daemon, starting it if
//! needed. It goes through the same authenticated connection as the relay.
//!
//! Safari's app extension (`installer/macos/safari`) runs
//!   udm-host safari
//! a relay that does not start the daemon: it runs inside Safari's sandbox,
//! and a daemon it started would be sandboxed too. Without a running daemon
//! it exits with the reason on stderr, which the app extension passes on.

// Tests may use todo!, unimplemented! and unreachable!, which clippy.toml's
// test exemptions do not cover (AGENTS.md section 9).
#![cfg_attr(test, allow(clippy::todo, clippy::unimplemented, clippy::unreachable))]

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use udm_infrastructure::app::MAX_MESSAGE_BYTES;

/// After the browser closes, how long to wait for the daemon to deliver
/// replies already in flight before giving up on them.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
use udm_domain::id::DownloadId;
use udm_domain::settings::{FileExists, Folders, DEFAULT_CONNECTIONS};
use udm_infrastructure::cli::Sent;
use udm_infrastructure::engine::{self, DownloadRequest, Progress};
use udm_infrastructure::ipc;
use udm_infrastructure::logging;
use udm_infrastructure::protocol::{read_message, write_frame, Incoming, Outgoing};
use udm_infrastructure::settings::default_downloads_dir;

/// The requests UDM's browser extension sends. The host passes only these
/// on: anything else from the browser, which an extension that is not
/// UDM's could send, never reaches the daemon.
const BROWSER_OPS: &[&str] = &[
    "hello",
    "list",
    "get_settings",
    "set_settings",
    "add",
    "confirm",
    "cancel",
    "retry",
    "remove",
    "refresh_link",
    "cancel_refresh",
    "probe_media",
    "pick_links",
    "pick_videos",
    "pick_gallery",
    "rendered",
    "cookies",
];

/// A frame from the browser as it may go to the daemon, with its `op`, or
/// `None` to drop it. A `hello` loses any `client`, so a browser connection
/// is never taken for the desktop window, which may change every setting.
fn from_browser(frame: &[u8]) -> Option<(String, Vec<u8>)> {
    let mut msg: serde_json::Value = serde_json::from_slice(frame).ok()?;
    let op = msg.get("op")?.as_str()?.to_string();
    if !BROWSER_OPS.contains(&op.as_str()) {
        return None;
    }
    if op == "hello" {
        msg.as_object_mut()?.remove("client");
    }
    Some((op, serde_json::to_vec(&msg).ok()?))
}

fn main() {
    // Before the log, which would make UDM's data folder first.
    let moved = udm_infrastructure::from_old_names::move_from_old_names();
    logging::init(logging::Program::Host);
    // A panic leaves a report in the data folder (`crash`).
    udm_infrastructure::crash::install(logging::Program::Host);
    moved.log();
    let args: Vec<String> = std::env::args().collect();
    // The test and `send` modes print to stdout and stderr on purpose: that
    // is their output, for the person or program that ran them.
    let spawn = match mode(&args) {
        Mode::Download => std::process::exit(run_cli(&args)),
        Mode::Send => std::process::exit(run_send(&args)),
        Mode::Relay { spawn } => spawn,
    };
    if let Err(e) = run_relay(spawn) {
        tracing::error!(error = format!("{e:#}"), "fatal");
        std::process::exit(1);
    }
}

/// What the host was started to do.
#[derive(Debug, PartialEq, Eq)]
enum Mode {
    /// Relay a browser's frames, starting the daemon when `spawn` and it is
    /// not running.
    Relay { spawn: bool },
    /// `udm-host download <url> [output_dir]`.
    Download,
    /// `udm-host send <file | ->`.
    Send,
}

/// The mode the command line `args` (the program first) asks for. Chrome
/// and Edge pass the extension's origin and Firefox the host manifest's
/// path: anything but a mode's name is a browser's relay.
fn mode(args: &[String]) -> Mode {
    match args.get(1).map(String::as_str) {
        Some("download") => Mode::Download,
        Some("send") => Mode::Send,
        Some("safari") => Mode::Relay { spawn: false },
        _ => Mode::Relay { spawn: true },
    }
}

/// The most a message to the browser may be: Chrome closes the native host
/// that sends a larger one.
const MAX_TO_BROWSER: usize = 1024 * 1024;

/// A frame from the daemon as it may go to the browser, or `None` to drop
/// it. The extension has no use for torrents' stats, files or rows'
/// torrent part, which can be large, and Chrome takes no message over
/// [`MAX_TO_BROWSER`].
fn to_browser(frame: Vec<u8>) -> Option<Vec<u8>> {
    let frame = if frame.windows(8).any(|w| w == b"\"torrent") {
        let mut msg: serde_json::Value = serde_json::from_slice(&frame).ok()?;
        let op = msg.get("op").and_then(|o| o.as_str()).unwrap_or("");
        if op.starts_with("torrent_") {
            return None;
        }
        let strip = |entry: &mut serde_json::Value| {
            if let Some(entry) = entry.as_object_mut() {
                entry.remove("torrent");
            }
        };
        match op {
            "downloads" => msg.get_mut("items").and_then(|i| i.as_array_mut()).into_iter().flatten().for_each(strip),
            "updated" => msg.get_mut("item").into_iter().for_each(strip),
            _ => {}
        }
        serde_json::to_vec(&msg).ok()?
    } else {
        frame
    };
    if frame.len() > MAX_TO_BROWSER {
        tracing::warn!(bytes = frame.len(), "dropped a message too large for the browser");
        return None;
    }
    Some(frame)
}

/// Relay frames between the browser (stdio) and the daemon (loopback TCP),
/// starting the daemon first when `spawn` and it is not running.
fn run_relay(spawn: bool) -> anyhow::Result<()> {
    let connect = || if spawn { ipc::connect_or_spawn() } else { ipc::connect_running() };
    relay(&mut io::stdin().lock(), io::stdout(), connect)
}

/// The browser's requests a new daemon connection starts with: the `hello`
/// that tells the daemon a browser is there, then the reads that fill the
/// extension's copy of the list and of the settings. None of them changes
/// anything, so they may go again to a daemon that started again.
const OPENING_OPS: [&str; 3] = ["hello", "list", "get_settings"];

/// The latest frame the browser sent for each of [`OPENING_OPS`].
#[derive(Default)]
struct Opening([Option<Vec<u8>>; 3]);

impl Opening {
    /// Keep `frame` when `op` is an opening one, and say whether it was.
    fn note(&mut self, op: &str, frame: &[u8]) -> bool {
        let Some(slot) = OPENING_OPS.iter().position(|o| *o == op) else { return false };
        self.0[slot] = Some(frame.to_vec());
        true
    }

    fn frames(&self) -> impl Iterator<Item = &[u8]> {
        self.0.iter().flatten().map(Vec::as_slice)
    }
}

/// One connection to the daemon: the side the browser's frames go to, and
/// the thread that copies the daemon's frames to the browser. The thread
/// hands the browser's output back when it ends, for the next connection.
struct Link<W> {
    to_daemon: TcpStream,
    /// Set once the daemon closed this connection or it broke.
    gone: Arc<AtomicBool>,
    downstream: JoinHandle<W>,
}

impl<W: Write + Send + 'static> Link<W> {
    /// Start copying `daemon`'s frames to `out`.
    fn open(daemon: TcpStream, mut out: W) -> io::Result<Self> {
        tracing::info!(addr = %ipc::daemon_addr(), "connected to the daemon");
        let to_daemon = daemon.try_clone()?;
        let gone = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&gone);
        let mut from_daemon = daemon;
        let downstream = thread::spawn(move || {
            while let Ok(Some(frame)) = read_message(&mut from_daemon, MAX_MESSAGE_BYTES) {
                let Some(frame) = to_browser(frame) else { continue };
                // The browser went: stdin closes too, which ends the relay.
                if write_frame(&mut out, &frame).is_err() || out.flush().is_err() {
                    return out;
                }
            }
            seen.store(true, Ordering::SeqCst);
            out
        });
        Ok(Link { to_daemon, gone, downstream })
    }

    fn gone(&self) -> bool {
        self.gone.load(Ordering::SeqCst)
    }

    /// Close this connection, connect again through `connect` and send the
    /// new daemon the browser's `opening` requests.
    fn reconnect(self, connect: &impl Fn() -> io::Result<TcpStream>, opening: &Opening) -> anyhow::Result<Self> {
        use anyhow::Context as _;
        let _ = self.to_daemon.shutdown(Shutdown::Both);
        let out = self.downstream.join().map_err(|_| anyhow::anyhow!("the thread to the browser stopped"))?;
        let daemon = connect().context("could not reach the daemon again")?;
        let mut link = Link::open(daemon, out).context("could not share the daemon connection")?;
        for frame in opening.frames() {
            write_frame(&mut link.to_daemon, frame).context("could not greet the daemon again")?;
        }
        Ok(link)
    }
}

/// Relay frames between the browser (`input` and `out`) and the daemon,
/// which `connect` reaches. A daemon that stops and starts again while the
/// browser keeps the host (an update, a crash) is reached again on the
/// browser's next request, and that request goes to the new daemon instead
/// of being lost. A request goes to the new daemon only when the old one
/// cannot have read it: the old one had closed the connection before the
/// request was written, or the write failed, which leaves at most part of
/// a frame, and the daemon drops a part. So a live daemon never gets a
/// request twice.
fn relay<W: Write + Send + 'static>(
    input: &mut impl Read,
    out: W,
    connect: impl Fn() -> io::Result<TcpStream>,
) -> anyhow::Result<()> {
    use anyhow::Context as _;
    let daemon = connect().context("could not reach the daemon")?;
    let mut link = Link::open(daemon, out).context("could not share the daemon connection")?;
    let mut opening = Opening::default();

    // browser -> daemon
    loop {
        let Some(frame) = read_message(input, MAX_MESSAGE_BYTES).context("could not read from the browser")? else {
            tracing::info!("stdin closed, exiting");
            break;
        };
        let Some((op, frame)) = from_browser(&frame) else {
            tracing::warn!("dropped a request the extension does not send");
            continue;
        };
        // An opening request goes with the new connection's opening.
        let opens = opening.note(&op, &frame);
        if link.gone() {
            tracing::warn!(op, "daemon connection lost, connecting again");
            link = link.reconnect(&connect, &opening)?;
            if opens {
                continue;
            }
        }
        if write_frame(&mut link.to_daemon, &frame).is_err() {
            tracing::warn!(op, "daemon connection lost while sending, connecting again");
            link = link.reconnect(&connect, &opening)?;
            if !opens && write_frame(&mut link.to_daemon, &frame).is_err() {
                tracing::warn!(op, "daemon connection lost again");
                break;
            }
        }
    }

    // Close only our sending side. The daemon sees EOF, flushes any reply still
    // queued for this client, and closes its end, which ends the downstream
    // thread. If that takes longer than DRAIN_TIMEOUT, force the socket shut.
    // Either way the daemon's downloads are unaffected.
    let _ = link.to_daemon.shutdown(Shutdown::Write);
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    while !link.downstream.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let _ = link.to_daemon.shutdown(Shutdown::Both);
    let _ = link.downstream.join();
    Ok(())
}

/// How long `send` waits for the daemon's answer once connected.
const SEND_TIMEOUT: Duration = Duration::from_secs(30);

/// `udm-host send <file | ->`: hand the JSON in the file (or on stdin) to
/// the daemon. One link is added as the extension adds one, and its id goes
/// to stdout; a list opens the link picker. Returns a process exit code: 0
/// when the daemon took it, 1 when it refused or could not be reached, 2 for
/// bad input.
fn run_send(args: &[String]) -> i32 {
    let source = match args.get(2) {
        Some(s) if args.len() == 3 => s,
        _ => {
            eprintln!("usage: udm-host send <file.json | ->");
            return 2;
        }
    };
    let json = match read_input(source) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("[udm-host] could not read {source}: {e}");
            return 2;
        }
    };
    let cwd = std::env::current_dir().unwrap_or_default();
    let sent = match udm_infrastructure::cli::parse_sent(&json, &cwd) {
        Ok(sent) => sent,
        Err(e) => {
            eprintln!("[udm-host] {e}");
            return 2;
        }
    };
    match send(sent) {
        Ok(Some(id)) => {
            println!("{id}");
            0
        }
        Ok(None) => 0,
        Err(e) => {
            eprintln!("[udm-host] {e}");
            1
        }
    }
}

/// The text of `source`, a file or `-` for stdin, up to the size of one
/// message.
fn read_input(source: &str) -> io::Result<String> {
    let limit = u64::from(MAX_MESSAGE_BYTES);
    let mut text = String::new();
    if source == "-" {
        io::stdin().lock().take(limit + 1).read_to_string(&mut text)?;
    } else {
        std::fs::File::open(source)?.take(limit + 1).read_to_string(&mut text)?;
    }
    if text.len() as u64 > limit {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("larger than {limit} bytes")));
    }
    // A file saved by Windows PowerShell 5 or Notepad may start with a BOM.
    Ok(text.trim_start_matches('\u{feff}').to_string())
}

/// Give `sent` to the daemon and wait for its answer: the new download's id
/// for one link, `None` once a list has reached the picker.
fn send(sent: Sent) -> io::Result<Option<DownloadId>> {
    let mut daemon = ipc::connect_or_spawn()?;
    daemon.set_read_timeout(Some(SEND_TIMEOUT))?;
    let hello =
        Incoming::Hello { version: Some(env!("CARGO_PKG_VERSION").to_string()), client: Some("program".into()) };
    let one = matches!(sent, Sent::One(_));
    let request = match sent {
        Sent::One(download) => Incoming::Add(download),
        Sent::Many { referer, links } => Incoming::PickLinks { page: referer, links, user_agent: None },
    };
    // `pick_links` has no answer, so a ping after it shows the daemon has
    // handled it: it reads a client's requests in order.
    let mut messages = vec![hello, request];
    if !one {
        messages.push(Incoming::Ping);
    }
    for msg in &messages {
        write_frame(&mut daemon, &serde_json::to_vec(msg)?)?;
    }
    loop {
        let raw = read_message(&mut daemon, MAX_MESSAGE_BYTES)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "the daemon closed the connection"))?;
        // Other clients' news comes too; only the answers matter here.
        match serde_json::from_slice::<Outgoing>(&raw) {
            Ok(Outgoing::Ack { id, .. }) if one => return Ok(Some(id)),
            Ok(Outgoing::Pong) if !one => return Ok(None),
            Ok(Outgoing::Rejected { message, .. } | Outgoing::Error { message, .. }) => {
                return Err(io::Error::other(message))
            }
            _ => {}
        }
    }
}

/// CLI test mode: download a URL in-process and print progress to stderr.
/// Returns a process exit code.
fn run_cli(args: &[String]) -> i32 {
    let url = match args.get(2) {
        Some(u) => u.clone(),
        None => {
            eprintln!("usage: udm-host download <url> [output_dir]");
            return 2;
        }
    };
    let output_dir = args.get(3).map(PathBuf::from).unwrap_or_else(default_downloads_dir);
    eprintln!("[udm-host] downloading {url} -> {}", output_dir.display());

    // No proxy or login, as ever in this test mode, but the network adapter
    // and DNS the settings choose, so it never goes out another way.
    let settings =
        match udm_infrastructure::settings::settings_path().and_then(|p| udm_infrastructure::settings::load(&p)) {
            Ok(settings) => settings,
            Err(e) => {
                eprintln!("[udm-host] the settings could not be read: {e}");
                return 1;
            }
        };
    let route = udm_infrastructure::connect::Network::default().route(&settings);
    let ok = Arc::new(AtomicBool::new(false));
    let ok_cb = Arc::clone(&ok);
    engine::run_blocking(
        DownloadRequest {
            url,
            folders: Folders::fixed(output_dir),
            filename: None,
            headers: Vec::new(),
            connections: DEFAULT_CONNECTIONS,
            reuse_path: None,
            file_exists: FileExists::Rename,
            post_body: None,
            net: Arc::new(udm_infrastructure::net::NetConfig { route, ..Default::default() }),
            media: None,
            ffmpeg: None,
            speed_limit: None,
            server_date: true,
            server_date_created: false,
            ignore_date: false,
            temp_dir: None,
            reuse_partial: None,
            host_limit: None,
            elevate: false,
            resolve: None,
            torrent_files: None,
            torrent_uploaded: udm_domain::bytes::ByteCount::ZERO,
        },
        Arc::new(move |p: Progress| print_progress(p, &ok_cb)),
    );

    if ok.load(Ordering::Relaxed) {
        0
    } else {
        1
    }
}

fn print_progress(p: Progress, ok: &AtomicBool) {
    match p {
        Progress::Started { total, resumable, connections, .. } => {
            let size = total.map(|t| format!("{t} bytes")).unwrap_or_else(|| "unknown size".into());
            eprintln!("[started] {size}, resumable={resumable}, connections={connections}");
        }
        Progress::Advanced { done, total, .. } => {
            match total {
                Some(t) if !t.is_zero() => {
                    eprint!("\r[progress] {done}/{t} ({}%)   ", done.get().saturating_mul(100) / t.get())
                }
                _ => eprint!("\r[progress] {done} bytes   "),
            }
            let _ = io::stderr().flush();
        }
        Progress::Finished { path, .. } => {
            ok.store(true, Ordering::Relaxed);
            eprintln!("\n[done] saved to {}", path.display());
        }
        Progress::Failed { error, .. } => eprintln!("\n[failed] {error}"),
        Progress::Canceled { .. } => eprintln!("\n[canceled]"),
        Progress::NameTaken { path, .. } => eprintln!("\n[failed] {} already exists", path.display()),
        Progress::TorrentInfo { .. } | Progress::TorrentStats { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{from_browser, mode, relay, to_browser, Mode, MAX_TO_BROWSER};
    use std::io::{self, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use udm_infrastructure::app::MAX_MESSAGE_BYTES;
    use udm_infrastructure::protocol::{read_message, write_frame};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn each_browser_gets_a_relay_and_only_safari_one_that_starts_no_daemon() {
        // Chrome and Edge pass the extension's origin (and on Windows a
        // parent window), Firefox the manifest's path and the extension's ID.
        let chrome = args(&["udm-host", "chrome-extension://abc/", "--parent-window=0"]);
        assert_eq!(mode(&chrome), Mode::Relay { spawn: true });
        let firefox = args(&["udm-host", "/x/com.udm.host.json", "udm@udm.local"]);
        assert_eq!(mode(&firefox), Mode::Relay { spawn: true });
        assert_eq!(mode(&args(&["udm-host"])), Mode::Relay { spawn: true });
        assert_eq!(mode(&args(&["udm-host", "safari"])), Mode::Relay { spawn: false });
        assert_eq!(mode(&args(&["udm-host", "download", "https://a/b"])), Mode::Download);
        assert_eq!(mode(&args(&["udm-host", "send", "-"])), Mode::Send);
    }

    fn op(frame: &[u8]) -> Option<serde_json::Value> {
        from_browser(frame).map(|(_, f)| serde_json::from_slice(&f).unwrap())
    }

    #[test]
    fn only_the_extensions_requests_pass() {
        assert!(op(br#"{"op":"add","url":"https://a/b.zip"}"#).is_some());
        // An add's fields pass unchanged, the parent page beside the referrer.
        let add = op(br#"{"op":"add","url":"https://a/b.zip","referrer":"https://a/f","page":"https://a/p"}"#).unwrap();
        assert_eq!((add["referrer"].as_str(), add["page"].as_str()), (Some("https://a/f"), Some("https://a/p")));
        assert!(op(br#"{"op":"set_settings","settings":{}}"#).is_some());
        // Requests only the window sends stay out.
        assert!(op(br#"{"op":"save_queue","queue":{}}"#).is_none());
        assert!(op(br#"{"op":"convert","id":1}"#).is_none());
        assert!(op(br#"not json"#).is_none());
        assert!(op(br#"{"url":"no op"}"#).is_none());
    }

    #[test]
    fn the_browser_hears_nothing_of_torrents_and_nothing_too_large() {
        assert_eq!(to_browser(br#"{"op":"torrent_stats","id":1}"#.to_vec()), None);
        assert_eq!(to_browser(br#"{"op":"torrent_info","id":1}"#.to_vec()), None);
        let rows = br#"{"op":"downloads","items":[{"id":1,"url":"magnet:?x","torrent":{"info_hash":"ab"}},{"id":2}]}"#;
        let rows: serde_json::Value = serde_json::from_slice(&to_browser(rows.to_vec()).unwrap()).unwrap();
        assert_eq!(rows["items"][0].get("torrent"), None);
        assert_eq!(rows["items"][1]["id"], 2);
        let updated = to_browser(br#"{"op":"updated","item":{"id":1,"torrent":{}}}"#.to_vec()).unwrap();
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&updated).unwrap()["item"].get("torrent"), None);
        let plain = br#"{"op":"progress","id":1}"#.to_vec();
        assert_eq!(to_browser(plain.clone()), Some(plain), "passed on as it is");
        let big = format!(r#"{{"op":"pick_links","page":"{}"}}"#, "x".repeat(MAX_TO_BROWSER));
        assert_eq!(to_browser(big.into_bytes()), None);
    }

    /// The browser's side of the relay: what the test hands it, in chunks
    /// sent on a channel, and EOF once the sender is dropped.
    struct Browser {
        chunks: mpsc::Receiver<Vec<u8>>,
        held: Vec<u8>,
    }

    impl Read for Browser {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.held.is_empty() {
                match self.chunks.recv() {
                    Ok(chunk) => self.held = chunk,
                    Err(_) => return Ok(0),
                }
            }
            let n = buf.len().min(self.held.len());
            buf[..n].copy_from_slice(&self.held[..n]);
            self.held.drain(..n);
            Ok(n)
        }
    }

    /// What the relay wrote to the browser.
    #[derive(Clone, Default)]
    struct Heard(Arc<Mutex<Vec<u8>>>);

    impl Write for Heard {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Heard {
        fn ops(&self) -> Vec<String> {
            let bytes = self.0.lock().unwrap().clone();
            let mut reader = &bytes[..];
            let mut ops = Vec::new();
            while let Some(frame) = read_message(&mut reader, MAX_MESSAGE_BYTES).unwrap() {
                ops.push(op_name(&frame));
            }
            ops
        }
    }

    fn frame(json: &str) -> Vec<u8> {
        let mut out = Vec::new();
        write_frame(&mut out, json.as_bytes()).unwrap();
        out
    }

    fn op_name(frame: &[u8]) -> String {
        let msg: serde_json::Value = serde_json::from_slice(frame).unwrap();
        msg["op"].as_str().unwrap().to_string()
    }

    /// A daemon on loopback that takes two connections, one after the other.
    /// It answers each `hello` with its own. It closes the first connection
    /// once it has read `first` frames, then tells `closed`; it reads the
    /// second to its end. Returns the ops each connection brought.
    fn daemon(listener: TcpListener, first: usize, closed: mpsc::Sender<()>) -> thread::JoinHandle<Vec<Vec<String>>> {
        thread::spawn(move || {
            let mut heard = Vec::new();
            for turn in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut ops = Vec::new();
                while let Some(frame) = read_message(&mut stream, MAX_MESSAGE_BYTES).unwrap() {
                    ops.push(op_name(&frame));
                    if ops.last().map(String::as_str) == Some("hello") {
                        write_frame(&mut stream, br#"{"op":"hello","name":"udm-daemon","version":"1"}"#).unwrap();
                    }
                    if turn == 0 && ops.len() == first {
                        break;
                    }
                }
                heard.push(ops);
                drop(stream);
                if turn == 0 {
                    closed.send(()).unwrap();
                }
            }
            heard
        })
    }

    /// Run the relay against `listener`'s daemon with the browser `browser`.
    fn run(listener: &TcpListener, browser: Browser, heard: Heard) -> thread::JoinHandle<anyhow::Result<()>> {
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let mut browser = browser;
            relay(&mut browser, heard, move || TcpStream::connect(addr))
        })
    }

    /// Wait for the relay's thread from the daemon to read the close.
    fn settle() {
        thread::sleep(Duration::from_millis(300));
    }

    #[test]
    fn a_request_after_the_daemon_restarted_reaches_the_new_daemon_once() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let (closed_tx, closed) = mpsc::channel();
        let daemon = daemon(listener.try_clone().unwrap(), 3, closed_tx);
        let (to_host, chunks) = mpsc::channel();
        let heard = Heard::default();
        let relay = run(&listener, Browser { chunks, held: Vec::new() }, heard.clone());

        // The extension's opening, which the first daemon reads, then closes.
        to_host.send(frame(r#"{"op":"hello","version":"1"}"#)).unwrap();
        to_host.send(frame(r#"{"op":"list"}"#)).unwrap();
        to_host.send(frame(r#"{"op":"get_settings"}"#)).unwrap();
        closed.recv_timeout(Duration::from_secs(5)).unwrap();
        settle();
        // The click right after the daemon came back.
        to_host.send(frame(r#"{"op":"add","url":"https://a/b.mp4"}"#)).unwrap();
        drop(to_host);

        relay.join().unwrap().unwrap();
        let ops = daemon.join().unwrap();
        assert_eq!(ops[0], ["hello", "list", "get_settings"]);
        // The new daemon hears the browser's opening again, then the click.
        assert_eq!(ops[1], ["hello", "list", "get_settings", "add"]);
        // Both daemons' greetings reach the browser.
        assert_eq!(heard.ops(), ["hello", "hello"]);
    }

    #[test]
    fn a_request_the_old_daemon_read_is_not_sent_again() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let (closed_tx, closed) = mpsc::channel();
        let daemon = daemon(listener.try_clone().unwrap(), 2, closed_tx);
        let (to_host, chunks) = mpsc::channel();
        let relay = run(&listener, Browser { chunks, held: Vec::new() }, Heard::default());

        to_host.send(frame(r#"{"op":"hello","version":"1"}"#)).unwrap();
        to_host.send(frame(r#"{"op":"add","url":"https://a/b.zip"}"#)).unwrap();
        closed.recv_timeout(Duration::from_secs(5)).unwrap();
        settle();
        to_host.send(frame(r#"{"op":"cancel","id":1}"#)).unwrap();
        drop(to_host);

        relay.join().unwrap().unwrap();
        let ops = daemon.join().unwrap();
        assert_eq!(ops[0], ["hello", "add"]);
        // Only the browser's hello opens the new connection: no second add.
        assert_eq!(ops[1], ["hello", "cancel"]);
    }

    #[test]
    fn a_request_that_restarts_the_opening_goes_once() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let (closed_tx, closed) = mpsc::channel();
        let daemon = daemon(listener.try_clone().unwrap(), 1, closed_tx);
        let (to_host, chunks) = mpsc::channel();
        let relay = run(&listener, Browser { chunks, held: Vec::new() }, Heard::default());

        to_host.send(frame(r#"{"op":"hello","version":"1"}"#)).unwrap();
        closed.recv_timeout(Duration::from_secs(5)).unwrap();
        settle();
        to_host.send(frame(r#"{"op":"list"}"#)).unwrap();
        drop(to_host);

        relay.join().unwrap().unwrap();
        assert_eq!(daemon.join().unwrap()[1], ["hello", "list"]);
    }

    #[test]
    fn a_daemon_that_does_not_come_back_ends_the_relay() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let tries = Arc::new(Mutex::new(0));
        let counted = Arc::clone(&tries);
        let connect = move || {
            let mut tries = counted.lock().unwrap();
            *tries += 1;
            if *tries == 1 {
                TcpStream::connect(addr)
            } else {
                Err(io::Error::new(io::ErrorKind::ConnectionRefused, "no daemon"))
            }
        };
        let accepted = thread::spawn(move || drop(listener.accept().unwrap()));
        let (to_host, chunks) = mpsc::channel();
        let relay = thread::spawn(move || {
            let mut browser = Browser { chunks, held: Vec::new() };
            relay(&mut browser, Heard::default(), connect)
        });
        accepted.join().unwrap();
        settle();
        to_host.send(frame(r#"{"op":"add","url":"https://a/b.zip"}"#)).unwrap();

        assert!(relay.join().unwrap().is_err(), "the browser sees the host go and connects again");
        assert_eq!(*tries.lock().unwrap(), 2);
    }

    #[test]
    fn a_browser_is_never_taken_for_the_window() {
        let hello = op(br#"{"op":"hello","client":"window","version":"1"}"#).unwrap();
        assert_eq!(hello.get("client"), None);
        assert_eq!(hello["version"], "1");
    }
}
