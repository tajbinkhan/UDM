use std::path::PathBuf;

use pullbay_domain::download::{DownloadError, Properties, Status};
use pullbay_domain::id::DownloadId;
use pullbay_domain::ports::{DownloadFiles, DownloadList};

/// Change a stopped or finished download's details. The link, referrer
/// and login go into the request a retry starts from; a finished
/// download's file is renamed or moved on disk first, and an unfinished
/// one's partial file moves with its folder.
pub struct SetProperties<'a, D, F> {
    downloads: &'a D,
    files: &'a F,
}

impl<'a, D: DownloadList, F: DownloadFiles> SetProperties<'a, D, F> {
    pub fn new(downloads: &'a D, files: &'a F) -> Self {
        SetProperties { downloads, files }
    }

    /// Returns where its file moved to, if it moved.
    pub fn run(&self, id: DownloadId, p: Properties) -> Result<Option<PathBuf>, DownloadError> {
        let entry = self.downloads.entry(id).ok_or(DownloadError::NoDownload(id))?;
        let plan = entry.plan_properties(p)?;
        let done = entry.status == Status::Done;
        if let Some((from, to)) = &plan.moved {
            let files = |e: pullbay_domain::ports::FilesError| DownloadError::Files(e.to_string());
            if done {
                self.files.move_finished(from, to).map_err(files)?;
            } else if entry.started() {
                self.files.move_partial(from, to).map_err(files)?;
            } else if self.files.exists(to) {
                // Never started: only a proposed path, which may be
                // somebody else's file, so nothing is moved.
                return Err(DownloadError::Exists(to.display().to_string()));
            }
        }
        let mut login_user = entry.login_user.clone();
        if !done {
            let mut req = self.downloads.request(id).unwrap_or_else(|| entry.base_request());
            plan.apply_to_request(&mut req);
            login_user = req.login.as_ref().map(|l| l.username.clone());
            self.downloads.keep_request(id, req);
        }
        let saved = self.downloads.update(id, |e| {
            if !e.properties_editable() {
                return false;
            }
            plan.apply_to_row(e, login_user);
            true
        });
        match saved {
            None => Err(DownloadError::NoDownload(id)),
            Some(false) => Err(DownloadError::StartedWhileSaving),
            Some(true) => Ok(plan.moved.map(|(_, to)| to)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, Downloads, Files};
    use pullbay_domain::download::DownloadLogin;

    fn full(p: &str) -> String {
        if cfg!(windows) {
            format!("C:{}", p.replace('/', "\\"))
        } else {
            p.to_string()
        }
    }

    #[test]
    fn a_finished_file_is_renamed_on_disk_and_in_the_list() {
        let mut e = row(1, Status::Done);
        e.connections = 1;
        e.path = Some(full("/dl/a.zip"));
        let downloads = Downloads::rows([e]);
        let files = Files::default();
        let p =
            Properties { filename: Some("b.zip".into()), description: Some(" new ".into()), ..Properties::default() };
        let to = SetProperties::new(&downloads, &files).run(DownloadId::new(1), p).unwrap();
        assert_eq!(to, Some(PathBuf::from(full("/dl/b.zip"))));
        assert_eq!(files.moved(), [(PathBuf::from(full("/dl/a.zip")), PathBuf::from(full("/dl/b.zip")), true)]);
        let e = downloads.entry(DownloadId::new(1)).unwrap();
        assert_eq!((e.path.as_deref(), e.description.as_deref()), (Some(full("/dl/b.zip").as_str()), Some("new")));
        assert!(downloads.request(DownloadId::new(1)).is_none(), "a finished download keeps no request");
    }

    #[test]
    fn a_stopped_download_gets_a_new_link_and_login_for_its_retry() {
        let downloads = Downloads::with(&[(1, Status::Failed)]);
        let files = Files::default();
        let p = Properties {
            url: Some("https://mirror.example/1.zip".into()),
            login: Some(DownloadLogin { username: "me".into(), password: Some("pw".into()) }),
            ..Properties::default()
        };
        SetProperties::new(&downloads, &files).run(DownloadId::new(1), p).unwrap();
        let req = downloads.request(DownloadId::new(1)).unwrap();
        assert_eq!(req.url, "https://mirror.example/1.zip");
        let e = downloads.entry(DownloadId::new(1)).unwrap();
        assert_eq!((e.url.as_str(), e.login_user.as_deref()), ("https://mirror.example/1.zip", Some("me")));
    }

    #[test]
    fn a_proposed_path_is_never_moved_onto_another_file() {
        let mut e = row(1, Status::Canceled);
        e.path = Some(full("/dl/1.zip"));
        let downloads = Downloads::rows([e]);
        let files = Files::with_existing(&[&full("/other/1.zip")]);
        let p = Properties { folder: Some(full("/other")), ..Properties::default() };
        let err = SetProperties::new(&downloads, &files).run(DownloadId::new(1), p).unwrap_err();
        assert_eq!(err, DownloadError::Exists(full("/other/1.zip")));
        assert!(files.moved().is_empty());
    }

    #[test]
    fn a_failed_move_changes_nothing() {
        let mut e = row(1, Status::Done);
        e.path = Some(full("/dl/a.zip"));
        let downloads = Downloads::rows([e]);
        let p = Properties { filename: Some("b.zip".into()), ..Properties::default() };
        let err = SetProperties::new(&downloads, &Files::failing()).run(DownloadId::new(1), p).unwrap_err();
        assert_eq!(err, DownloadError::Files("the file is open".into()));
        assert_eq!(downloads.entry(DownloadId::new(1)).unwrap().path, Some(full("/dl/a.zip")));
    }
}
