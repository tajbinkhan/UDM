//! Downloads for the tests, read from the JSON the daemon sends, since
//! `DownloadEntry` has no constructor of its own.

use udm_domain::download::{DownloadEntry, Status};

/// Download `id` named `name`, in `status`, of `total` bytes with `done`
/// downloaded, saved in `/home/u/Downloads`.
pub fn entry(id: u64, name: &str, status: Status, total: u64, done: u64) -> DownloadEntry {
    let mut e: DownloadEntry = serde_json::from_value(serde_json::json!({
        "id": id,
        "url": format!("http://127.0.0.1:8765/{name}"),
        "path": format!("/home/u/Downloads/{name}"),
        "total": total,
        "done": done,
        "status": "queued",
        "error": null,
        "resumable": true,
        "connections": 8,
    }))
    .unwrap();
    e.status = status;
    e
}
