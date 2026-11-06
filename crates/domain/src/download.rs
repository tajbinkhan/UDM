//! A download as UIDM keeps it: what to fetch and how (`Download`), its
//! place in the list (`DownloadEntry`, `Status`), and the save-as
//! dialog's answer (`Answer`). The daemon and its clients exchange these
//! inside the messages of `uidm_infrastructure::protocol`.

use std::collections::BTreeMap;

use crate::media::MediaChoice;
use crate::queue::WhenDone;
use crate::settings::{Duplicate, FileExists};
use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

/// The save-as dialog's answer for a `confirming` download, or the start of
/// a `waiting` one. `filename` and `folder` replace the proposed ones when
/// given. With `start` false the download waits until started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub id: u64,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default = "yes")]
    pub start: bool,
    /// With `start` false, the queue it waits in; missing uses the queue
    /// Download later is set to, else the main queue.
    #[serde(default)]
    pub queue: Option<u64>,
    /// What to do if the file name is taken (rename or overwrite); the
    /// setting decides when missing.
    #[serde(default)]
    pub file_exists: Option<FileExists>,
    /// For a download with `duplicate_of`: what to do about the other
    /// download with the same address. Missing (or `ask`) adds it with a
    /// new name.
    #[serde(default)]
    pub duplicate: Option<Duplicate>,
    /// The category chosen for it, which the list files it under whatever
    /// its file type; missing keeps the one its type gives.
    #[serde(default)]
    pub category: Option<String>,
    /// A note about it; missing keeps the one it has, empty clears it.
    #[serde(default)]
    pub description: Option<String>,
    /// "Remember this path": turn keeping the last chosen folder on (and
    /// keep this one) or off for its category; missing leaves it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remember_folder: Option<bool>,
    /// "Remember my selection": make `duplicate` the setting for every
    /// address already in the list.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remember_duplicate: bool,
    /// "Don't ask me again": with `start` false, make `queue` the one
    /// "Download later" uses without asking.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remember_queue: bool,
    /// Save into the folder with administrator rights, because UIDM may
    /// not write there itself.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub elevate: bool,
}

impl Answer {
    /// Start download `id` as proposed.
    pub fn start(id: u64) -> Self {
        Answer {
            id,
            filename: None,
            folder: None,
            start: true,
            queue: None,
            file_exists: None,
            duplicate: None,
            category: None,
            description: None,
            remember_folder: None,
            remember_duplicate: false,
            remember_queue: false,
            elevate: false,
        }
    }
}

/// The payload a browser hand-off needs to reissue a request outside the page.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Download {
    pub url: String,
    /// `POST` for a download a form started; anything else is a GET.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// The form body of a POST, base64-encoded. Its content type goes in
    /// `headers`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Request headers as name -> value. A map keeps lookup simple; switch to a
    /// `Vec<(String, String)>` if you need to preserve duplicate header names.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookies: Option<String>,
    /// The address sent as the HTTP `Referer` header: the document that
    /// held the link, which may be a frame inside the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referrer: Option<String>,
    /// The web page the download came from (the browser tab's address),
    /// kept apart from the referrer the way IDM keeps both. It is only
    /// shown and sorted by, never sent to the server. Missing means the
    /// referrer is the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// Suggested filename, when the client could derive one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// `false` adds the download to a queue without starting it ("Download
    /// later"). Missing means start it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<bool>,
    /// The proxy the browser uses, such as `http://proxy:3128`, for the
    /// "use the browser's proxy" option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    /// The queue a download added with `start: false` waits in; the main
    /// queue when missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<u64>,
    /// The folder to save in, a full path, instead of the one its category
    /// picks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// A stream (HLS or DASH) to download as one video, and which quality.
    /// `url` is its playlist or manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaChoice>,
    /// What to do when its file name is taken, chosen in the save-as dialog;
    /// the setting decides when missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_exists: Option<FileExists>,
    /// What to do when its address is already in the list, answered when it
    /// was added; the setting decides when missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate: Option<Duplicate>,
    /// A note about the download, shown in the list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The category the list files it under, chosen in the save-as dialog;
    /// missing goes by its file type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// A login for this download alone. It is tried on the download's own
    /// site before any login saved in Options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<DownloadLogin>,
    /// This download's own speed limit in bytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_limit: Option<u64>,
    /// Show the save-as dialog for it (`true`) or not, whatever the
    /// setting says; missing follows the setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<bool>,
    /// The other headers the browser sent with the request, in its order,
    /// so the server sees the same request (IDM sends them all). The daemon
    /// keeps only the ones that are safe to repeat; see
    /// `app::browser_request_headers`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub request_headers: Vec<(String, String)>,
    /// The addresses the browser was redirected through before `url`,
    /// first to last.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redirects: Vec<String>,
    /// The size the browser's response gave (its `Content-Length`), shown
    /// until the daemon asks the server itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// The `Content-Type` of the browser's response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// The browser's whole proxy setting, for the "use the browser's
    /// proxy" option. It wins over `proxy`, which only ever named a fixed
    /// proxy and is still sent for older daemons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_proxy: Option<BrowserProxy>,
    /// Save into a folder that needs administrator rights: the finished
    /// file is moved there after a UAC prompt.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub elevate: bool,
}

