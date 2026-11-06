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

pub mod convert;
pub mod dash;
pub mod hds;
pub mod hls;
pub mod ytdlp;

use serde::{Deserialize, Serialize};

/// The stream formats, and a video page yt-dlp reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Hls,
    Dash,
    /// Adobe HTTP Dynamic Streaming: an F4M manifest and F4F fragments.
    Hds,
    Page,
}

/// Which stream to download, and in which quality.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaChoice {
    /// The format; `None` finds out from the playlist itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<MediaKind>,
    /// The video quality: an option `id` from `MediaInfo`. `None` takes the
    /// best.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<String>,
    /// The audio track: an option `id` from `MediaInfo`. `None` takes the
    /// default, or the best.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<String>,
    /// Keep only the sound, as this (`mp3` or `m4a`). Needs ffmpeg; only
    /// the audio track is fetched when the stream has one of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_only: Option<convert::Target>,
    /// The subtitle tracks to keep: option `id`s from `MediaInfo`'s
    /// `subtitles`. `None` keeps the stream's default ones; an empty list
    /// keeps none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitles: Option<Vec<String>>,
}

/// One quality or audio track a stream offers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaOption {
    /// What to put in `MediaChoice` to get this one.
    pub id: String,
    /// For people: "1080p, 5.2 Mbps" or "English".
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Bits per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<u64>,
    /// About how big it is, from its bit rate and the length of the video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// A subtitle track's language, as the stream names it (`en`, `de-AT`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The stream marks this track as the one to use by default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
}

/// What a stream offers, or why it cannot be downloaded.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<MediaKind>,
    /// Video qualities, best first.
    #[serde(default)]
    pub video: Vec<MediaOption>,
    /// Separate audio tracks, if the stream has them.
    #[serde(default)]
    pub audio: Vec<MediaOption>,
    /// Subtitle tracks (WebVTT) the stream offers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitles: Vec<MediaOption>,
    /// Length in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    /// The stream is protected by DRM and will not be downloaded.
    #[serde(default)]
    pub drm: bool,
    /// Why the stream cannot be downloaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The address answered with something that is not a playlist or a
    /// manifest at all: a `.m3u8` a page's script decodes itself, or one the
    /// browser caught that is no stream. The menu leaves it out when there is
    /// something to download beside it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not_stream: bool,
    /// The address is a whole video or audio file, not a stream, though its
    /// name or type said it was a playlist: the browser downloads it as a
    /// file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<MediaFile>,
}

/// A whole video or audio file found where a stream was expected.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaFile {
    /// Its type, as a file extension: `mp4`, `ts`, `webm`.
    pub ext: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

impl MediaInfo {
    pub fn failed(kind: Option<MediaKind>, error: String) -> Self {
        MediaInfo { kind, error: Some(error), ..MediaInfo::default() }
    }
}

/// The key of an AES-128 encrypted segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentKey {
    pub url: String,
    pub iv: [u8; 16],
}

/// One piece of a track: a URL, maybe only a byte range of it, maybe
/// encrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub url: String,
    /// Inclusive byte range.
    pub range: Option<(u64, u64)>,
    pub key: Option<SegmentKey>,
}

impl Segment {
    pub fn whole(url: impl Into<String>) -> Self {
        Segment { url: url.into(), range: None, key: None }
    }
}

/// The segments of one video or audio rendition, in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    /// A header every segment needs (fragmented MP4), fetched once.
    pub init: Option<Segment>,
    pub segments: Vec<Segment>,
    /// The file extension the joined track has: `ts`, `mp4`, `m4a`, `webm`.
    pub ext: &'static str,
    /// Length in seconds, when known.
    pub duration: Option<f64>,
    /// HDS: the segments are F4F fragments, and the joined track is this
    /// (an FLV header and the stream's metadata) followed by each
    /// fragment's FLV tags (its `mdat`), not the fragments as they are.
    pub hds_header: Option<Vec<u8>>,
}

/// A subtitle track: WebVTT, in one file or in segments.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Subtitle {
    /// For people, and the track's title in the video: "English".
    pub label: String,
    pub language: Option<String>,
    pub segments: Vec<Segment>,
}

/// One WebVTT file from the segments of a subtitle track: a `WEBVTT`
/// header, then every segment's cues. Each segment's own header block goes,
/// with HLS's `X-TIMESTAMP-MAP` in it, which leaves cue times counted from
/// the start of the video, as players show them.
pub fn join_webvtt(parts: &[String]) -> String {
    let mut out = String::from("WEBVTT\n");
    for part in parts {
        let text = part.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
        // The header block runs to the first blank line.
        let body = match text.trim_start().strip_prefix("WEBVTT") {
            Some(rest) => rest.split_once("\n\n").map_or("", |(_, cues)| cues),
            None => text.as_str(),
        };
        let body = body.trim_matches('\n');
        if !body.is_empty() {
            out.push('\n');
            out.push_str(body);
            out.push('\n');
        }
    }
    out
}

