//! Streamed video: HLS playlists (`.m3u8`), DASH manifests (`.mpd`) and HDS
//! manifests (`.f4m`).
//!
//! A stream is a list of short segments rather than one file. This module
//! reads playlists and manifests into `Track`s (the segments of one video or
//! audio rendition, with their keys) and describes the qualities a stream
//! offers. The engine fetches the segments and joins them (see
//! `engine::stream`).
//!
//! Streams protected by DRM, and live streams, are refused: UIDM does not get
//! around DRM, and a live stream has no end to download to.
//!
//! `convert` turns a finished video into an MP4, or its sound into an MP3
//! or M4A, with ffmpeg. `ytdlp` reads video pages that name no stream at
//! all (YouTube, Facebook, Instagram, Vimeo) with yt-dlp.

/// [`uidm_domain::media::resolve`], with its error as text, which the
/// playlist and manifest readers still report.
pub(crate) fn resolve(base: &str, url: &str) -> Result<String, String> {
    uidm_domain::media::resolve(base, url).map_err(|e| e.to_string())
}

pub mod convert;
pub mod dash;
pub mod hds;
pub mod hls;
pub mod ytdlp;