/// How the browser reaches the web, as its extension reads it
/// (`chrome.proxy.settings`, or Firefox's `browser.proxy.settings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserProxyMode {
    /// No proxy.
    #[default]
    Direct,
    /// The fixed proxies in `http`, `https` and `socks`.
    Fixed,
    /// A proxy auto-config script, from `pac_url` or given in `pac_script`.
    Pac,
    /// The script WPAD finds on the network.
    AutoDetect,
    /// Whatever the system's proxy setting says.
    System,
}

/// The browser's proxy setting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserProxy {
    #[serde(default)]
    pub mode: BrowserProxyMode,
    /// `scheme://host:port` of the proxy for `http://` addresses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<String>,
    /// `scheme://host:port` of the proxy for `https://` addresses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub https: Option<String>,
    /// `socks4://` or `socks5://host:port` for what has no proxy of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socks: Option<String>,
    /// Where the auto-config script is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pac_url: Option<String>,
    /// The auto-config script itself, when the browser was given its text
    /// instead of an address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pac_script: Option<String>,
    /// Hosts reached directly, as the browser lists them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bypass: Vec<String>,
}

/// The most of each thing a browser request may carry; what is past them
/// is dropped (see `Download::limit_browser_request`). A page decides much
/// of what the browser sends, so none of it is trusted to be small.
pub const MAX_REQUEST_HEADERS: usize = 64;
pub const MAX_HEADER_NAME: usize = 128;
pub const MAX_HEADER_VALUE: usize = 8 * 1024;
pub const MAX_REDIRECTS: usize = 20;
pub const MAX_URL: usize = 8 * 1024;
pub const MAX_PAC_SCRIPT: usize = 512 * 1024;
pub const MAX_BYPASS: usize = 256;

impl Download {
    /// Drop what is too much or malformed in the request details the
    /// browser sent: headers whose name is not a header name or whose value
    /// could start another header, past the limits above; redirects that
    /// are not web addresses; an overlong content type or proxy setting.
    pub fn limit_browser_request(&mut self) {
        let token = |n: &str| {
            !n.is_empty()
                && n.len() <= MAX_HEADER_NAME
                && n.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
        };
        let value = |v: &str| v.len() <= MAX_HEADER_VALUE && !v.bytes().any(|b| b == b'\r' || b == b'\n' || b == 0);
        self.request_headers.retain(|(n, v)| token(n) && value(v));
        self.request_headers.truncate(MAX_REQUEST_HEADERS);
        self.redirects.retain(|u| u.len() <= MAX_URL && crate::batch::is_web_url(u));
        self.redirects.truncate(MAX_REDIRECTS);
        if self.content_type.as_deref().is_some_and(|t| !value(t) || t.len() > 256) {
            self.content_type = None;
        }
        if let Some(p) = &mut self.browser_proxy {
            for field in [&mut p.http, &mut p.https, &mut p.socks, &mut p.pac_url] {
                if field.as_deref().is_some_and(|a| a.len() > MAX_URL) {
                    *field = None;
                }
            }
            if p.pac_script.as_deref().is_some_and(|s| s.len() > MAX_PAC_SCRIPT) {
                p.pac_script = None;
            }
            p.bypass.retain(|b| !b.trim().is_empty() && b.len() <= 256);
            p.bypass.truncate(MAX_BYPASS);
        }
    }
}

/// A link on a web page, offered in the window's link picker.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageLink {
    pub url: String,
    /// The link's text, or an image's alt text.
    #[serde(default)]
    pub text: String,
    /// An image shown on the page rather than a link.
    #[serde(default)]
    pub image: bool,
    /// The browser's cookies for the link's site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookies: Option<String>,
}

