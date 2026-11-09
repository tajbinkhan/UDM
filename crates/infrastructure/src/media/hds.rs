//! Adobe HTTP Dynamic Streaming (HDS): F4M manifests and F4F fragments.
//!
//! An F4M manifest (`<manifest xmlns="http://ns.adobe.com/f4m/1.0">`) lists
//! one `<media>` per bit rate. Each names the start of its fragments' names
//! (`url`) and a bootstrap: an `abst` box, inline in base64 or at an address,
//! whose segment run table (`asrt`) and fragment run table (`afrt`) say
//! which fragments there are. Fragment `n` of segment `s` is at
//! `<baseURL>/<media url>Seg<s>-Frag<n>`. F4M 2.0 may instead point each
//! `<media href=...>` at another manifest that has the fragments.
//!
//! Each fragment is an F4F file: MP4-style boxes whose `mdat` holds FLV tags.
//! The video is an FLV file made of an FLV header, the media's `<metadata>`
//! as its `onMetaData` tag, and every fragment's tags in order. ffmpeg, when
//! it is there, remuxes that into an MP4 as it does an HLS `.ts`.
//!
//! Akamai, which serves most HDS, signs the manifest's address
//! (`hdnea=...`, `pvtoken=...`) and wants the same signature on every
//! fragment, so the manifest's query string goes on each fragment's address,
//! and so does a `<pv-2.0>` token. DRM (`<drmAdditionalHeader>`) and live
//! streams are refused.

use std::io::{self, Read, Seek, SeekFrom, Write};

use base64::Engine as _;
use roxmltree::{Document, Node};

use uidm_domain::media::{
    resolve, AddressError, MediaInfo, MediaKind, MediaOption, Segment, Track, DRM_REFUSED, LIVE_REFUSED,
};

/// More fragments than this in one stream means a broken bootstrap.
const MAX_FRAGMENTS: usize = 500_000;

/// Why a manifest or its bootstrap information cannot be downloaded. The
/// message is shown to the user as the download's failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HdsError {
    /// What the XML parser said.
    #[error("The manifest is not valid XML: {0}")]
    NotXml(String),
    #[error("This is not an HDS manifest.")]
    NotManifest,
    #[error("{}", LIVE_REFUSED)]
    Live,
    #[error("{}", DRM_REFUSED)]
    Drm,
    /// What the decoder said.
    #[error("The manifest has bad base64 in it: {0}")]
    BadBase64(String),
    #[error("A quality in the manifest has no address.")]
    NoAddress,
    #[error("A quality in the manifest names no bootstrap information.")]
    NoBootstrap,
    #[error("The manifest has no video.")]
    NoVideo,
    #[error("A quality's manifest lists more manifests.")]
    Nested,
    #[error("The stream's bootstrap information is cut short.")]
    Truncated,
    #[error("The stream's bootstrap information has a bad box.")]
    BadBox,
    #[error("The stream's bootstrap information is not an abst box.")]
    NotAbst,
    #[error("The stream's bootstrap information lists no segments.")]
    NoSegments,
    #[error("The stream lists too many segments.")]
    TooManySegments,
    #[error("The stream lists too many fragments.")]
    TooManyFragments,
    #[error(transparent)]
    Address(#[from] AddressError),
    #[error(transparent)]
    Url(#[from] url::ParseError),
}

/// Where a media's bootstrap info is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bootstrap {
    /// In the manifest, decoded from base64.
    Inline(Vec<u8>),
    /// At this address.
    Url(String),
}

/// What a `<media>` leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Its fragments.
    Fragments {
        /// The address the fragments' names (`Seg1-Frag1`) go after,
        /// without a query.
        url: String,
        /// The query every fragment's address carries, without the `?`.
        query: Option<String>,
        bootstrap: Bootstrap,
        /// The `onMetaData` tag's body (AMF), when the manifest has one.
        metadata: Option<Vec<u8>>,
    },
    /// Another manifest that has them (F4M 2.0).
    Manifest(String),
}

