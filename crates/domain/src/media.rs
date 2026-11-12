//! Streamed video as UDM describes it: the formats, the qualities and
//! tracks a stream offers, and which of them to download. Reading the
//! playlists and manifests is `udm_infrastructure::media`.

use std::path::{Path, PathBuf};

use crate::address::AddressError;
use crate::bytes::ByteCount;
use serde::{Deserialize, Serialize};

/// What a video becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Mp4,
    Mp3,
    M4a,
    /// Opus sound in an Ogg file, which is what most video sites send
    /// already, so it is usually kept as it is.
    Opus,
    /// Lossless FLAC. A site's sound is lossy already, so this only keeps
    /// what is there without losing more.
    Flac,
}

impl Target {
    /// The sound-only formats, as the menus list them.
    pub const SOUND: [Target; 4] = [Target::Mp3, Target::M4a, Target::Opus, Target::Flac];

    pub fn extension(self) -> &'static str {
        match self {
            Target::Mp4 => "mp4",
            Target::Mp3 => "mp3",
            Target::M4a => "m4a",
            Target::Opus => "opus",
            Target::Flac => "flac",
        }
    }

    /// The format named by its extension (`mp3`, `opus`), in any case.
    pub fn from_extension(ext: &str) -> Option<Target> {
        [Target::Mp4, Target::Mp3, Target::M4a, Target::Opus, Target::Flac]
            .into_iter()
            .find(|t| t.extension().eq_ignore_ascii_case(ext))
    }

    pub fn audio_only(self) -> bool {
        self != Target::Mp4
    }

    /// The name of `input` converted to this format: its stem with the new
    /// extension, or with " (converted)" when the extension stays the same.
    pub fn converted_name(self, input: &Path) -> String {
        let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "converted".into());
        let ext = self.extension();
        if input.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
            format!("{stem} (converted).{ext}")
        } else {
            format!("{stem}.{ext}")
        }
    }
}

/// A finished download to convert: its file becomes a new row of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversion {
    /// The download converted.
    pub from: crate::id::DownloadId,
    /// Its address, which the new row shows.
    pub url: crate::address::Link,
    pub input: PathBuf,
    /// A free name beside the input.
    pub output: PathBuf,
    pub target: Target,
    /// Remove the original, with its file, once the conversion succeeds.
    pub delete_original: bool,
}

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
    /// Keep only the sound, as this (`mp3`, `m4a`, `opus`, `flac`). Needs ffmpeg; only
    /// the audio track is fetched when the stream has one of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_only: Option<Target>,
    /// The subtitle tracks to keep: option `id`s from `MediaInfo`'s
    /// `subtitles`. `None` keeps the stream's default ones; an empty list
    /// keeps none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitles: Option<Vec<String>>,
    /// The tallest video to take when no `video` is chosen: the quality a
    /// playlist's videos are saved in, since each lists its own formats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<u32>,
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
    pub size: Option<ByteCount>,
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
    /// The page is a playlist or a channel: its videos, to pick from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playlist: Option<Playlist>,
}

/// The most videos of a playlist or channel listed.
pub const MAX_PLAYLIST_ENTRIES: usize = 1000;

/// A playlist or a channel's videos, as yt-dlp lists them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub entries: Vec<PlaylistEntry>,
}

/// One video of a playlist.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaylistEntry {
    /// Its page, which yt-dlp reads when it is downloaded.
    pub url: String,
    #[serde(default)]
    pub title: String,
    /// Length in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
}

/// How every picked video of a playlist is saved: the best quality, no
/// taller than a height, or the sound alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistQuality {
    Best,
    UpTo(u32),
    Sound(Target),
}

impl PlaylistQuality {
    /// The choices the playlist dialog offers, best first.
    pub const CHOICES: [PlaylistQuality; 9] = [
        PlaylistQuality::Best,
        PlaylistQuality::UpTo(1080),
        PlaylistQuality::UpTo(720),
        PlaylistQuality::UpTo(480),
        PlaylistQuality::UpTo(360),
        PlaylistQuality::Sound(Target::Mp3),
        PlaylistQuality::Sound(Target::M4a),
        PlaylistQuality::Sound(Target::Opus),
        PlaylistQuality::Sound(Target::Flac),
    ];

    /// What each video's download asks yt-dlp for.
    pub fn choice(self) -> MediaChoice {
        let page = MediaChoice { kind: Some(MediaKind::Page), ..MediaChoice::default() };
        match self {
            PlaylistQuality::Best => page,
            PlaylistQuality::UpTo(h) => MediaChoice { max_height: Some(h), ..page },
            PlaylistQuality::Sound(t) if t.audio_only() => MediaChoice { audio_only: Some(t), ..page },
            PlaylistQuality::Sound(_) => page,
        }
    }