/// A language tag's main part, for a file name or ffmpeg: `en-US` is `en`.
pub fn language_code(tag: &str) -> Option<String> {
    let main = tag.split(['-', '_']).next()?.trim().to_ascii_lowercase();
    (!main.is_empty() && main.len() <= 3 && main.chars().all(|c| c.is_ascii_alphabetic())).then_some(main)
}

/// A language tag as the three-letter ISO 639-2 code MP4 files need
/// (`en-US` is `eng`). `None` for a two-letter code it does not know.
pub fn iso639_2(tag: &str) -> Option<String> {
    const TWO_TO_THREE: &[(&str, &str)] = &[
        ("ar", "ara"),
        ("bg", "bul"),
        ("bn", "ben"),
        ("ca", "cat"),
        ("cs", "ces"),
        ("da", "dan"),
        ("de", "deu"),
        ("el", "ell"),
        ("en", "eng"),
        ("es", "spa"),
        ("et", "est"),
        ("fa", "fas"),
        ("fi", "fin"),
        ("fil", "fil"),
        ("fr", "fra"),
        ("he", "heb"),
        ("hi", "hin"),
        ("hr", "hrv"),
        ("hu", "hun"),
        ("id", "ind"),
        ("is", "isl"),
        ("it", "ita"),
        ("ja", "jpn"),
        ("ko", "kor"),
        ("lt", "lit"),
        ("lv", "lav"),
        ("ms", "msa"),
        ("nb", "nob"),
        ("nl", "nld"),
        ("no", "nor"),
        ("pl", "pol"),
        ("pt", "por"),
        ("ro", "ron"),
        ("ru", "rus"),
        ("sk", "slk"),
        ("sl", "slv"),
        ("sr", "srp"),
        ("sv", "swe"),
        ("ta", "tam"),
        ("te", "tel"),
        ("th", "tha"),
        ("tr", "tur"),
        ("uk", "ukr"),
        ("ur", "urd"),
        ("vi", "vie"),
        ("zh", "zho"),
    ];
    let code = language_code(tag)?;
    match code.len() {
        3 => Some(code),
        2 => TWO_TO_THREE.iter().find(|(two, _)| *two == code).map(|(_, three)| three.to_string()),
        _ => None,
    }
}

/// Why a stream is refused, in words for the user.
pub const DRM_REFUSED: &str = "This video is protected by DRM (copy protection), so UIDM will not download it.";
pub const LIVE_REFUSED: &str = "This is a live stream. UIDM downloads recorded videos, which have an end.";

/// `url` resolved against `base`, as a browser would.
pub fn resolve(base: &str, url: &str) -> Result<String, String> {
    let base = url::Url::parse(base).map_err(|e| format!("bad address {base}: {e}"))?;
    let joined = base.join(url.trim()).map_err(|e| format!("bad address {url}: {e}"))?;
    match joined.scheme() {
        "http" | "https" => Ok(joined.to_string()),
        other => Err(format!("{other}: addresses are not supported in a stream")),
    }
}

/// "1080p, 5.2 Mbps", as much of it as is known.
pub fn quality_label(height: Option<u32>, bandwidth: Option<u64>) -> String {
    let mut parts = Vec::new();
    if let Some(h) = height {
        parts.push(format!("{h}p"));
    }
    if let Some(b) = bandwidth {
        if b >= 1_000_000 {
            parts.push(format!("{:.1} Mbps", b as f64 / 1e6));
        } else {
            parts.push(format!("{} kbps", (b / 1000).max(1)));
        }
    }
    if parts.is_empty() {
        "Default".to_string()
    } else {
        parts.join(", ")
    }
}

/// About how many bytes `seconds` of a `bandwidth` bits-per-second stream
/// take.
pub fn estimate_size(bandwidth: Option<u64>, seconds: Option<f64>) -> Option<u64> {
    let (b, s) = (bandwidth?, seconds?);
    (s > 0.0).then(|| (b as f64 * s / 8.0) as u64)
}

/// Whether `text` looks like an HLS playlist, a DASH manifest or an HDS
/// manifest.
pub fn sniff(text: &str) -> Option<MediaKind> {
    let head = text.trim_start_matches('\u{feff}').trim_start();
    if head.starts_with("#EXTM3U") {
        Some(MediaKind::Hls)
    } else if head.starts_with('<') && head.as_bytes()[..head.len().min(4096)].windows(4).any(|w| w == b"<MPD") {
        Some(MediaKind::Dash)
    } else if hds::sniff(head) {
        Some(MediaKind::Hds)
    } else {
        None
    }
}

