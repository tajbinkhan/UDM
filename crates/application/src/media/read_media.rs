use std::sync::Mutex;

use udm_domain::download::Download;
use udm_domain::media::{MediaInfo, PageAnswers};
use udm_domain::ports::{Probes, Timestamps};
use udm_domain::sync::LockExt;

/// What the stream or the video page `download` names offers: its
/// qualities, tracks and subtitles, or a playlist's videos. A video page
/// read less than `PAGE_ANSWER_KEPT_SECS` ago is answered from what it said
/// then, without asking yt-dlp again.
pub struct ReadMedia<'a, P: ?Sized, T: ?Sized> {
    probes: &'a P,
    clock: &'a T,
    answers: &'a Mutex<PageAnswers>,
}

impl<'a, P, T> ReadMedia<'a, P, T>
where
    P: Probes + ?Sized,
    T: Timestamps + ?Sized,
{
    pub fn new(probes: &'a P, clock: &'a T, answers: &'a Mutex<PageAnswers>) -> Self {
        ReadMedia { probes, clock, answers }
    }

    /// Runs on the caller's thread for as long as the read takes.
    pub fn run(&self, download: &Download) -> MediaInfo {
        let Some(page) = download.url.address() else {
            return self.probes.media(download);
        };
        if let Some(info) = self.answers.lock_or_recover().kept(page, self.clock.unix_now()) {
            tracing::debug!(url = %page.as_str(), "media answer kept");
            return info;
        }
        let info = self.probes.media(download);
        self.answers.lock_or_recover().keep(page, &info, self.clock.unix_now());
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{MediaPages, Seconds};
    use udm_domain::address::Address;
    use udm_domain::media::{MediaKind, MediaOption, PAGE_ANSWER_KEPT_SECS};

    fn offers(label: &str) -> MediaInfo {
        let video = vec![MediaOption { id: "18".into(), label: label.into(), ..MediaOption::default() }];
        MediaInfo { kind: Some(MediaKind::Page), video, ..MediaInfo::default() }
    }

    fn page(url: &str) -> Download {
        Download { url: Address::parse(url).unwrap().into(), ..Download::default() }
    }

    #[test]
    fn a_page_read_lately_is_not_read_again() {
        let pages = MediaPages::answering(offers("360p"));
        let (clock, answers) = (Seconds::at(100), Mutex::default());
        let read = ReadMedia::new(&pages, &clock, &answers);
        let video = page("https://www.youtube.com/watch?v=a1");
        assert_eq!(read.run(&video), offers("360p"));
        assert_eq!(read.run(&video), offers("360p"));
        assert_eq!(pages.asked(), 1, "one yt-dlp run");
        clock.set(100 + PAGE_ANSWER_KEPT_SECS);
        read.run(&video);
        assert_eq!(pages.asked(), 2, "an old answer is read again");
        read.run(&page("https://www.youtube.com/watch?v=b2"));
        assert_eq!(pages.asked(), 3, "another page is its own");
    }

    #[test]
    fn a_failure_is_asked_about_again() {
        let pages = MediaPages::answering(MediaInfo::failed(Some(MediaKind::Page), "yt-dlp: timed out".into()));
        let (clock, answers) = (Seconds::at(0), Mutex::default());
        let read = ReadMedia::new(&pages, &clock, &answers);
        let video = page("https://www.youtube.com/watch?v=a1");
        assert!(read.run(&video).error.is_some());
        assert!(read.run(&video).error.is_some());
        assert_eq!(pages.asked(), 2);
    }
}