/// One bit rate a manifest offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Media {
    /// What to put in `MediaChoice::video` to get this one.
    pub id: String,
    /// Bits per second.
    pub bitrate: Option<u64>,
    pub height: Option<u32>,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Manifest {
    /// Best first.
    pub media: Vec<Media>,
    /// Length in seconds.
    pub duration: Option<f64>,
}

fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|n| n.is_element() && n.tag_name().name() == name)
}

fn children<'a, 'i: 'a>(node: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    node.children().filter(move |n| n.is_element() && n.tag_name().name() == name)
}

fn text_in<'a>(node: Option<Node<'a, '_>>) -> Option<&'a str> {
    node.and_then(|n| n.text()).map(str::trim).filter(|t| !t.is_empty())
}

fn base64(text: &str) -> Result<Vec<u8>, HdsError> {
    let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD.decode(clean).map_err(|e| HdsError::BadBase64(e.to_string()))
}

/// `url` with `query` added to its own query string.
fn with_query(url: &str, query: Option<&str>) -> String {
    match query {
        Some(q) if !url.contains(q) => {
            let sep = if url.contains('?') { '&' } else { '?' };
            format!("{url}{sep}{q}")
        }
        _ => url.to_string(),
    }
}

/// Read a manifest fetched from `url`.
pub fn parse(text: &str, url: &str) -> Result<Manifest, HdsError> {
    let doc = Document::parse(text.trim_start_matches('\u{feff}')).map_err(|e| HdsError::NotXml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "manifest" {
        return Err(HdsError::NotManifest);
    }
    if text_of(root, "streamType") == Some("live") {
        return Err(HdsError::Live);
    }
    // The manifest's own query is Akamai's signature, which every address
    // in the stream needs.
    let manifest_query =
        url::Url::parse(url).ok().and_then(|u| u.query().map(str::to_string)).filter(|q| !q.is_empty());
    let pv = text_in(child(root, "pv-2.0")).map(|t| t.trim_matches(';').to_string());
    let base = match text_in(child(root, "baseURL")) {
        Some(b) if b.ends_with('/') => resolve(url, b)?,
        Some(b) => resolve(url, &format!("{b}/"))?,
        None => url.to_string(),
    };
    let drm_everywhere = root.descendants().any(|n| n.is_element() && n.tag_name().name() == "drmAdditionalHeader");

    let mut bootstraps = Vec::new();
    for b in children(root, "bootstrapInfo") {
        let id = b.attribute("id").unwrap_or("");
        let source = match b.attribute("url") {
            Some(u) => Bootstrap::Url(with_query(&resolve(&base, u)?, manifest_query.as_deref())),
            None => Bootstrap::Inline(base64(b.text().unwrap_or(""))?),
        };
        bootstraps.push((id, source));
    }

    let mut manifest = Manifest {
        duration: text_of(root, "duration").and_then(|d| d.parse().ok()).filter(|d: &f64| *d > 0.0),
        ..Manifest::default()
    };
    let mut drm = false;
    for (i, m) in children(root, "media").enumerate() {
        if drm_everywhere || m.attribute("drmAdditionalHeaderId").is_some() {
            drm = true;
            continue;
        }
        let bitrate = m.attribute("bitrate").and_then(|b| b.trim().parse::<f64>().ok()).map(|k| (k * 1000.0) as u64);
        let height = m.attribute("height").and_then(|h| h.trim().parse().ok());
        let (name, source) = if let Some(href) = m.attribute("href") {
            (href, Source::Manifest(with_query(&resolve(&base, href)?, manifest_query.as_deref())))
        } else {
            let name = m.attribute("url").ok_or(HdsError::NoAddress)?;
            let resolved = url::Url::parse(&resolve(&base, name)?)?;
            let query: Vec<String> = [resolved.query().map(str::to_string), manifest_query.clone(), pv.clone()]
                .into_iter()
                .flatten()
                .filter(|q| !q.is_empty())
                .fold(Vec::new(), |mut all, q| {
                    if !all.contains(&q) {
                        all.push(q);
                    }
                    all
                });
            let mut bare = resolved.clone();
            bare.set_query(None);
            bare.set_fragment(None);
            let bootstrap = match m.attribute("bootstrapInfoId") {
                Some(id) => bootstraps.iter().find(|(b, _)| *b == id),
                None => bootstraps.first(),
            }
            .map(|(_, b)| b.clone())
            .ok_or(HdsError::NoBootstrap)?;
            let metadata = text_in(child(m, "metadata")).map(base64).transpose()?;
            let source = Source::Fragments {
                url: bare.to_string(),
                query: (!query.is_empty()).then(|| query.join("&")),
                bootstrap,
                metadata,
            };
            (name, source)
        };
        let id =
            if manifest.media.iter().any(|x: &Media| x.id == name) { format!("{name}#{i}") } else { name.to_string() };
        manifest.media.push(Media { id, bitrate, height, source });
    }
    if manifest.media.is_empty() {
        return Err(if drm { HdsError::Drm } else { HdsError::NoVideo });
    }
    manifest.media.sort_by_key(|m| std::cmp::Reverse((m.height.unwrap_or(0), m.bitrate.unwrap_or(0))));
    Ok(manifest)
}

fn text_of<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    text_in(child(node, name))
}