/// A download's own login.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DownloadLogin {
    pub username: String,
    /// Missing in a `set_properties` change keeps the password it had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

/// Leaves the password out of logs.
impl std::fmt::Debug for DownloadLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadLogin")
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .finish()
    }
}

/// Changes to a stopped or finished download's details. A missing field is
/// left as it is; an empty referrer or description clears it, and a login
/// with an empty user name removes the download's login.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Properties {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referrer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Not for a finished download, which keeps no request to log in with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<DownloadLogin>,
    /// A new name for a finished download's file, which is renamed on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// A new folder (a full path) for the download's file. A finished file
    /// is moved there; a stopped, unfinished download moves its partial
    /// file and resume sidecar there and saves there from then on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

/// Where a download is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Downloading,
    Done,
    Failed,
    Canceled,
    /// Was queued or running when the daemon last stopped. It can be retried,
    /// and resumes from its partial file.
    Interrupted,
    /// Probed and waiting for the user to answer the save-as dialog. `path`
    /// holds the proposed file.
    Confirming,
    /// Added to start later ("Download later"). `path` holds the chosen file.
    Waiting,
}

impl Status {
    /// Finished one way or another; nothing is running for it.
    pub fn is_terminal(self) -> bool {
        matches!(self, Status::Done | Status::Failed | Status::Canceled | Status::Interrupted | Status::Waiting)
    }

    /// Stopped before finishing, so it can be started again.
    pub fn is_retryable(self) -> bool {
        matches!(self, Status::Failed | Status::Canceled | Status::Interrupted | Status::Waiting)
    }
}

/// One connection's share of a segmented download, for showing each
/// connection's progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentInfo {
    /// First and last byte of the range.
    pub start: u64,
    pub end: u64,
    /// Bytes of the range fetched so far.
    pub done: u64,
    pub state: SegmentState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentState {
    /// No connection has it yet.
    Waiting,
    Connecting,
    Receiving,
    /// The last attempt failed; it tries again after a pause.
    Retrying,
    Done,
}

/// The virus scan of a finished download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Scan {
    Scanning,
    Clean,
    /// The scanner reported a threat; `detail` is the end of its output.
    Threat {
        detail: String,
    },
    /// The scanner could not run or did not finish.
    Failed {
        detail: String,
    },
}

/// One download as the daemon tracks it, sent in a `downloads` snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadEntry {
    pub id: u64,
    pub url: String,
    /// Output file, once the engine has chosen it.
    pub path: Option<String>,
    pub total: Option<u64>,
    pub done: u64,
    pub status: Status,
    pub error: Option<String>,
    pub resumable: bool,
    pub connections: usize,
    /// The virus scan of the finished file, when scanning is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan: Option<Scan>,
    /// For a stream: segments fetched and segments in all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<[u64; 2]>,
    /// Something to know about a finished download, such as a stream saved
    /// as two files because ffmpeg was not found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// While `confirming`: another download in the list has the same
    /// address, and the save-as dialog asks what to do about it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<u64>,
    /// The Referer its request sends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referrer: Option<String>,
    /// The web page it came from, for finding it in the list. Entries saved
    /// before it was kept apart from the referrer get the referrer here
    /// when the history is read (see `History::load`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// When it last started, in seconds since 1970.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_try: Option<u64>,
    /// The user's note about it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The category chosen for it in the save-as dialog; missing goes by
    /// its file type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// The user name of the download's own login, if it has one. The
    /// password stays in the daemon's encrypted request store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_user: Option<String>,
    /// Its own speed limit in bytes per second, kept across restarts, and
    /// across stops and retries while `limit_remember` is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_limit: Option<u64>,
    /// Keep `speed_limit` when the download is stopped and resumed (IDM's
    /// "Remember on stop and resume"). On unless the user unticks it, as in
    /// IDM; when off, resuming starts it without its own limit.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub limit_remember: bool,
    /// Where its unfinished data is, when not at `path`: a temporary folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<String>,
    /// The browser extension took it from the browser.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub from_browser: bool,
    /// What to do once it finishes, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_done: Option<OnDone>,
    /// What the server said about the file when it was last downloaded or
    /// checked, so a synchronizing queue can tell whether it changed since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validator: Option<Validator>,
}

/// What a server says identifies one version of a file: its size, ETag and
/// Last-Modified date, whichever it sends.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Validator {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

