# Streams, batches and the site grabber

**Seeing videos.** The extension's service worker reads the headers of every
response a tab gets (`webRequest.onHeadersReceived`) and keeps, per tab, the
direct video and audio files (a `video/` or `audio/` type, or a media file
name with a generic type; anything under `video_min_size_kb`, 128 KiB by
default, is left out) and the HLS, DASH and HDS playlists, of the types in
`video_types` (`m3u8`, `mpd` and `f4m` for the streams; a file whose type neither
its name nor its MIME type says is kept). A settings file from before HDS
support gets `f4m` added once, recorded in `video_types_hds`. Segments (`.ts`, `.m4s`, `video/mp2t`,
HDS fragments' `video/f4f`) are left out, and
the list starts over when the tab loads a new page, or when a single-page
site moves to another (`webNavigation.onHistoryStateUpdated`; what came in
the second before stays, and the tab's buttons are reset). A content script
(`content.js`) shows **Download this video** over a playing `<video>` or
`<audio>`, or a media `<object>`/`<embed>` the pointer is over, in a closed
shadow root, so the page's scripts and styles cannot reach it, and shows
page and server text with `textContent` only. It tells the worker when each
player starts loading or playing (`video:started`, with a number per
player). When clicked it asks the worker for its tab's list for that player
(`video:menu` with the player's number and its `currentSrc`, `src` and
`<source>` addresses): the files and streams at those addresses, and the
playlists its frame asked for while it was the player that started last
(or that started within 1.5 s after), which tells players fed through
`blob:` addresses apart; with nothing of its own, the whole list. The worker
asks Pullbay (`probe_media`) what each stream offers. The button hides when, at
five points of the player, what is on top lies outside the player's box
(its furthest ancestor not much bigger than it, so a custom player's
controls and overlays do not count), when `document.hidden`, and when
something is full screen; it looks again on scroll, resize and once a
second. Tabs open before the extension was installed, updated or turned
back on get the content script injected (`scripting.executeScript`, every
frame that lacks it) on `runtime.onInstalled` and on the first connection
to Pullbay in a browser session. A content script can only pick from what the worker saw
in its tab, by number, never pass in an address. A video element that raised
an `encrypted` event or holds `MediaKeys` is DRM protected: the menu says so
and offers nothing, or with `video_protected_note` off the button does not
show on it. `video_view` (`full` or `mini`) sets how the button and the
selected-links button look; the mini one is a round arrow with its text as
a tooltip. Playlists that are themselves one quality of a listed
master playlist are folded into it.

**YouTube, Facebook, Instagram and Vimeo.** YouTube's player streams over
SABR: POST requests to `googlevideo.com/videoplayback` whose bodies hold
video and sound in YouTube's own framing (`application/vnd.yt-ump`), so no
response is a file, a playlist or a manifest; Facebook and Instagram fetch
signed pieces, and Vimeo reads its streams from its player's configuration.
For these sites the worker keeps the tab's video page instead
(`kind: "page"`, one address per video):

| Site | Pages | Kept as |
|---|---|---|
| YouTube | `/watch?v=ID`, `/shorts/ID`, `/live/ID`, `youtu.be/ID` | `https://www.youtube.com/watch?v=ID` |
| Facebook | `facebook.com/<page>/videos/ID`, `facebook.com/watch?v=ID`, `facebook.com/reel/ID`, `fb.watch/CODE` | `https://www.facebook.com/watch/?v=ID`, `.../reel/ID`, `https://fb.watch/CODE/` |
| Instagram | `instagram.com/p/CODE`, `/reel/CODE`, `/reels/CODE`, `/tv/CODE` | `https://www.instagram.com/p/CODE/` (or `reel/`, `tv/`) |
| Vimeo | `vimeo.com/ID` (with an unlisted video's hash), `vimeo.com/channels/<name>/ID`, `player.vimeo.com/video/ID` | `https://vimeo.com/ID`, the player's address with its `h=` hash |

The page is taken from the tab's address as these sites move between videos
without loading a page (`tabs.onUpdated` and
`webNavigation.onHistoryStateUpdated`). The daemon sends only these
addresses to yt-dlp (`ytdlp::supports`); any other goes to the stream
reader. `probe_media` on such an address runs
`yt-dlp -J` and offers one video per height (H.264 first, then the highest
bit rate) and the sound per language. The download runs yt-dlp with the
chosen formats and `--ffmpeg-location`, so ffmpeg joins them into an `.mp4`;
the sound alone is `-x --audio-format mp3|m4a`. Pullbay reads yt-dlp's progress
lines, and canceling ends yt-dlp and its children (`taskkill /T` on
Windows); its `.part` files let a retry carry on. Without ffmpeg the best
format with both video and sound in one file is saved, with a note. Live
streams are refused. Pullbay does not send the browser's cookies to yt-dlp, so
only videos anyone can watch signed out are offered (on Facebook and
Instagram that is public posts only).

**HLS.** Pullbay reads master playlists (qualities, separate audio tracks) and
media playlists (segments, `#EXT-X-BYTERANGE`, `#EXT-X-MAP` headers,
`#EXT-X-MEDIA-SEQUENCE`). Segments encrypted with AES-128 are decrypted with
the key from the playlist's key URL, fetched once, and the IV from the
playlist or the segment's sequence number. Any other key system
(`SAMPLE-AES`, or a `KEYFORMAT` other than `identity`, as FairPlay and
Widevine use) is DRM and refused, as is a live playlist without
`#EXT-X-ENDLIST`. Subtitle tracks (`#EXT-X-MEDIA:TYPE=SUBTITLES`) in the
quality's `SUBTITLES` group are offered by name and language.

**DASH.** Pullbay reads a static manifest's `SegmentTemplate` (with a fixed
duration or a `SegmentTimeline`, `$Number$`, `$Time$`, `$Bandwidth$`,
`$RepresentationID$` and `%0Nd` widths), `SegmentList` (with byte ranges)
and `SegmentBase` or a plain `BaseURL` (the whole file), inheriting them
from the period and adaptation set, and resolving `BaseURL`s at every level.
It picks the best video and audio, or the ones chosen. `ContentProtection`
marks DRM, refused; live (`dynamic`) manifests and manifests split into
several periods are refused too. WebVTT subtitle sets (`text/vtt`) are
offered by their `Label` or language; one with the `main` role is the
default. TTML and WebVTT inside MP4 segments are left out.

**HDS.** Adobe HTTP Dynamic Streaming (`.f4m`, `application/f4m+xml`, or XML
whose root is `<manifest>` in the `ns.adobe.com/f4m` namespace). Pullbay reads
F4M 1.0 manifests (`<media url bitrate width height bootstrapInfoId>`, each
offered as a quality, `<bootstrapInfo>` inline in base64 or at a `url`,
`<baseURL>`, `<metadata>`, `<duration>`) and F4M 2.0 manifests whose
`<media href>` points at another manifest. The bootstrap's `abst` box gives
the segment run table (`asrt`) and fragment run table (`afrt`), from which
every fragment's address is made: `<baseURL>/<media url>Seg<s>-Frag<n>`. The
manifest's own query string (Akamai's `hdnea`/`pvtoken` signature) and any
`<pv-2.0>` token go on every fragment, bootstrap and child manifest. Each
F4F fragment's `mdat` box holds FLV tags; the joined video is an FLV header,
the media's metadata as an `onMetaData` tag, then every fragment's tags.
`<drmAdditionalHeader>` marks DRM and is refused, as are live streams
(`<streamType>live</streamType>` or the bootstrap's live flag).

**Downloading.** Segments go one file each into `<file>.pullbayparts` next to
the output, written as `.tmp` and renamed once complete, up to **Connections
per file** at once, through the shared speed limit, each retried with the
usual backoff. Progress counts segments and bytes, and the window's bar
follows the segments. Canceling keeps the finished segments; a retry fetches
only the missing ones. Then each track is joined in order. With ffmpeg,
`ffmpeg -i video [-i audio] -c copy` makes one `.mp4` (or `.webm`) without
re-encoding; without it, an HLS video is saved as its `.ts` (or `.mp4` for
fragmented MP4 segments), an HDS video as its `.flv`, and a stream with
separate audio as two files, and
the entry carries a note saying so.

**Subtitles.** The video button's menu lists a stream's subtitle languages
with a checkbox each, the stream's default ones ticked; the ticked ones go
with whichever quality is clicked (not with the sound alone), as IDM keeps
them. Their WebVTT segments are fetched with the video's and joined into one
file, each segment's own header (and HLS's `X-TIMESTAMP-MAP`) dropped. With
ffmpeg they go into the video as subtitle tracks (`mov_text` in an MP4,
WebVTT in a WebM) with a three-letter language tag and the track's name;
without it each is saved beside the video as `<name>.<language>.vtt`. A
track that is not WebVTT text is skipped without failing the download. ffmpeg runs without a console window and
its error, if any, becomes the download's error. Streams land in the Video
category's folder whatever their extension.

**Batches and imports** go through `add_many`, which adds every item waiting
in one queue and announces the queue once, so 10,000 downloads do not send
10,000 queue updates. An item with a full-path `folder` is saved there under
its own name.

**The site grabber** runs in the daemon, so it uses the proxy settings, and
reports to the window that started it. It reads `robots.txt` once per site
(the group for `Pullbay` if there is one, else `*`; longest match wins, with `*`
and `$`), waits at least the set time between requests, and caps a site's
`Crawl-delay` at 30 seconds. Pages are parsed with html5ever, the HTML parser
from Servo, so markup inside scripts or comments is not mistaken for links.
It follows `<a>`, `<area>` and frames, keeps files linked from those and from
`<img>`, `<source>`, `<video>`, `<audio>`, `<embed>` and `<object>`, honors
`<base href>`, and only ever follows or lists `http` and `https` addresses.
It reads at most 5 MiB of any page, follows only page-like links (no
extension, or `.html`, `.php` and the like), and lists files without
fetching them. Closing the dialog or **Stop** ends a crawl within a tenth of
a second.

Pages read at once are fetched on their own threads, one batch at a time,
with the usual wait between batches; robots.txt is read before a site's
first batch. With `*` in the types, every link with a file name that is not
a page counts as a file. A project's login is stored in Credential Manager
as `Pullbay/grabber/<start host>/<user name>`, so projects on one site can use
different accounts (a password an older version kept under
`Pullbay/grabber/<start host>` is still read), sent to the start site when it asks, and
given to the downloads on that site. A login page's cookies come from the
extension's `cookies_wanted` request at crawl start, as the start site's do
with **Use my browser's sign-in**, or else from the pasted ones, which
project files keep encrypted for the user (as saved requests are) and
templates never keep. Each site's cookies go only to pages on that site's
host (and its subdomains when the crawl explores them), never to other
hosts the crawl reads; the downloads it adds for files on the login site
carry that site's cookies too, as captured browser downloads do, and
files on other hosts get none. Named filters are kept in
`%APPDATA%\Pullbay\grabber\filters.json` and named templates in
`grabber\templates\`. With scripts run, the extension closes every window
the page opens and reports its address; the crawl explores those only when
**Ignore the windows page scripts open** is off. A scheduled run sets the
project's when-done action, file and force-close choice on its queue for
that run only (`when_done_once`, `open_when_done_once`, `force_close_once`),
so the queue's own settings come back afterwards; a run that adds nothing
does them at once. With a stop time it also sets `stop_at_once`, and the
scheduler stops that queue run (its downloads wait again, progress kept)
once the time passes.

---

[← Back to the help pages](README.md)