/// The qualities a manifest offers.
pub fn describe(m: &Manifest) -> MediaInfo {
    let video = m
        .media
        .iter()
        .map(|media| MediaOption {
            id: media.id.clone(),
            label: uidm_domain::media::quality_label(media.height, media.bitrate),
            height: media.height,
            bandwidth: media.bitrate,
            size: uidm_domain::media::estimate_size(media.bitrate, m.duration),
            ..MediaOption::default()
        })
        .collect();
    MediaInfo { kind: Some(MediaKind::Hds), video, duration: m.duration, ..MediaInfo::default() }
}

/// The media `id` names, or the best.
pub fn pick<'a>(m: &'a Manifest, id: Option<&str>) -> &'a Media {
    id.and_then(|id| m.media.iter().find(|x| x.id == id)).unwrap_or(&m.media[0])
}

/// One run of the fragment run table: fragments numbered from `first`,
/// starting at `timestamp`, each `duration` long. A `duration` of 0 marks a
/// discontinuity (or the end), with its kind in `discontinuity`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentRun {
    pub first: u32,
    pub timestamp: u64,
    pub duration: u32,
    pub discontinuity: Option<u8>,
}

/// What an `abst` box says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BootstrapInfo {
    pub live: bool,
    pub timescale: u32,
    pub current_media_time: u64,
    /// The segment run table: (first segment, fragments per segment).
    pub segments: Vec<(u32, u32)>,
    /// The fragment run table, and its own timescale.
    pub fragments: Vec<FragmentRun>,
    pub fragment_timescale: u32,
}

/// Reads big-endian numbers and strings off a box.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], HdsError> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.data.len()).ok_or(HdsError::Truncated)?;
        let out = &self.data[self.at..end];
        self.at = end;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, HdsError> {
        Ok(self.bytes(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, HdsError> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().map_err(|_| HdsError::Truncated)?))
    }
    fn u64(&mut self) -> Result<u64, HdsError> {
        Ok(u64::from_be_bytes(self.bytes(8)?.try_into().map_err(|_| HdsError::Truncated)?))
    }
    /// A NUL-terminated string.
    fn string(&mut self) -> Result<&'a [u8], HdsError> {
        let len = self.data[self.at..].iter().position(|b| *b == 0).ok_or(HdsError::Truncated)?;
        let s = self.bytes(len)?;
        self.at += 1;
        Ok(s)
    }
    /// A box: its type and its body.
    fn boxed(&mut self) -> Result<([u8; 4], &'a [u8]), HdsError> {
        let size = self.u32()? as u64;
        let kind: [u8; 4] = self.bytes(4)?.try_into().map_err(|_| HdsError::Truncated)?;
        let (size, header) = match size {
            1 => (self.u64()?, 16),
            0 => ((self.data.len() - self.at + 8) as u64, 8),
            s => (s, 8),
        };
        let body = size.checked_sub(header).ok_or(HdsError::BadBox)?;
        Ok((kind, self.bytes(usize::try_from(body).map_err(|_| HdsError::Truncated)?)?))
    }
}

