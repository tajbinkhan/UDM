use std::sync::Mutex;

use pullbay_domain::gallery::{gallery_page, Gallery, GalleryError, GalleryReads};
use pullbay_domain::ports::{Probes, Timestamps};
use pullbay_domain::sync::LockExt;

/// "Download gallery": list the pictures gallery-dl finds on a page. Only
/// http and https pages are read, one read of a page at a time and at most
/// `MAX_READS` at once (each is a gallery-dl process), and a page that held
/// nothing is not read again for a while (`GalleryReads`).
pub struct ReadGallery<'a, P: ?Sized, T: ?Sized> {
    probes: &'a P,
    clock: &'a T,
    reads: &'a Mutex<GalleryReads>,
}

impl<'a, P, T> ReadGallery<'a, P, T>
where
    P: Probes + ?Sized,
    T: Timestamps + ?Sized,
{
    pub fn new(probes: &'a P, clock: &'a T, reads: &'a Mutex<GalleryReads>) -> Self {
        ReadGallery { probes, clock, reads }
    }

    /// The pictures of the gallery at `page`, read with the browser's
    /// `user_agent` when it sent one. Runs on the caller's thread for as
    /// long as gallery-dl takes, up to its timeout.
    pub fn run(&self, page: &str, user_agent: Option<&str>) -> Result<Gallery, GalleryError> {
        let page = gallery_page(page)?;
        self.reads.lock_or_recover().begin(&page, self.clock.unix_now())?;
        let user_agent = user_agent
            .map(|ua| ua.chars().filter(|c| !c.is_control()).take(512).collect::<String>())
            .filter(|ua| !ua.trim().is_empty());
        let found = self.probes.gallery(&page, user_agent.as_deref()).map(Gallery::cleaned).and_then(|g| {
            if g.pictures.is_empty() {
                Err(GalleryError::NothingFound)
            } else {
                Ok(g)
            }
        });
        self.reads.lock_or_recover().end(&page, &found, self.clock.unix_now());
        match &found {
            Ok(g) => tracing::info!(page = %page.as_str(), pictures = g.pictures.len(), "gallery read"),
            Err(e) => tracing::info!(page = %page.as_str(), error = %e, "no gallery"),
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{GalleryPages, Seconds};
    use pullbay_domain::gallery::{Picture, MISS_KEPT_SECS};

    fn pictures(urls: &[&str]) -> Gallery {
        Gallery {
            title: "Cats".into(),
            pictures: urls.iter().map(|u| Picture { url: u.to_string(), ..Picture::default() }).collect(),
        }
    }

    #[test]
    fn a_page_lists_its_web_pictures() {
        let pages = GalleryPages::with(
            "https://g.example/cats",
            Ok(pictures(&["https://i/1.jpg", "text:x", "https://i/1.jpg"])),
        );
        let (clock, reads) = (Seconds::at(100), Mutex::default());
        let g = ReadGallery::new(&pages, &clock, &reads).run(" https://g.example/cats ", Some("Agent\n1")).unwrap();
        assert_eq!(g.pictures.len(), 1, "cleaned");
        assert_eq!(pages.asked(), [("https://g.example/cats".to_string(), Some("Agent1".to_string()))]);
    }

    #[test]
    fn other_addresses_never_reach_gallery_dl() {
        let pages = GalleryPages::default();
        let (clock, reads) = (Seconds::at(0), Mutex::default());
        let read = ReadGallery::new(&pages, &clock, &reads);
        for page in ["ftp://g.example/", "file:///etc/passwd", "--help", "javascript:alert(1)"] {
            assert!(matches!(read.run(page, None), Err(GalleryError::NotWebPage(_))), "{page}");
        }
        assert!(pages.asked().is_empty());
    }

    #[test]
    fn a_page_with_nothing_is_not_read_again_for_a_while() {
        let pages = GalleryPages::with("https://g.example/empty", Ok(pictures(&["text:only"])));
        let (clock, reads) = (Seconds::at(1000), Mutex::default());
        let read = ReadGallery::new(&pages, &clock, &reads);
        assert_eq!(read.run("https://g.example/empty", None), Err(GalleryError::NothingFound));
        assert_eq!(read.run("https://g.example/empty", None), Err(GalleryError::NothingFound));
        assert_eq!(pages.asked().len(), 1, "the miss was kept");
        clock.set(1000 + MISS_KEPT_SECS);
        assert_eq!(read.run("https://g.example/empty", None), Err(GalleryError::NothingFound));
        assert_eq!(pages.asked().len(), 2);
    }

    #[test]
    fn a_failure_can_be_tried_again_at_once() {
        let pages = GalleryPages::with("https://g.example/slow", Err(GalleryError::TimedOut(55)));
        let (clock, reads) = (Seconds::at(0), Mutex::default());
        let read = ReadGallery::new(&pages, &clock, &reads);
        assert_eq!(read.run("https://g.example/slow", None), Err(GalleryError::TimedOut(55)));
        assert_eq!(read.run("https://g.example/slow", None), Err(GalleryError::TimedOut(55)));
        assert_eq!(pages.asked().len(), 2);
        assert_eq!(read.run("https://g.example/unknown", None), Err(GalleryError::Unsupported));
    }
}
