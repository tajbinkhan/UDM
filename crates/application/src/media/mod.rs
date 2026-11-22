//! Reading what a stream or a video page offers (through the `Probes`
//! port), for the browser extension's video menu. A video page's answer is
//! kept a while (`PageAnswers`), since each read is a yt-dlp run.

mod read_media;

pub use read_media::ReadMedia;