/// Read a bootstrap info (`abst`) box.
pub fn parse_bootstrap(data: &[u8]) -> Result<BootstrapInfo, HdsError> {
    let (kind, body) = Reader { data, at: 0 }.boxed()?;
    if &kind != b"abst" {
        return Err(HdsError::NotAbst);
    }
    let mut r = Reader { data: body, at: 0 };
    r.bytes(4)?; // version and flags
    r.u32()?; // bootstrap info version
    let flags = r.u8()?;
    let mut info = BootstrapInfo {
        live: flags & 0x20 != 0,
        timescale: r.u32()?,
        current_media_time: r.u64()?,
        ..BootstrapInfo::default()
    };
    r.u64()?; // SMPTE time code offset
    r.string()?; // movie identifier
    for _ in 0..r.u8()? {
        r.string()?; // server entries
    }
    for _ in 0..r.u8()? {
        r.string()?; // quality entries
    }
    r.string()?; // DRM data
    r.string()?; // metadata
                 // Only the first segment and fragment run tables are read; any others
                 // are other qualities'.
    for _ in 0..r.u8()? {
        let (kind, body) = r.boxed()?;
        if &kind != b"asrt" || !info.segments.is_empty() {
            continue;
        }
        let mut s = Reader { data: body, at: 0 };
        s.bytes(4)?; // version and flags
        for _ in 0..s.u8()? {
            s.string()?; // quality segment URL modifiers
        }
        for _ in 0..s.u32()? {
            let entry = (s.u32()?, s.u32()?);
            if info.segments.len() >= MAX_FRAGMENTS {
                return Err(HdsError::TooManySegments);
            }
            info.segments.push(entry);
        }
    }
    for _ in 0..r.u8()? {
        let (kind, body) = r.boxed()?;
        if &kind != b"afrt" || !info.fragments.is_empty() {
            continue;
        }
        let mut f = Reader { data: body, at: 0 };
        f.bytes(4)?; // version and flags
        info.fragment_timescale = f.u32()?;
        for _ in 0..f.u8()? {
            f.string()?; // quality segment URL modifiers
        }
        for _ in 0..f.u32()? {
            let first = f.u32()?;
            let timestamp = f.u64()?;
            let duration = f.u32()?;
            let discontinuity = if duration == 0 { Some(f.u8()?) } else { None };
            if info.fragments.len() >= MAX_FRAGMENTS {
                return Err(HdsError::TooManyFragments);
            }
            info.fragments.push(FragmentRun { first, timestamp, duration, discontinuity });
        }
    }
    Ok(info)
}

/// Every fragment, as (segment, fragment) numbers in order. Segment run
/// `i` covers the segments from its first up to the next run's first, each
/// with its count of fragments; the last run covers one segment. Fragments
/// are numbered on from the fragment run table's first.
pub fn fragments(info: &BootstrapInfo) -> Result<Vec<(u32, u32)>, HdsError> {
    if info.segments.is_empty() {
        return Err(HdsError::NoSegments);
    }
    let mut next = info.fragments.iter().find(|r| r.duration > 0).map_or(1, |r| r.first);
    let mut out = Vec::new();
    for (i, &(first, per)) in info.segments.iter().enumerate() {
        let last = info.segments.get(i + 1).map_or(first, |(n, _)| n.saturating_sub(1).max(first));
        for segment in first..=last {
            if out.len() + per as usize > MAX_FRAGMENTS {
                return Err(HdsError::TooManyFragments);
            }
            for _ in 0..per {
                out.push((segment, next));
                next = next.checked_add(1).ok_or(HdsError::TooManyFragments)?;
            }
        }
    }
    Ok(out)
}