impl Validator {
    /// Whether the server's copy described by `now` differs from the one
    /// this describes. Only what both sides know is compared: a different
    /// size always means a change; an ETag on both sides then decides alone
    /// (servers can touch a date without changing the file); without one a
    /// different Last-Modified means a change. When nothing can be compared
    /// the file counts as unchanged, so a server that says nothing about its
    /// files is not downloaded again on every run.
    pub fn changed(&self, now: &Validator) -> bool {
        if let (Some(a), Some(b)) = (self.size, now.size) {
            if a != b {
                return true;
            }
        }
        if let (Some(a), Some(b)) = (&self.etag, &now.etag) {
            return a != b;
        }
        matches!((&self.last_modified, &now.last_modified), (Some(a), Some(b)) if a != b)
    }
}

/// What to do once one download finishes, set from its window or
/// Properties. It runs once, after the virus scan if there is one, and is
/// then cleared. A download that fails does none of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OnDone {
    /// Show the completion dialog for it, whatever Options > When done says.
    #[serde(default)]
    pub dialog: bool,
    /// Then exit UIDM, or sleep, hibernate or shut down after the usual
    /// countdown.
    #[serde(default)]
    pub then: WhenDone,
    /// Shutting down closes programs that would hold it up.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force_close: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(size: Option<u64>, etag: Option<&str>, date: Option<&str>) -> Validator {
        Validator { size, etag: etag.map(str::to_string), last_modified: date.map(str::to_string) }
    }

    #[test]
    fn what_a_browser_request_may_carry_is_limited() {
        let mut d = Download {
            url: "https://x/a".into(),
            request_headers: vec![
                ("Accept".into(), "*/*".into()),
                ("Bad Name".into(), "x".into()),
                ("X-Split".into(), "a\r\nSet-Cookie: b".into()),
                ("X-Long".into(), "v".repeat(MAX_HEADER_VALUE + 1)),
            ],
            redirects: vec![
                "https://x/1".into(),
                "javascript:alert(1)".into(),
                format!("https://x/{}", "a".repeat(MAX_URL)),
            ],
            content_type: Some("a\nb".into()),
            browser_proxy: Some(BrowserProxy {
                pac_script: Some("x".repeat(MAX_PAC_SCRIPT + 1)),
                bypass: vec!["".into(), "*.corp".into(), "h".repeat(300)],
                ..BrowserProxy::default()
            }),
            ..Download::default()
        };
        d.request_headers.extend((0..100).map(|i| (format!("X-{i}"), "1".to_string())));
        d.limit_browser_request();
        assert_eq!(d.request_headers.len(), MAX_REQUEST_HEADERS);
        assert_eq!(d.request_headers[0].0, "Accept");
        assert_eq!(d.request_headers[1].0, "X-0", "the bad ones are gone");
        assert_eq!(d.redirects, ["https://x/1"]);
        assert_eq!(d.content_type, None);
        let proxy = d.browser_proxy.unwrap();
        assert_eq!((proxy.pac_script, proxy.bypass), (None, vec!["*.corp".to_string()]));
    }

    #[test]
    fn a_server_copy_changes_by_size_etag_or_date() {
        const MON: &str = "Mon, 02 Nov 2026 10:00:00 GMT";
        const TUE: &str = "Tue, 03 Nov 2026 10:00:00 GMT";
        let old = v(Some(1000), Some("\"a\""), Some(MON));
        assert!(!old.changed(&old.clone()), "the same file");
        assert!(old.changed(&v(Some(1001), Some("\"a\""), Some(MON))), "a new size");
        assert!(old.changed(&v(Some(1000), Some("\"b\""), Some(MON))), "a new ETag");
        assert!(!old.changed(&v(Some(1000), Some("\"a\""), Some(TUE))), "the ETag outranks a touched date");
        // Without ETags the date decides.
        let dated = v(Some(1000), None, Some(MON));
        assert!(dated.changed(&v(Some(1000), None, Some(TUE))));
        assert!(!dated.changed(&v(Some(1000), None, Some(MON))));
        assert!(!dated.changed(&v(Some(1000), Some("\"a\""), None)), "nothing new to compare");
        // A size alone (from before validators were kept) still counts.
        assert!(v(Some(10), None, None).changed(&v(Some(20), Some("\"x\""), Some(TUE))));
        assert!(!v(Some(10), None, None).changed(&v(Some(10), Some("\"x\""), Some(TUE))));
        // A server that says nothing is not downloaded again every run.
        assert!(!v(None, None, None).changed(&v(None, None, None)));
        assert!(!v(Some(10), None, None).changed(&v(None, None, None)));
    }
}