    /// The extension the saved files get.
    pub fn extension(self) -> &'static str {
        match self {
            PlaylistQuality::Sound(t) => t.extension(),
            _ => "mp4",
        }
    }
}

impl Playlist {
    /// The entries yt-dlp listed that UDM can download: http and https
    /// pages, without repeats, at most `MAX_PLAYLIST_ENTRIES`.
    pub fn cleaned(self) -> Playlist {
        let mut seen = std::collections::HashSet::new();
        let entries = self
            .entries
            .into_iter()
            .filter(|e| crate::batch::is_web_url(&e.url))
            .filter(|e| seen.insert(e.url.clone()))
            .take(MAX_PLAYLIST_ENTRIES)
            .collect();
        Playlist { title: self.title, entries }
    }

    /// The downloads for the entries at `picked`, in the playlist's order,
    /// each named after its title (or numbered when it has none), saved as
    /// `quality` says and coming from `page`.
    pub fn downloads(&self, picked: &[usize], quality: PlaylistQuality, page: &str) -> Vec<crate::download::Download> {
        let mut picked: Vec<usize> = picked.iter().copied().filter(|&i| i < self.entries.len()).collect();
        picked.sort_unstable();
        picked.dedup();
        let ext = quality.extension();
        picked
            .into_iter()
            .filter_map(|i| {
                let e = &self.entries[i];
                let address = crate::address::Address::parse(&e.url).ok()?;
                let title = e.title.trim();
                let stem =
                    if title.is_empty() { format!("{} {}", self.title.trim(), i + 1) } else { title.to_string() };
                let name = crate::filename::sanitize_filename(&format!("{}.{ext}", stem.trim()))
                    .map(|n| n.as_str().to_string())
                    .unwrap_or_else(|| format!("video {}.{ext}", i + 1));
                Some(crate::download::Download {
                    url: address.into(),
                    filename: Some(name),
                    referrer: Some(page.to_string()).filter(|p| !p.is_empty()),
                    media: Some(quality.choice()),
                    ..crate::download::Download::default()
                })
            })
            .collect()
    }
}

/// A whole video or audio file found where a stream was expected.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaFile {
    /// Its type, as a file extension: `mp4`, `ts`, `webm`.
    pub ext: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<ByteCount>,
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
pub const DRM_REFUSED: &str = "This video is protected by DRM (copy protection), so UDM will not download it.";
pub const LIVE_REFUSED: &str = "This is a live stream. UDM downloads recorded videos, which have an end.";