/// The address of fragment `fragment` of segment `segment`.
pub fn fragment_url(url: &str, query: Option<&str>, segment: u32, fragment: u32) -> String {
    let mut out = format!("{url}Seg{segment}-Frag{fragment}");
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    out
}

/// The FLV file's start: its header, the first (empty) previous-tag size,
/// and `metadata` as an `onMetaData` script tag.
pub fn flv_header(metadata: Option<&[u8]>) -> Vec<u8> {
    // "FLV", version 1, audio and video, a 9-byte header; PreviousTagSize0.
    let mut out = b"FLV\x01\x05\x00\x00\x00\x09\x00\x00\x00\x00".to_vec();
    // A tag's size has 24 bits; metadata bigger than that is broken.
    if let Some(m) = metadata.filter(|m| !m.is_empty() && m.len() < 1 << 24) {
        out.push(0x12); // script data
        out.extend_from_slice(&(m.len() as u32).to_be_bytes()[1..]);
        out.extend_from_slice(&[0; 4]); // timestamp and its extension
        out.extend_from_slice(&[0; 3]); // stream id
        out.extend_from_slice(m);
        out.extend_from_slice(&(m.len() as u32 + 11).to_be_bytes());
    }
    out
}

/// Copy the payload of every `mdat` box in the F4F fragment `fragment` (its
/// FLV tags) to `out`, skipping the other boxes. Returns the bytes copied.
pub fn copy_mdat<R: Read + Seek, W: Write>(fragment: &mut R, out: &mut W) -> io::Result<u64> {
    let bad = |why: &str| io::Error::new(io::ErrorKind::InvalidData, format!("a fragment of the stream {why}"));
    let mut copied = 0;
    let mut found = false;
    loop {
        let mut head = [0u8; 8];
        match fragment.read(&mut head[..1])? {
            0 => break,
            _ => fragment.read_exact(&mut head[1..])?,
        }
        let size = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as u64;
        let (body, to_end) = match size {
            0 => (0, true),
            1 => {
                let mut large = [0u8; 8];
                fragment.read_exact(&mut large)?;
                (u64::from_be_bytes(large).checked_sub(16).ok_or_else(|| bad("has a bad box"))?, false)
            }
            s => (s.checked_sub(8).ok_or_else(|| bad("has a bad box"))?, false),
        };
        if &head[4..] == b"mdat" {
            found = true;
            copied += if to_end {
                io::copy(fragment, out)?
            } else {
                let n = io::copy(&mut fragment.by_ref().take(body), out)?;
                if n < body {
                    return Err(bad("is cut short"));
                }
                n
            };
        } else if to_end {
            break;
        } else {
            fragment.seek(SeekFrom::Current(i64::try_from(body).map_err(|_| bad("has a bad box"))?))?;
        }
        if to_end {
            break;
        }
    }
    if !found {
        return Err(bad("has no media data (mdat)"));
    }
    Ok(copied)
}

