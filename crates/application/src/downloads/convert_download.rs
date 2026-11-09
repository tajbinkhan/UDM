use std::path::{Path, PathBuf};

use pullbay_domain::download::{DownloadError, Status};
use pullbay_domain::filename::numbered_name;
use pullbay_domain::media::{Conversion, Target};
use pullbay_domain::ports::{Converter, DownloadFiles, DownloadList};

/// Convert a finished download's file with ffmpeg: to an MP4, or its sound
/// alone to an MP3 or M4A. The new file is a row of its own, beside the
/// original under a free name.
pub struct ConvertDownload<'a, D, F, C> {
    downloads: &'a D,
    files: &'a F,
    converter: &'a C,
}

impl<'a, D: DownloadList, F: DownloadFiles, C: Converter> ConvertDownload<'a, D, F, C> {
    pub fn new(downloads: &'a D, files: &'a F, converter: &'a C) -> Self {
        ConvertDownload { downloads, files, converter }
    }

    /// Start converting download `id` to `to`. Returns the new file's path.
    pub fn run(&self, id: u64, to: Target, delete_original: bool) -> Result<PathBuf, DownloadError> {
        let entry = self.downloads.entry(id).ok_or(DownloadError::NoDownload(id))?;
        let input = entry
            .path
            .as_deref()
            .map(PathBuf::from)
            .filter(|p| entry.status == Status::Done && self.files.is_file(p))
            .ok_or(DownloadError::NotConvertible)?;
        if !self.converter.available() {
            return Err(DownloadError::NoFfmpeg);
        }
        let name = to.converted_name(&input);
        let dir = input.parent().map(Path::to_path_buf).unwrap_or_default();
        // Numbered names never run out; the first free one is taken.
        let output =
            (0..).map(|n| dir.join(numbered_name(&name, n))).find(|p| !self.files.exists(p)).unwrap_or_default();
        self.converter.start(Conversion {
            from: id,
            url: entry.url,
            input,
            output: output.clone(),
            target: to,
            delete_original,
        });
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, Convert, Downloads, Files};

    fn finished(path: &str) -> Downloads {
        let mut e = row(1, Status::Done);
        e.path = Some(path.into());
        Downloads::rows([e])
    }

    #[test]
    fn the_converted_file_takes_a_free_name_beside_the_original() {
        let downloads = finished("/dl/clip.webm");
        let taken = Path::new("/dl").join("clip.mp4");
        let files = Files::with_existing(&["/dl/clip.webm", taken.to_str().unwrap()]);
        let converter = Convert::with_ffmpeg();
        let out = ConvertDownload::new(&downloads, &files, &converter).run(1, Target::Mp4, true).unwrap();
        assert_eq!(out, Path::new("/dl").join("clip (1).mp4"));
        let c = &converter.started()[0];
        assert_eq!((c.from, c.input.as_path(), c.delete_original), (1, Path::new("/dl/clip.webm"), true));
    }

    #[test]
    fn it_needs_a_finished_file_and_ffmpeg() {
        let files = Files::with_existing(&["/dl/clip.webm"]);
        let with = Convert::with_ffmpeg();
        let gone = finished("/dl/moved.webm");
        assert_eq!(
            ConvertDownload::new(&gone, &files, &with).run(1, Target::Mp3, false),
            Err(DownloadError::NotConvertible)
        );
        let running = Downloads::with(&[(1, Status::Downloading)]);
        assert_eq!(
            ConvertDownload::new(&running, &files, &with).run(1, Target::Mp3, false),
            Err(DownloadError::NotConvertible)
        );
        let without = Convert::default();
        let done = finished("/dl/clip.webm");
        assert_eq!(
            ConvertDownload::new(&done, &files, &without).run(1, Target::Mp3, false),
            Err(DownloadError::NoFfmpeg)
        );
        assert!(with.started().is_empty() && without.started().is_empty());
    }
}
