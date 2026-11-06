//! The daemon's download list on disk, so it survives restarts.
//!
//! - `%LOCALAPPDATA%\UIDM\history.json` holds every download's entry (URL,
//!   file, size, progress, status) and the next id to hand out. It never holds
//!   cookies or request headers.
//! - `%LOCALAPPDATA%\UIDM\requests.bin` holds the original requests of
//!   downloads that can still be retried, cookies included, encrypted with
//!   DPAPI for the current user. It is deleted when no such request is left.
//!
//! Both are written through a temp file and a rename, so a crash never leaves
//! a half-written file.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dpapi;
use crate::protocol::{Download, DownloadEntry, Status};
use crate::settings::write_atomic;
use uidm_domain::queue::{Queue, MAIN_QUEUE};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct History {
    /// The id the next download gets, so ids keep increasing across restarts.
    pub next_id: u64,
    /// Every download, oldest first.
    pub items: Vec<DownloadEntry>,
    /// The queues, the main queue first.
    #[serde(default)]
    pub queues: Vec<Queue>,
}

impl History {
    /// Read the history at `path`. A missing file is an empty history.
    /// Downloads that were queued or running when it was written are marked
    /// interrupted, since nothing is running for them any more, unless they
    /// belong to a queue: those wait in it again. See `tidy_queues` for the
    /// rest.
    pub fn load(path: &Path) -> io::Result<History> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let mut history = History::default();
                history.tidy_queues(false);
                return Ok(history);
            }
            Err(e) => return Err(e),
        };
        let mut history: History =
            serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        // Every daemon with queues saves at least the main one, so a history
        // without any is from before queues.
        let before_queues = history.queues.is_empty();
        history.tidy_queues(before_queues);
        let queued: std::collections::HashSet<u64> =
            history.queues.iter().flat_map(|q| q.order.iter().copied()).collect();
        for e in &mut history.items {
            if !e.status.is_terminal() {
                e.status = if queued.contains(&e.id) { Status::Waiting } else { Status::Interrupted };
            }
            // The question it was waiting on went away with the daemon.
            e.duplicate_of = None;
            // Before the parent web page was kept apart from the referrer,
            // the referrer stood for both, so "By Parent Web Page" keeps
            // sorting old rows by it.
            if e.page.is_none() {
                e.page = e.referrer.clone();
            }
        }
        let max_id = history.items.iter().map(|e| e.id).max().unwrap_or(0);
        history.next_id = history.next_id.max(max_id + 1);
        Ok(history)
    }

    /// Make the queues consistent with the downloads: the main queue exists
    /// and comes first, every queue lists only downloads that exist, no
    /// download is in two queues, and finished downloads leave a one-time
    /// queue (a synchronizing queue keeps them, to check each run). With `before_queues` (a history saved before there were queues) every
    /// waiting download joins the main queue. Otherwise a waiting download
    /// outside every queue stays there: the user took it out with "Remove
    /// from queue", and putting it back would let a queue start it.
    pub fn tidy_queues(&mut self, before_queues: bool) {
        if !self.queues.iter().any(|q| q.id == MAIN_QUEUE) {
            self.queues.insert(0, Queue::main());
        }
        self.queues.sort_by_key(|q| q.id != MAIN_QUEUE);
        let status: HashMap<u64, Status> = self.items.iter().map(|e| (e.id, e.status)).collect();
        let mut seen = std::collections::HashSet::new();
        for q in &mut self.queues {
            let keep_done = q.kind.is_sync();
            q.order.retain(|id| status.get(id).is_some_and(|s| keep_done || *s != Status::Done) && seen.insert(*id));
        }
        for e in self.items.iter().filter(|_| before_queues) {
            if e.status == Status::Waiting && !seen.contains(&e.id) {
                self.queues[0].order.push(e.id);
            }
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        write_atomic(path, &bytes)
    }
}

/// Read the saved requests at `path`. A missing file means none.
pub fn load_requests(path: &Path) -> io::Result<HashMap<u64, Download>> {
    let sealed = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(e),
    };
    let plain = dpapi::unprotect(&sealed)?;
    let list: Vec<(u64, Download)> =
        serde_json::from_slice(&plain).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(list.into_iter().collect())
}