/// The track of `media` (a `Source::Fragments`), from its bootstrap info
/// `bootstrap`. `duration` is the manifest's, when it says.
pub fn track(media: &Media, bootstrap: &[u8], duration: Option<f64>) -> Result<Track, HdsError> {
    let Source::Fragments { url, query, metadata, .. } = &media.source else {
        return Err(HdsError::Nested);
    };
    let info = parse_bootstrap(bootstrap)?;
    if info.live {
        return Err(HdsError::Live);
    }
    let segments =
        fragments(&info)?.into_iter().map(|(s, f)| Segment::whole(fragment_url(url, query.as_deref(), s, f))).collect();
    let from_bootstrap = (info.timescale > 0 && info.current_media_time > 0)
        .then(|| info.current_media_time as f64 / info.timescale as f64);
    Ok(Track {
        init: None,
        segments,
        ext: "flv",
        duration: duration.or(from_bootstrap),
        hds_header: Some(flv_header(metadata.as_deref())),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use uidm_domain::bytes::ByteCount;
    use uidm_domain::media::sniff_hds as sniff;

    const URL: &str = "https://cdn.x/z/movie/manifest.f4m?hdnea=st=1~exp=2~hmac=abc";

    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    /// An `abst` box by hand: `segments` (first, fragments per segment), one
    /// fragment run from `first_fragment` of 4-second fragments, and the end
    /// marker.
    pub(crate) fn abst(live: bool, segments: &[(u32, u32)], first_fragment: u32) -> Vec<u8> {
        let mut asrt = vec![0, 0, 0, 0, 0]; // version, flags, no quality entries
        asrt.extend_from_slice(&(segments.len() as u32).to_be_bytes());
        for (s, f) in segments {
            asrt.extend_from_slice(&s.to_be_bytes());
            asrt.extend_from_slice(&f.to_be_bytes());
        }
        let mut afrt = vec![0, 0, 0, 0];
        afrt.extend_from_slice(&1000u32.to_be_bytes());
        afrt.push(0);
        afrt.extend_from_slice(&2u32.to_be_bytes());
        afrt.extend_from_slice(&first_fragment.to_be_bytes());
        afrt.extend_from_slice(&0u64.to_be_bytes());
        afrt.extend_from_slice(&4000u32.to_be_bytes());
        afrt.extend_from_slice(&[0, 0, 0, 0]); // end of presentation
        afrt.extend_from_slice(&0u64.to_be_bytes());
        afrt.extend_from_slice(&[0, 0, 0, 0, 0]);

        let mut body = vec![0, 0, 0, 0]; // version, flags
        body.extend_from_slice(&1u32.to_be_bytes());
        body.push(if live { 0x20 } else { 0 });
        body.extend_from_slice(&1000u32.to_be_bytes());
        body.extend_from_slice(&8000u64.to_be_bytes());
        body.extend_from_slice(&0u64.to_be_bytes());
        body.extend_from_slice(b"movie\0");
        body.push(1);
        body.extend_from_slice(b"server\0");
        body.push(0);
        body.extend_from_slice(b"\0\0"); // DRM data, metadata
        body.push(1);
        body.extend_from_slice(&boxed(b"asrt", &asrt));
        body.push(1);
        body.extend_from_slice(&boxed(b"afrt", &afrt));
        boxed(b"abst", &body)
    }

    /// An F4F fragment: an `afra` and a `moof` box around `tags` in `mdat`.
    pub(crate) fn fragment(tags: &[u8]) -> Vec<u8> {
        let mut out = boxed(b"afra", &[1, 2, 3, 4, 5]);
        out.extend(boxed(b"moof", b"not media"));
        out.extend(boxed(b"mdat", tags));
        out
    }

    fn b64(data: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(data)
    }

    #[test]
    fn reads_bootstrap_boxes() {
        let info = parse_bootstrap(&abst(false, &[(1, 3)], 1)).unwrap();
        assert!(!info.live);
        assert_eq!((info.timescale, info.current_media_time), (1000, 8000));
        assert_eq!(info.segments, [(1, 3)]);
        assert_eq!(info.fragment_timescale, 1000);
        assert_eq!(info.fragments[0], FragmentRun { first: 1, timestamp: 0, duration: 4000, discontinuity: None });
        assert_eq!(info.fragments[1].discontinuity, Some(0));
        assert_eq!(fragments(&info).unwrap(), [(1, 1), (1, 2), (1, 3)]);

        // Segments 1 and 2 with two fragments each, then segment 3 with one;
        // numbering starts where the fragment table does.
        let info = parse_bootstrap(&abst(false, &[(1, 2), (3, 1)], 5)).unwrap();
        assert_eq!(fragments(&info).unwrap(), [(1, 5), (1, 6), (2, 7), (2, 8), (3, 9)]);

        assert!(parse_bootstrap(&abst(true, &[(1, 1)], 1)).unwrap().live);
        assert!(parse_bootstrap(&boxed(b"moov", b"")).is_err());
        let whole = abst(false, &[(1, 3)], 1);
        assert!(parse_bootstrap(&whole[..whole.len() - 10]).is_err(), "cut short");
        let too_many = parse_bootstrap(&abst(false, &[(1, u32::MAX)], 1)).unwrap();
        assert!(fragments(&too_many).is_err());
    }

    #[test]
    fn fragment_addresses() {
        assert_eq!(fragment_url("https://x/z/video_1000", None, 1, 7), "https://x/z/video_1000Seg1-Frag7");
        assert_eq!(
            fragment_url("https://x/z/v", Some("hdnea=a&pvtoken=b"), 2, 3),
            "https://x/z/vSeg2-Frag3?hdnea=a&pvtoken=b"
        );
    }

    fn v1() -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest xmlns="http://ns.adobe.com/f4m/1.0">
  <id>movie</id>
  <streamType>recorded</streamType>
  <duration>8.0</duration>
  <baseURL>https://media.x/hds</baseURL>
  <pv-2.0>pvtoken=xyz;</pv-2.0>
  <bootstrapInfo profile="named" id="boot1">{boot}</bootstrapInfo>
  <bootstrapInfo profile="named" id="boot2" url="b2.bootstrap"/>
  <media url="video_500" bitrate="500" width="640" height="360" bootstrapInfoId="boot1">
    <metadata>{meta}</metadata>
  </media>
  <media url="video_1500?x=1" bitrate="1500" width="1280" height="720" bootstrapInfoId="boot2"/>
</manifest>"#,
            boot = b64(&abst(false, &[(1, 2)], 1)),
            meta = b64(b"\x02\x00\x0aonMetaData")
        )
    }

    #[test]
    fn version_1_manifests_with_several_bit_rates() {
        let text = v1();
        assert!(sniff(&text));
        let m = parse(&text, URL).unwrap();
        assert_eq!(m.duration, Some(8.0));
        let ids: Vec<&str> = m.media.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["video_1500?x=1", "video_500"], "best first");
        assert_eq!(m.media[0].bitrate, Some(1_500_000));
        assert_eq!(
            m.media[0].source,
            Source::Fragments {
                url: "https://media.x/hds/video_1500".into(),
                query: Some("x=1&hdnea=st=1~exp=2~hmac=abc&pvtoken=xyz".into()),
                bootstrap: Bootstrap::Url("https://media.x/hds/b2.bootstrap?hdnea=st=1~exp=2~hmac=abc".into()),
                metadata: None,
            }
        );
        let Source::Fragments { bootstrap: Bootstrap::Inline(boot), metadata, .. } = &m.media[1].source else {
            panic!("inline bootstrap");
        };
        assert_eq!(metadata.as_deref(), Some(&b"\x02\x00\x0aonMetaData"[..]));

        let info = describe(&m);
        assert_eq!(info.kind, Some(MediaKind::Hds));
        let labels: Vec<&str> = info.video.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(labels, ["720p, 1.5 Mbps", "360p, 500 kbps"]);
        assert_eq!(info.video[1].size, Some(ByteCount::new(500_000)));

        let low = pick(&m, Some("video_500"));
        let t = track(low, boot, m.duration).unwrap();
        let urls: Vec<&str> = t.segments.iter().map(|s| s.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://media.x/hds/video_500Seg1-Frag1?hdnea=st=1~exp=2~hmac=abc&pvtoken=xyz",
                "https://media.x/hds/video_500Seg1-Frag2?hdnea=st=1~exp=2~hmac=abc&pvtoken=xyz"
            ]
        );
        assert_eq!((t.ext, t.duration), ("flv", Some(8.0)));
        assert!(t.hds_header.unwrap().ends_with(&(10u32 + 3 + 11).to_be_bytes()));
        assert_eq!(pick(&m, Some("gone")).id, "video_1500?x=1");
    }

    #[test]
    fn version_2_manifests_point_at_others() {
        let text = r#"<manifest xmlns="http://ns.adobe.com/f4m/2.0">
          <baseURL>https://set.x/s/</baseURL>
          <media href="low.f4m" bitrate="400"/>
          <media href="https://other.x/high.f4m?k=1" bitrate="2000"/>
        </manifest>"#;
        assert!(sniff(text));
        let m = parse(text, "https://set.x/set.f4m?hdnea=t").unwrap();
        let sources: Vec<&Source> = m.media.iter().map(|x| &x.source).collect();
        assert_eq!(
            sources,
            [
                &Source::Manifest("https://other.x/high.f4m?k=1&hdnea=t".into()),
                &Source::Manifest("https://set.x/s/low.f4m?hdnea=t".into())
            ]
        );
        assert!(track(&m.media[0], &abst(false, &[(1, 1)], 1), None).is_err());
    }

    #[test]
    fn drm_and_live_are_refused() {
        let drm = r#"<manifest xmlns="http://ns.adobe.com/f4m/1.0">
          <drmAdditionalHeader id="d">AAAA</drmAdditionalHeader>
          <bootstrapInfo id="b">AAAA</bootstrapInfo>
          <media url="v" bootstrapInfoId="b" drmAdditionalHeaderId="d"/></manifest>"#;
        assert_eq!(parse(drm, URL).unwrap_err(), HdsError::Drm);
        let live = r#"<manifest xmlns="http://ns.adobe.com/f4m/1.0"><streamType>live</streamType></manifest>"#;
        assert_eq!(parse(live, URL).unwrap_err(), HdsError::Live);
        let m = parse(&v1(), URL).unwrap();
        assert_eq!(track(&m.media[1], &abst(true, &[(1, 1)], 1), None).unwrap_err(), HdsError::Live);
        assert!(parse("<MPD/>", URL).is_err());
        assert!(!sniff("<MPD xmlns=\"urn:mpeg:dash:schema:mpd:2011\">"));
        assert!(!sniff("<manifest><media/></manifest>"), "not Adobe's namespace");
    }

    #[test]
    fn takes_the_flv_tags_out_of_fragments() {
        let mut out = Vec::new();
        let n = copy_mdat(&mut io::Cursor::new(fragment(b"TAGS")), &mut out).unwrap();
        assert_eq!((n, out.as_slice()), (4, &b"TAGS"[..]));

        // A large-size mdat, and one that runs to the end (size 0).
        let mut large = 1u32.to_be_bytes().to_vec();
        large.extend_from_slice(b"mdat");
        large.extend_from_slice(&(16u64 + 3).to_be_bytes());
        large.extend_from_slice(b"abc");
        large.extend_from_slice(&0u32.to_be_bytes());
        large.extend_from_slice(b"mdat");
        large.extend_from_slice(b"rest");
        let mut out = Vec::new();
        copy_mdat(&mut io::Cursor::new(large), &mut out).unwrap();
        assert_eq!(out, b"abcrest");

        assert!(copy_mdat(&mut io::Cursor::new(boxed(b"moof", b"x")), &mut Vec::new()).is_err(), "no mdat");
        let mut short = fragment(b"TAGS");
        short.truncate(short.len() - 2);
        assert!(copy_mdat(&mut io::Cursor::new(short), &mut Vec::new()).is_err(), "cut short");
    }

    #[test]
    fn writes_flv_headers() {
        let bare = flv_header(None);
        assert_eq!(bare, b"FLV\x01\x05\x00\x00\x00\x09\x00\x00\x00\x00");
        assert_eq!(uidm_domain::media::sniff_file(&bare), Some("flv"));
        let meta = flv_header(Some(b"AMF!"));
        assert_eq!(&meta[13..24], &[0x12, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&meta[24..28], b"AMF!");
        assert_eq!(&meta[28..], &15u32.to_be_bytes());
    }
}