/// The kind of video or audio file that starts with `head`, as a file
/// extension, or `None` when it is no media file UIDM knows.
pub fn sniff_file(head: &[u8]) -> Option<&'static str> {
    let at = |i: usize, sig: &[u8]| head.get(i..i + sig.len()) == Some(sig);
    if at(4, b"ftyp") {
        return Some(if at(8, b"M4A ") { "m4a" } else { "mp4" });
    }
    if at(0, &[0x1a, 0x45, 0xdf, 0xa3]) {
        let doc = &head[..head.len().min(64)];
        return Some(if doc.windows(8).any(|w| w == b"matroska") { "mkv" } else { "webm" });
    }
    // MPEG-TS: 188-byte packets, each starting with 0x47.
    if head.len() >= 377 && head[0] == 0x47 && head[188] == 0x47 && head[376] == 0x47 {
        return Some("ts");
    }
    if at(0, b"FLV") {
        return Some("flv");
    }
    if at(0, b"OggS") {
        return Some("ogg");
    }
    if at(0, b"RIFF") && at(8, b"WAVE") {
        return Some("wav");
    }
    if at(0, b"ID3") {
        return Some("mp3");
    }
    match head {
        [0xff, b, ..] if b & 0xf6 == 0xf0 => Some("aac"),
        [0xff, b, ..] if b & 0xe0 == 0xe0 => Some("mp3"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webvtt_segments_join_into_one_file() {
        let parts = [
            "WEBVTT\nX-TIMESTAMP-MAP=MPEGTS:900000,LOCAL:00:00:00.000\n\n00:00:01.000 --> 00:00:02.000\nHello\n".to_string(),
            "\u{feff}WEBVTT\r\nX-TIMESTAMP-MAP=MPEGTS:900000,LOCAL:00:00:00.000\r\n\r\n00:00:07.000 --> 00:00:08.000\r\nWorld\r\n".to_string(),
            "WEBVTT\n\n".to_string(),
        ];
        assert_eq!(
            join_webvtt(&parts),
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHello\n\n00:00:07.000 --> 00:00:08.000\nWorld\n"
        );
    }

    #[test]
    fn language_tags_shorten_to_their_main_part() {
        assert_eq!(language_code("en-US").as_deref(), Some("en"));
        assert_eq!(language_code("DEU").as_deref(), Some("deu"));
        assert_eq!(language_code("English"), None);
        assert_eq!(language_code(""), None);
        assert_eq!(iso639_2("en-US").as_deref(), Some("eng"));
        assert_eq!(iso639_2("de").as_deref(), Some("deu"));
        assert_eq!(iso639_2("fre").as_deref(), Some("fre"));
        assert_eq!(iso639_2("xx"), None);
    }

    #[test]
    fn labels_and_sizes() {
        assert_eq!(quality_label(Some(1080), Some(5_200_000)), "1080p, 5.2 Mbps");
        assert_eq!(quality_label(None, Some(128_000)), "128 kbps");
        assert_eq!(quality_label(None, None), "Default");
        assert_eq!(estimate_size(Some(8_000_000), Some(10.0)), Some(10_000_000));
        assert_eq!(estimate_size(None, Some(10.0)), None);
    }

    #[test]
    fn resolving() {
        assert_eq!(resolve("https://x/a/b.m3u8", "c/1.ts").unwrap(), "https://x/a/c/1.ts");
        assert_eq!(resolve("https://x/a/b.m3u8", "/1.ts").unwrap(), "https://x/1.ts");
        assert_eq!(resolve("https://x/a/b.m3u8", "http://y/1.ts").unwrap(), "http://y/1.ts");
        assert!(resolve("https://x/a/b.m3u8", "file:///c:/x").is_err());
        assert!(resolve("https://x/a/b.m3u8", "data:text/plain,x").is_err());
    }

    #[test]
    fn sniffing() {
        assert_eq!(sniff("\u{feff}#EXTM3U\n"), Some(MediaKind::Hls));
        assert_eq!(
            sniff("<?xml version=\"1.0\"?>\n<MPD xmlns=\"urn:mpeg:dash:schema:mpd:2011\">"),
            Some(MediaKind::Dash)
        );
        assert_eq!(
            sniff("<?xml version=\"1.0\"?>\n<manifest xmlns=\"http://ns.adobe.com/f4m/1.0\">"),
            Some(MediaKind::Hds)
        );
        assert_eq!(sniff("<html>"), None);
    }

    #[test]
    fn sniffing_files() {
        assert_eq!(sniff_file(b"\0\0\0\x20ftypisom\0\0\x02\0"), Some("mp4"));
        assert_eq!(sniff_file(b"\0\0\0\x20ftypM4A \0\0\0\0"), Some("m4a"));
        let mut ts = vec![0u8; 400];
        ts[0] = 0x47;
        ts[188] = 0x47;
        ts[376] = 0x47;
        assert_eq!(sniff_file(&ts), Some("ts"));
        assert_eq!(sniff_file(&[0x47; 100]), None);
        assert_eq!(sniff_file(b"\x1a\x45\xdf\xa3\x01\x42\x82\x84webm"), Some("webm"));
        assert_eq!(sniff_file(b"\x1a\x45\xdf\xa3\x01\x42\x82\x88matroska"), Some("mkv"));
        assert_eq!(sniff_file(b"ID3\x04"), Some("mp3"));
        assert_eq!(sniff_file(&[0xff, 0xf1, 0x50]), Some("aac"));
        assert_eq!(sniff_file(b"<html><body>"), None);
        assert_eq!(sniff_file(b"#EXTM3U"), None);
    }
}