/// Encrypt and save `requests` to `path`, or delete the file when empty.
pub fn save_requests(path: &Path, requests: &HashMap<u64, Download>) -> io::Result<()> {
    if requests.is_empty() {
        return match fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let mut list: Vec<(&u64, &Download)> = requests.iter().collect();
    list.sort_by_key(|(id, _)| **id);
    let plain = serde_json::to_vec(&list).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_atomic(path, &dpapi::protect(&plain)?)
}

fn local_data_dir() -> io::Result<PathBuf> {
    crate::platform::data_dir()
}

/// `%LOCALAPPDATA%\UIDM\history.json`.
pub fn history_path() -> io::Result<PathBuf> {
    Ok(local_data_dir()?.join("history.json"))
}

/// `%LOCALAPPDATA%\UIDM\requests.bin`.
pub fn requests_path() -> io::Result<PathBuf> {
    Ok(local_data_dir()?.join("requests.bin"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("uidm_history_{}_{name}", std::process::id()))
    }

    fn entry(id: u64, status: Status) -> DownloadEntry {
        DownloadEntry {
            id,
            url: format!("https://x/{id}.zip"),
            path: Some(format!("C:\\dl\\{id}.zip")),
            total: Some(1000),
            done: 400,
            status,
            error: None,
            resumable: true,
            connections: 4,
            scan: None,
            parts: None,
            note: None,
            duplicate_of: None,
            referrer: None,
            page: None,
            last_try: None,
            description: None,
            category: None,
            login_user: None,
            speed_limit: None,
            limit_remember: true,
            partial: None,
            from_browser: false,
            on_done: None,
            validator: None,
        }
    }

    #[test]
    fn missing_file_is_empty() {
        let h = History::load(&temp_path("missing.json")).unwrap();
        assert!(h.items.is_empty());
        assert!(load_requests(&temp_path("missing.bin")).unwrap().is_empty());
    }

    #[test]
    fn old_entries_take_the_referrer_as_their_page() {
        let path = temp_path("page.json");
        let mut old = serde_json::to_value(entry(1, Status::Done)).unwrap();
        old["referrer"] = "https://x/list".into();
        assert!(old.get("page").is_none(), "saved before pages were kept");
        let mut new = entry(2, Status::Done);
        new.referrer = Some("https://x/frame".into());
        new.page = Some("https://x/article".into());
        let history = serde_json::json!({ "next_id": 3, "items": [old, new] });
        fs::write(&path, serde_json::to_vec(&history).unwrap()).unwrap();
        let h = History::load(&path).unwrap();
        assert_eq!(h.items[0].referrer.as_deref(), Some("https://x/list"));
        assert_eq!(h.items[0].page.as_deref(), Some("https://x/list"));
        assert_eq!(h.items[1].referrer.as_deref(), Some("https://x/frame"));
        assert_eq!(h.items[1].page.as_deref(), Some("https://x/article"));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn running_downloads_load_as_interrupted() {
        let path = temp_path("interrupted.json");
        History {
            queues: Vec::new(),
            next_id: 5,
            items: vec![
                entry(1, Status::Done),
                entry(2, Status::Downloading),
                entry(3, Status::Queued),
                entry(4, Status::Failed),
            ],
        }
        .save(&path)
        .unwrap();
        let h = History::load(&path).unwrap();
        let statuses: Vec<Status> = h.items.iter().map(|e| e.status).collect();
        assert_eq!(statuses, [Status::Done, Status::Interrupted, Status::Interrupted, Status::Failed]);
        assert_eq!(h.items[1].done, 400, "progress is kept");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn queues_load_tidy_and_keep_their_downloads_waiting() {
        let path = temp_path("queues.json");
        let mut night = Queue::new(2, "Night");
        night.order = vec![3, 1, 2, 99, 3];
        night.running = true;
        History {
            next_id: 6,
            items: vec![
                entry(1, Status::Done),
                entry(2, Status::Downloading),
                entry(3, Status::Waiting),
                entry(4, Status::Waiting),
                entry(5, Status::Downloading),
            ],
            queues: vec![night],
        }
        .save(&path)
        .unwrap();
        let h = History::load(&path).unwrap();
        assert_eq!(h.queues[0].id, MAIN_QUEUE, "the main queue is added first");
        assert_eq!(h.queues[1].order, [3, 2], "finished, missing and repeated ids are dropped");
        assert!(h.queues[0].order.is_empty(), "a download taken out of its queue stays out");
        assert!(h.queues[1].running, "a running queue stays started");
        let status = |id: u64| h.items.iter().find(|e| e.id == id).unwrap().status;
        assert_eq!(status(2), Status::Waiting, "a queued download waits again");
        assert_eq!(status(4), Status::Waiting, "and one outside every queue still waits");
        assert_eq!(status(5), Status::Interrupted, "a download outside queues is interrupted");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn a_synchronizing_queue_keeps_its_finished_downloads() {
        let path = temp_path("sync_queue.json");
        let mut sync = Queue::new(2, "Mirror");
        sync.kind = uidm_domain::queue::QueueKind::Synchronize;
        sync.order = vec![1, 2];
        let mut once = Queue::new(3, "Once");
        once.order = vec![3, 4];
        History {
            next_id: 5,
            items: vec![
                entry(1, Status::Done),
                entry(2, Status::Waiting),
                entry(3, Status::Done),
                entry(4, Status::Waiting),
            ],
            queues: vec![Queue::main(), sync, once],
        }
        .save(&path)
        .unwrap();
        let h = History::load(&path).unwrap();
        assert_eq!(h.queues[1].kind, uidm_domain::queue::QueueKind::Synchronize, "the type is saved");
        assert_eq!(h.queues[1].order, [1, 2], "a finished download stays to be checked");
        assert_eq!(h.queues[2].order, [4], "a one-time queue lets it go");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn waiting_downloads_from_before_queues_join_the_main_queue() {
        let path = temp_path("before_queues.json");
        History { next_id: 3, items: vec![entry(1, Status::Waiting), entry(2, Status::Failed)], queues: Vec::new() }
            .save(&path)
            .unwrap();
        let h = History::load(&path).unwrap();
        assert_eq!(h.queues.len(), 1);
        assert_eq!(h.queues[0].order, [1], "only the waiting download joins");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn next_id_is_never_below_a_saved_id() {
        let path = temp_path("next_id.json");
        History { next_id: 1, items: vec![entry(41, Status::Done)], queues: Vec::new() }.save(&path).unwrap();
        assert_eq!(History::load(&path).unwrap().next_id, 42);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn requests_are_encrypted_and_never_in_the_history() {
        let mut requests = HashMap::new();
        requests.insert(
            2,
            Download {
                url: "https://x/2.zip".into(),
                cookies: Some("session=topsecret".into()),
                headers: [("Authorization".to_string(), "Bearer hunter2".to_string())].into(),
                ..Download::default()
            },
        );
        let req_path = temp_path("requests.bin");
        save_requests(&req_path, &requests).unwrap();
        let raw = fs::read(&req_path).unwrap();
        for secret in [&b"topsecret"[..], b"hunter2"] {
            assert!(!raw.windows(secret.len()).any(|w| w == secret));
        }
        let back = load_requests(&req_path).unwrap();
        assert_eq!(back[&2].cookies.as_deref(), Some("session=topsecret"));

        // Saving none deletes the file.
        save_requests(&req_path, &HashMap::new()).unwrap();
        assert!(!req_path.exists());

        let hist_path = temp_path("no-secrets.json");
        History { next_id: 3, items: vec![entry(2, Status::Downloading)], queues: Vec::new() }
            .save(&hist_path)
            .unwrap();
        let text = fs::read_to_string(&hist_path).unwrap().to_lowercase();
        assert!(!text.contains("cookie") && !text.contains("header"));
        fs::remove_file(&hist_path).ok();
    }
}