/// `url` resolved against `base`, as a browser would.
pub fn resolve(base: &str, url: &str) -> Result<String, AddressError> {
    let base_url = url::Url::parse(base).map_err(|e| AddressError::Bad(base.to_string(), e.to_string()))?;
    let joined = base_url.join(url.trim()).map_err(|e| AddressError::Bad(url.to_string(), e.to_string()))?;
    match joined.scheme() {
        "http" | "https" => Ok(joined.to_string()),
        other => Err(AddressError::Scheme(other.to_string())),
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
pub fn estimate_size(bandwidth: Option<u64>, seconds: Option<f64>) -> Option<ByteCount> {
    let (b, s) = (bandwidth?, seconds?);
    (s > 0.0).then(|| ByteCount::new((b as f64 * s / 8.0) as u64))
}

/// Whether `text` looks like an HLS playlist, a DASH manifest or an HDS
/// manifest.
pub fn sniff(text: &str) -> Option<MediaKind> {
    let head = text.trim_start_matches('\u{feff}').trim_start();
    if head.starts_with("#EXTM3U") {
        Some(MediaKind::Hls)
    } else if head.starts_with('<') && head.as_bytes()[..head.len().min(4096)].windows(4).any(|w| w == b"<MPD") {
        Some(MediaKind::Dash)
    } else if sniff_hds(head) {
        Some(MediaKind::Hds)
    } else {
        None
    }
}

/// Whether `text` is an F4M manifest: XML whose root is `<manifest>` in
/// Adobe's F4M namespace.
pub fn sniff_hds(text: &str) -> bool {
    let head = text.trim_start_matches('\u{feff}').trim_start();
    let head = &head.as_bytes()[..head.len().min(4096)];
    head.starts_with(b"<")
        && head.windows(9).any(|w| w == b"<manifest")
        && head.windows(16).any(|w| w == b"ns.adobe.com/f4m")
}

/// The kind of video or audio file that starts with `head`, as a file
/// extension, or `None` when it is no media file UDM knows.
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

/// Whether a file name looks like a video ffmpeg can convert.
pub fn is_video(name: &str) -> bool {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    matches!(
        ext.as_str(),
        "mp4" | "mkv" | "webm" | "ts" | "avi" | "mov" | "flv" | "m4v" | "wmv" | "mpg" | "mpeg" | "3gp"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sizes in a probe's answer are plain numbers on the wire.
    #[test]
    fn sizes_keep_their_wire_shape() {
        let raw = r#"{"id":"137","label":"1080p","size":51600000}"#;
        let option: MediaOption = serde_json::from_str(raw).unwrap();
        assert_eq!(option.size, Some(ByteCount::new(51_600_000)));
        assert_eq!(serde_json::to_string(&option).unwrap(), raw);
        let raw = r#"{"ext":"mp4","size":300000}"#;
        let file: MediaFile = serde_json::from_str(raw).unwrap();
        assert_eq!(serde_json::to_string(&file).unwrap(), raw);
    }

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
        assert_eq!(estimate_size(Some(8_000_000), Some(10.0)), Some(ByteCount::new(10_000_000)));
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

    #[test]
    fn sound_formats_by_extension() {
        assert_eq!(Target::from_extension("OPUS"), Some(Target::Opus));
        assert_eq!(Target::from_extension("flac"), Some(Target::Flac));
        assert_eq!(Target::from_extension("wav"), None);
        assert!(Target::SOUND.iter().all(|t| t.audio_only()));
        assert_eq!(serde_json::to_string(&Target::Flac).unwrap(), r#""flac""#);
    }

    fn playlist() -> Playlist {
        let entry = |url: &str, title: &str| PlaylistEntry { url: url.into(), title: title.into(), duration: None };
        Playlist {
            title: "Mix".into(),
            entries: vec![
                entry("https://v.example/watch?v=1", "First: one/two"),
                entry("javascript:alert(1)", "Bad"),
                entry("https://v.example/watch?v=2", ""),
                entry("https://v.example/watch?v=1", "Again"),
                entry("ftp://v.example/3", "Ftp"),
            ],
        }
    }

    #[test]
    fn a_playlist_keeps_only_web_pages_once() {
        let clean = playlist().cleaned();
        let urls: Vec<&str> = clean.entries.iter().map(|e| e.url.as_str()).collect();
        assert_eq!(urls, ["https://v.example/watch?v=1", "https://v.example/watch?v=2"]);
        let many = Playlist {
            title: String::new(),
            entries: (0..MAX_PLAYLIST_ENTRIES + 5)
                .map(|i| PlaylistEntry { url: format!("https://v/{i}"), ..PlaylistEntry::default() })
                .collect(),
        };
        assert_eq!(many.cleaned().entries.len(), MAX_PLAYLIST_ENTRIES);
    }

    #[test]
    fn picked_videos_become_page_downloads() {
        let list = playlist().cleaned();
        let items = list.downloads(&[1, 0, 0, 9], PlaylistQuality::UpTo(720), "https://v.example/playlist?list=x");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].url.as_str(), "https://v.example/watch?v=1");
        assert_eq!(items[0].filename.as_deref(), Some("First_ one_two.mp4"));
        assert_eq!(items[1].filename.as_deref(), Some("Mix 2.mp4"));
        let media = items[0].media.as_ref().unwrap();
        assert_eq!((media.kind, media.max_height, media.audio_only), (Some(MediaKind::Page), Some(720), None));
        assert_eq!(items[0].referrer.as_deref(), Some("https://v.example/playlist?list=x"));
        let sound = list.downloads(&[0], PlaylistQuality::Sound(Target::Opus), "");
        assert_eq!(sound[0].filename.as_deref(), Some("First_ one_two.opus"));
        assert_eq!(sound[0].media.as_ref().unwrap().audio_only, Some(Target::Opus));
        assert_eq!(sound[0].referrer, None);
        assert_eq!(PlaylistQuality::Best.choice().max_height, None);
    }

    #[test]
    fn a_converted_file_keeps_its_stem() {
        assert_eq!(Target::Mp3.converted_name(Path::new("/dl/song.webm")), "song.mp3");
        assert_eq!(Target::Mp4.converted_name(Path::new("/dl/clip.MP4")), "clip (converted).mp4");
    }
}
