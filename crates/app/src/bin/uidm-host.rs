//! UIDM native-messaging host: the process the browser launches.
//!
//! It is a thin relay. It connects to the daemon (starting it if needed), then
//! copies frames both ways without parsing them: browser stdin -> daemon, and
//! daemon -> browser stdout. Both sides use the same 4-byte little-endian
//! length prefix plus UTF-8 JSON, so a frame passes through unchanged.
//!
//! When the browser closes the port the host exits, but the daemon and its
//! downloads keep running.
//!
//! stdout carries protocol frames only; logs go to stderr, which the browser
//! captures.
//!
//! A CLI test mode runs a download in-process, without a browser or daemon:
//!   uidm-host download <url> [output_dir]
//!
//! Other programs send links to UIDM with
//!   uidm-host send <file.json | ->
//! which reads one link, or a list of links for the link picker, as JSON
//! (see `uidm_infrastructure::cli::parse_sent`) and hands it to the daemon, starting it if
//! needed. It goes through the same authenticated connection as the relay.

use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use uidm_infrastructure::app::MAX_MESSAGE_BYTES;

/// After the browser closes, how long to wait for the daemon to deliver
/// replies already in flight before giving up on them.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
use uidm_infrastructure::cli::Sent;
use uidm_infrastructure::engine::{self, DownloadRequest, Progress};
use uidm_infrastructure::ipc;
use uidm_infrastructure::protocol::{read_message, write_frame, Incoming, Outgoing};
use uidm_infrastructure::settings::{default_downloads_dir, FileExists, Folders, DEFAULT_CONNECTIONS};

/// The requests UIDM's browser extension sends. The host passes only these
/// on: anything else from the browser, which an extension that is not
/// UIDM's could send, never reaches the daemon.
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
    "rendered",
    "cookies",
];

/// A frame from the browser as it may go to the daemon, or `None` to drop
/// it. A `hello` loses any `client`, so a browser connection is never taken
/// for the desktop window, which may change every setting.
fn from_browser(frame: &[u8]) -> Option<Vec<u8>> {
    let mut msg: serde_json::Value = serde_json::from_slice(frame).ok()?;
    let op = msg.get("op")?.as_str()?;
    if !BROWSER_OPS.contains(&op) {
        return None;
    }
    if op == "hello" {
        msg.as_object_mut()?.remove("client");
    }
    serde_json::to_vec(&msg).ok()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("download") => std::process::exit(run_cli(&args)),
        Some("send") => std::process::exit(run_send(&args)),
        _ => {}
    }
    if let Err(e) = run_relay() {
        eprintln!("[uidm-host] fatal: {e}");
        std::process::exit(1);
    }
}

/// Relay frames between the browser (stdio) and the daemon (loopback TCP).
fn run_relay() -> io::Result<()> {
    let daemon = ipc::connect_or_spawn()?;
    eprintln!("[uidm-host] connected to daemon at {}", ipc::daemon_addr());
    let mut to_daemon = daemon.try_clone()?;
    let mut from_daemon = daemon;

    // daemon -> browser
    let downstream = thread::spawn(move || {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        while let Ok(Some(frame)) = read_message(&mut from_daemon, MAX_MESSAGE_BYTES) {
            if write_frame(&mut out, &frame).is_err() || out.flush().is_err() {
                break;
            }
        }
    });

    // browser -> daemon
    let stdin = io::stdin();
    let mut input = stdin.lock();
    loop {
        match read_message(&mut input, MAX_MESSAGE_BYTES)? {
            Some(frame) => {
                let Some(frame) = from_browser(&frame) else {
                    eprintln!("[uidm-host] dropped a request the extension does not send");
                    continue;
                };
                if write_frame(&mut to_daemon, &frame).is_err() {
                    eprintln!("[uidm-host] daemon connection lost");
                    break;
                }
            }
            None => {
                eprintln!("[uidm-host] stdin closed, exiting");
                break;
            }
        }
    }

    // Close only our sending side. The daemon sees EOF, flushes any reply still
    // queued for this client, and closes its end, which ends the downstream
    // thread. If that takes longer than DRAIN_TIMEOUT, force the socket shut.
    // Either way the daemon's downloads are unaffected.
    let _ = to_daemon.shutdown(Shutdown::Write);
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    while !downstream.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let _ = to_daemon.shutdown(Shutdown::Both);
    let _ = downstream.join();
    Ok(())
}

/// How long `send` waits for the daemon's answer once connected.
const SEND_TIMEOUT: Duration = Duration::from_secs(30);

/// `uidm-host send <file | ->`: hand the JSON in the file (or on stdin) to
/// the daemon. One link is added as the extension adds one, and its id goes
/// to stdout; a list opens the link picker. Returns a process exit code: 0
/// when the daemon took it, 1 when it refused or could not be reached, 2 for
/// bad input.
fn run_send(args: &[String]) -> i32 {
    let source = match args.get(2) {
        Some(s) if args.len() == 3 => s,
        _ => {
            eprintln!("usage: uidm-host send <file.json | ->");
            return 2;
        }
    };
    let json = match read_input(source) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("[uidm-host] could not read {source}: {e}");
            return 2;
        }
    };
    let cwd = std::env::current_dir().unwrap_or_default();
    let sent = match uidm_infrastructure::cli::parse_sent(&json, &cwd) {
        Ok(sent) => sent,
        Err(e) => {
            eprintln!("[uidm-host] {e}");
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
            eprintln!("[uidm-host] {e}");
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
fn send(sent: Sent) -> io::Result<Option<u64>> {
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
            Ok(Outgoing::Rejected { message, .. } | Outgoing::Error { message }) => {
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
            eprintln!("usage: uidm-host download <url> [output_dir]");
            return 2;
        }
    };
    let output_dir = args.get(3).map(PathBuf::from).unwrap_or_else(default_downloads_dir);
    eprintln!("[uidm-host] downloading {url} -> {}", output_dir.display());

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
            net: Default::default(),
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
                Some(t) if t > 0 => eprint!("\r[progress] {done}/{t} ({}%)   ", done * 100 / t),
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
    }
}

#[cfg(test)]
mod tests {
    use super::from_browser;

    fn op(frame: &[u8]) -> Option<serde_json::Value> {
        from_browser(frame).map(|f| serde_json::from_slice(&f).unwrap())
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
    fn a_browser_is_never_taken_for_the_window() {
        let hello = op(br#"{"op":"hello","client":"window","version":"1"}"#).unwrap();
        assert_eq!(hello.get("client"), None);
        assert_eq!(hello["version"], "1");
    }
}
