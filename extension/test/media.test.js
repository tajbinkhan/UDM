// Tests for the video button's media list (media.js) and the worker's side
// of it. Run with: node --test extension/test/*.test.js

const test = require("node:test");
const assert = require("node:assert/strict");

const { autoCaptureMedia, mediaFileExtension, classifyMedia, recordMedia, recordPage, videoPage, videoPageUrl, pageTitle, recentMedia, mediaMenu, mediaRequest, videoButtonAllowed, titleFileName, MEDIA_PER_TAB } = require("../media.js");
const { loadWorker } = require("./harness.js");
const { hiddenMenuItems } = require("../rules.js");

const same = (a, b) => assert.equal(JSON.stringify(a), JSON.stringify(b));
const response = (url, type, extra = {}) => ({
  url,
  statusCode: extra.status || 200,
  responseHeaders: [
    { name: "Content-Type", value: type },
    ...Object.entries(extra.headers || {}).map(([name, value]) => ({ name, value })),
  ],
});

test("a saved media file only ever gets a media extension", () => {
  assert.equal(mediaFileExtension({ url: "https://a/clip.webm", mime: "video/webm" }), "webm");
  // A server can send a program as video; it is saved as a video.
  assert.equal(mediaFileExtension({ url: "https://a/clip.exe", mime: "video/mp4" }), "mp4");
  assert.equal(mediaFileExtension({ url: "https://a/run.hta?x=1", mime: "audio/mpeg" }), "mp3");
  assert.equal(mediaFileExtension({ url: "https://a/stream", mime: "audio/ogg" }), "ogg");
});

test("responses are sorted into streams, files and the rest", () => {
  assert.equal(classifyMedia(response("https://v.net/master.m3u8?t=1", "application/vnd.apple.mpegurl")).kind, "hls");
  assert.equal(classifyMedia(response("https://v.net/a/playlist", "application/x-mpegURL")).kind, "hls");
  assert.equal(classifyMedia(response("https://v.net/manifest.mpd", "text/plain")).kind, "dash");
  assert.equal(classifyMedia(response("https://v.net/x", "application/dash+xml; charset=utf-8")).kind, "dash");
  const file = classifyMedia(response("https://v.net/movie.mp4", "video/mp4", { headers: { "Content-Length": "5000000" } }));
  same([file.kind, file.size], ["file", 5000000]);
  // A player reading a file in ranges reports the whole size.
  const ranged = classifyMedia(
    response("https://v.net/movie.webm", "video/webm", { status: 206, headers: { "Content-Range": "bytes 0-999/7340032" } }),
  );
  assert.equal(ranged.size, 7340032);
  assert.equal(classifyMedia(response("https://v.net/song.mp3", "application/octet-stream")).kind, "file");

  // Segments, small files, errors and everything else are not listed.
  assert.equal(classifyMedia(response("https://v.net/seg1.ts", "video/mp2t")), null);
  assert.equal(classifyMedia(response("https://v.net/chunk-5.m4s", "video/mp4")), null);
  assert.equal(classifyMedia(response("https://v.net/beep.mp4", "video/mp4", { headers: { "Content-Length": "4000" } })), null);
  assert.equal(classifyMedia(response("https://v.net/movie.mp4", "video/mp4", { status: 404 })), null);
  assert.equal(classifyMedia(response("https://v.net/page", "text/html")), null);
  assert.equal(classifyMedia(response("blob:https://v.net/1", "video/mp4")), null);
});

test("each tab keeps a bounded list, one entry per URL", () => {
  const store = new Map();
  const a = recordMedia(store, 1, "https://v.net/a.mp4", { kind: "file", mime: "video/mp4", size: 0 }, 1);
  const again = recordMedia(store, 1, "https://v.net/a.mp4", { kind: "file", mime: "video/mp4", size: 900000 }, 2);
  assert.equal(a.id, again.id);
  assert.equal(again.size, 900000, "a later response teaches the size");
  for (let i = 0; i < MEDIA_PER_TAB + 5; i++) {
    recordMedia(store, 2, `https://v.net/${i}.mp4`, { kind: "file", mime: "video/mp4", size: 0 }, i);
  }
  assert.equal(store.get(2).size, MEDIA_PER_TAB);
  assert.ok(!store.get(2).has("https://v.net/0.mp4"), "the oldest go first");
  assert.equal(store.get(1).size, 1);
});

test("the menu lists qualities and hides a master's own playlists", () => {
  const entries = [
    { id: 1, url: "https://v.net/master.m3u8", kind: "hls", mime: "", size: 0, time: 1 },
    { id: 2, url: "https://v.net/720/index.m3u8", kind: "hls", mime: "", size: 0, time: 2 },
    { id: 3, url: "https://v.net/clip.mp4", kind: "file", mime: "video/mp4", size: 3000000, time: 3 },
    { id: 4, url: "https://drm.net/m.mpd", kind: "dash", mime: "", size: 0, time: 0 },
  ];
  const infos = new Map([
    [
      "https://v.net/master.m3u8",
      {
        kind: "hls",
        video: [
          { id: "https://v.net/720/index.m3u8", label: "720p, 3.0 Mbps", size: 7500000 },
          { id: "https://v.net/360/index.m3u8", label: "360p, 900 kbps", size: 2250000 },
        ],
        audio: [],
        drm: false,
      },
    ],
    ["https://v.net/720/index.m3u8", { kind: "hls", video: [{ id: "", label: "Default" }], audio: [] }],
    ["https://drm.net/m.mpd", { kind: "dash", video: [], audio: [], drm: true, error: "protected" }],
  ]);
  const menu = mediaMenu(entries, infos);
  same(
    menu.map((i) => [i.entry, i.label]),
    [
      [3, "MP4 video"],
      [1, "HLS stream"],
      [4, "DASH stream"],
    ],
  );
  same(menu[0].options, [{ label: "Download", size: 3000000 }]);
  same(
    menu[1].options.filter((o) => !o.audioOnly).map((o) => [o.label, o.video]),
    [
      ["720p, 3.0 Mbps", "https://v.net/720/index.m3u8"],
      ["360p, 900 kbps", "https://v.net/360/index.m3u8"],
    ],
  );
  assert.equal(menu[2].drm, true);
  assert.equal(menu[2].options.length, 0, "nothing to choose for a protected stream");
});

test("the button can be off everywhere or on some sites", () => {
  assert.equal(videoButtonAllowed(null, "https://a.com/"), true);
  assert.equal(videoButtonAllowed({ video_button: true, video_button_excluded_sites: ["*.tube.net"] }, "https://www.tube.net/v"), false);
  assert.equal(videoButtonAllowed({ video_button: true, video_button_excluded_sites: ["tube.net"] }, "https://other.net/v"), true);
  assert.equal(videoButtonAllowed({ video_button: false }, "https://other.net/v"), false);
});

test("page titles become safe file names", () => {
  assert.equal(titleFileName('My "great" video: part 1/2', "mp4"), "My great video part 1 2.mp4");
  assert.equal(titleFileName("  ...  ", "mp4"), undefined);
  assert.equal(titleFileName("x".repeat(300), "mp4").length, 124);
});

function connected(settings = {}) {
  const w = loadWorker();
  w.port.onMessage.fire({ op: "hello", name: "uidm-daemon", version: "x" });
  w.port.onMessage.fire({
    op: "settings",
    settings: { capture_types: [], capture_min_size_kb: 0, capture_excluded_sites: [], video_button: true, video_button_excluded_sites: [], ...settings },
  });
  return w;
}

const tick = () => new Promise((r) => setTimeout(r, 0));

const seen = (w, tabId, url, type, headers) =>
  w.chrome.webRequest.onHeadersReceived.fire({ tabId, type: "xmlhttprequest", ...response(url, type, { headers }) });

test("the settings choose the video types and the smallest file", () => {
  const rules = { video_types: ["mp4", "m3u8"], video_min_size_kb: 1024 };
  const mp4 = (size) => response("https://v.net/movie.mp4", "video/mp4", { headers: { "Content-Length": String(size) } });
  assert.equal(classifyMedia(mp4(2 << 20), rules).kind, "file");
  assert.equal(classifyMedia(mp4(500 << 10), rules), null, "under the minimum");
  assert.equal(classifyMedia(mp4(500 << 10)).kind, "file", "the default minimum is 128 KB");
  assert.equal(classifyMedia(response("https://v.net/a.webm", "video/webm"), rules), null, "not a listed type");
  assert.equal(classifyMedia(response("https://v.net/clip", "video/webm"), rules), null, "the type from the MIME type");
  assert.equal(classifyMedia(response("https://v.net/clip", "video/x-unknown"), rules).kind, "file", "an unknown type is kept");
  assert.equal(classifyMedia(response("https://v.net/master.m3u8", "application/vnd.apple.mpegurl"), rules).kind, "hls");
  assert.equal(classifyMedia(response("https://v.net/a.mpd", "application/dash+xml"), rules), null, "DASH is left out");
  assert.equal(classifyMedia(response("https://v.net/a.mpd", "application/dash+xml"), { video_types: [] }), null);
});

test("the newest media across tabs come first", () => {
  const store = new Map();
  recordMedia(store, 1, "https://a.net/1.mp4", { kind: "file", mime: "video/mp4", size: 0 }, 100);
  recordMedia(store, 2, "https://b.net/2.m3u8", { kind: "hls", mime: "", size: 0 }, 300);
  recordMedia(store, 1, "https://a.net/3.mp4", { kind: "file", mime: "video/mp4", size: 0 }, 200);
  same(recentMedia(store, 2).map((r) => [r.tabId, r.entry.url]), [[2, "https://b.net/2.m3u8"], [1, "https://a.net/3.mp4"]]);
  assert.equal(recentMedia(new Map(), 10).length, 0);
});

test("the context menu has the items the settings keep, and the last videos", async () => {
  const w = connected();
  const items = () => [...w.chrome.contextMenus.items.keys()];
  w.port.onMessage.fire({ op: "settings", settings: { browser_menu_hidden: ["all_links", "download_later"] } });
  for (const gone of ["uidm-all-links", "uidm-later-link", "uidm-later-media"]) assert.ok(!items().includes(gone), gone);
  for (const kept of ["uidm-link", "uidm-media", "uidm-selection-links", "uidm-last-video", "uidm-recent"]) {
    assert.ok(items().includes(kept), kept);
  }
  await new Promise((r) => setTimeout(r, 350));
  assert.equal(w.chrome.contextMenus.items.get("uidm-last-video").enabled, false, "no video yet");

  await seen(w, 7, "https://v.net/old.mp4", "video/mp4", { "Content-Length": "9000000" });
  await new Promise((r) => setTimeout(r, 5));
  await seen(w, 8, "https://v.net/new.mp4", "video/mp4", { "Content-Length": "9000000" });
  await new Promise((r) => setTimeout(r, 350));
  assert.equal(w.chrome.contextMenus.items.get("uidm-last-video").enabled, true);
  const children = [...w.chrome.contextMenus.items.values()].filter((i) => i.parentId === "uidm-recent");
  same(children.map((c) => c.title), ["v.net (MP4 video)", "v.net (MP4 video)"]);

  await w.chrome.contextMenus.onClicked.fire({ menuItemId: "uidm-last-video" });
  assert.equal(w.sent.filter((m) => m.op === "add").at(-1).url, "https://v.net/new.mp4");
  await w.chrome.contextMenus.onClicked.fire({ menuItemId: children[1].id });
  assert.equal(w.sent.filter((m) => m.op === "add").at(-1).url, "https://v.net/old.mp4");

  // Showing everything again brings the items back.
  w.port.onMessage.fire({ op: "settings", settings: { browser_menu_hidden: [] } });
  assert.ok(items().includes("uidm-all-links"));
});

test("a tab's video goes to UIDM in the quality picked", async () => {
  const w = connected();
  await seen(w, 7, "https://v.net/master.m3u8", "application/vnd.apple.mpegurl");
  await seen(w, 8, "https://other.net/elsewhere.mp4", "video/mp4", { "Content-Length": "9000000" });

  const asked = w.content({ type: "video:menu" }, 7);
  await tick();
  // The worker asks UIDM what the stream offers.
  const probe = w.sent.find((m) => m.op === "probe_media");
  assert.equal(probe.download.url, "https://v.net/master.m3u8");
  assert.equal(probe.download.referrer, "https://site.net/watch");
  w.port.onMessage.fire({
    op: "media_info",
    request: probe.request,
    info: { kind: "hls", video: [{ id: "https://v.net/1080.m3u8", label: "1080p", size: 9 }], audio: [] },
  });
  const menu = await asked;
  assert.equal(menu.items.length, 1, "only this tab's media");
  const item = menu.items[0];
  assert.equal(item.options[0].video, "https://v.net/1080.m3u8");

  // A second menu uses the answer it has.
  await w.content({ type: "video:menu" }, 7);
  assert.equal(w.sent.filter((m) => m.op === "probe_media").length, 1);

  const ok = await w.content({ type: "video:download", entry: item.entry, video: item.options[0].video, title: "Cats: the movie" }, 7);
  assert.equal(ok.ok, true);
  const add = w.sent.find((m) => m.op === "add");
  same(
    [add.url, add.filename, add.media, add.referrer],
    ["https://v.net/master.m3u8", "Cats the movie.mp4", { kind: "hls", video: "https://v.net/1080.m3u8" }, "https://site.net/watch"],
  );

  // Another tab cannot pick this tab's entry.
  const denied = await w.content({ type: "video:download", entry: item.entry, title: "x" }, 8);
  assert.equal(denied.ok, false);
  // A new page in the tab starts its list over.
  await w.chrome.webRequest.onBeforeRequest.fire({ tabId: 7, type: "main_frame", method: "GET", url: "https://site.net/next" });
  assert.equal((await w.content({ type: "video:menu" }, 7)).items.length, 0);
});

test("a stream is asked for with the headers the player sent", async () => {
  const w = connected();
  const url = "https://cdn.v.net/hls/master.m3u8?token=abc";
  w.chrome.webRequest.onSendHeaders.fire({
    requestId: "41",
    tabId: 7,
    frameId: 0,
    type: "xmlhttprequest",
    method: "GET",
    url,
    requestHeaders: [
      { name: "Origin", value: "https://site.net" },
      { name: "Referer", value: "https://site.net/" },
      { name: "X-Stream-Key", value: "k1" },
      { name: "Cookie", value: "session=1" },
    ],
  });
  await w.chrome.webRequest.onHeadersReceived.fire({ requestId: "41", tabId: 7, type: "xmlhttprequest", ...response(url, "application/vnd.apple.mpegurl") });

  const asked = w.content({ type: "video:menu" }, 7);
  await tick();
  const probe = w.sent.find((m) => m.op === "probe_media");
  same(probe.download.request_headers, [
    ["Origin", "https://site.net"],
    ["Referer", "https://site.net/"],
    ["X-Stream-Key", "k1"],
  ]);
  assert.equal(probe.download.referrer, "https://site.net/", "the Referer the player sent, not the page's address");
  w.port.onMessage.fire({ op: "media_info", request: probe.request, info: { kind: "hls", video: [{ id: "", label: "Default" }], audio: [] } });
  const item = (await asked).items[0];

  await w.content({ type: "video:download", entry: item.entry, title: "Clip" }, 7);
  const add = w.sent.find((m) => m.op === "add");
  same(add.request_headers, probe.download.request_headers);
  assert.equal(add.referrer, "https://site.net/");
});

test("a media entry keeps its latest request's headers, without cookies", () => {
  const store = new Map();
  const found = { kind: "hls", mime: "", size: 0 };
  const e = recordMedia(store, 1, "https://v.net/a.m3u8", found, 1, 0, [["Origin", "https://a.net"]]);
  recordMedia(store, 1, "https://v.net/a.m3u8", found, 2, 0, [["Origin", "https://b.net"], ["Cookie", "x=1"], [":authority", "v.net"]]);
  recordMedia(store, 1, "https://v.net/a.m3u8", found, 3, 0, undefined);
  same(mediaRequest(e), { request_headers: [["Origin", "https://b.net"]] });
  same(mediaRequest(recordMedia(store, 1, "https://v.net/b.m3u8", found, 4)), {});
  same(mediaRequest({ headers: [["Referer", "android-app://x"]] }), { request_headers: [["Referer", "android-app://x"]] }, "only a web Referer is a referrer");
});

test("YouTube pages are one address per video", () => {
  const watch = "https://www.youtube.com/watch?v=jNQXAC9IVRw";
  assert.equal(videoPageUrl(watch), watch);
  assert.equal(videoPageUrl("https://m.youtube.com/watch?v=jNQXAC9IVRw&list=PL1&t=4"), watch);
  assert.equal(videoPageUrl("https://www.youtube.com/shorts/jNQXAC9IVRw"), watch);
  assert.equal(videoPageUrl("https://youtu.be/jNQXAC9IVRw?si=x"), watch);
  for (const url of ["https://www.youtube.com/", "https://www.youtube.com/@jawed", "https://notyoutube.com/watch?v=x", "nonsense", "https://www.youtube.com/watch?v=a%22b"]) {
    assert.equal(videoPageUrl(url), null, url);
  }
  assert.equal(pageTitle("(3) Me at the zoo - YouTube"), "Me at the zoo");
  assert.equal(pageTitle("A - B - YouTube"), "A - B");

  // Moving to another video replaces the tab's page, not its other media.
  const store = new Map();
  recordMedia(store, 1, "https://v.net/a.mp4", { kind: "file", mime: "video/mp4", size: 0 }, 1);
  recordPage(store, 1, watch, 2);
  recordPage(store, 1, "https://www.youtube.com/watch?v=other", 3);
  same([...store.get(1).keys()], ["https://v.net/a.mp4", "https://www.youtube.com/watch?v=other"]);
});

test("a YouTube video goes to UIDM as a page yt-dlp reads", async () => {
  const w = connected();
  const page = "https://www.youtube.com/watch?v=jNQXAC9IVRw";
  const asked = w.content({ type: "video:menu" }, 7, "https://www.youtube.com/watch?v=jNQXAC9IVRw&t=3");
  await tick();
  const probe = w.sent.find((m) => m.op === "probe_media");
  assert.equal(probe.download.url, page);
  w.port.onMessage.fire({
    op: "media_info",
    request: probe.request,
    info: { kind: "page", video: [{ id: "137", label: "1080p", size: 9 }], audio: [{ id: "140", label: "en" }] },
  });
  const menu = await asked;
  same(menu.items.map((i) => i.label), ["YouTube video"]);
  const item = menu.items[0];
  same(item.options.map((o) => o.label), ["1080p", "Sound only (M4A)", "Sound only (MP3)"]);

  await w.content({ type: "video:download", entry: item.entry, video: "137", title: "(2) Me at the zoo - YouTube" }, 7);
  const add = w.sent.find((m) => m.op === "add");
  same([add.url, add.filename, add.media], [page, "Me at the zoo.mp4", { kind: "page", video: "137" }]);

  // Another video in the same tab, without a page load, replaces it.
  await w.chrome.tabs.onUpdated.fire(7, { url: "https://www.youtube.com/shorts/abc" }, { incognito: false });
  const next = w.content({ type: "video:menu" }, 7, "https://www.youtube.com/shorts/abc");
  await tick();
  const second = w.sent.filter((m) => m.op === "probe_media").at(-1);
  assert.equal(second.download.url, "https://www.youtube.com/watch?v=abc");
  w.port.onMessage.fire({ op: "media_info", request: second.request, info: { kind: "page", video: [{ id: "18", label: "360p" }], audio: [] } });
  assert.equal((await next).items.length, 1);
});

test("Facebook, Instagram and Vimeo video pages are one address per video", () => {
  const cases = [
    ["https://www.facebook.com/NASA/videos/1234567890/", "https://www.facebook.com/watch/?v=1234567890", "facebook"],
    ["https://m.facebook.com/NASA/videos/vb.11/1234567890/?type=3", "https://www.facebook.com/watch/?v=1234567890", "facebook"],
    ["https://www.facebook.com/watch/?v=1234567890&ref=x", "https://www.facebook.com/watch/?v=1234567890", "facebook"],
    ["https://www.facebook.com/watch?v=1234567890", "https://www.facebook.com/watch/?v=1234567890", "facebook"],
    ["https://www.facebook.com/reel/987654321?s=1", "https://www.facebook.com/reel/987654321", "facebook"],
    ["https://fb.watch/aBc-12/", "https://fb.watch/aBc-12/", "facebook"],
    ["https://www.instagram.com/p/CxYz_12-a/?img_index=1", "https://www.instagram.com/p/CxYz_12-a/", "instagram"],
    ["https://instagram.com/reel/CxYz12/", "https://www.instagram.com/reel/CxYz12/", "instagram"],
    ["https://www.instagram.com/reels/CxYz12/", "https://www.instagram.com/reel/CxYz12/", "instagram"],
    ["https://www.instagram.com/tv/CxYz12", "https://www.instagram.com/tv/CxYz12/", "instagram"],
    ["https://vimeo.com/76979871", "https://vimeo.com/76979871", "vimeo"],
    ["https://vimeo.com/76979871/a1b2c3d4e5", "https://vimeo.com/76979871/a1b2c3d4e5", "vimeo"],
    ["https://vimeo.com/channels/staffpicks/76979871", "https://vimeo.com/76979871", "vimeo"],
    ["https://player.vimeo.com/video/76979871?h=abc123&autoplay=1", "https://player.vimeo.com/video/76979871?h=abc123", "vimeo"],
    ["https://www.youtube.com/watch?v=jNQXAC9IVRw", "https://www.youtube.com/watch?v=jNQXAC9IVRw", "youtube"],
  ];
  for (const [url, page, site] of cases) same(videoPage(url), { site, url: page });
  for (const url of [
    "https://www.facebook.com/",
    "https://www.facebook.com/NASA",
    "https://www.facebook.com/NASA/videos/",
    "https://www.facebook.com/watch/?v=abc",
    "https://fb.watch/",
    "https://www.instagram.com/nasa/",
    "https://www.instagram.com/explore/",
    "https://vimeo.com/",
    "https://vimeo.com/staff",
    "https://vimeo.com/channels/staffpicks",
    "https://player.vimeo.com/video/abc",
    "https://notvimeo.com/76979871",
    "https://evil.net/www.facebook.com/x/videos/1",
    "ftp://vimeo.com/76979871",
  ]) {
    assert.equal(videoPageUrl(url), null, url);
  }
});

test("a Facebook video goes to UIDM as a page, also after moving there in the page", async () => {
  const w = connected();
  // The site moves to a video without loading a page.
  await w.chrome.webNavigation.onHistoryStateUpdated.fire({ tabId: 7, frameId: 0, url: "https://www.facebook.com/NASA/videos/42/" });
  const asked = w.content({ type: "video:menu" }, 7, "https://www.facebook.com/NASA");
  await tick();
  const probe = w.sent.find((m) => m.op === "probe_media");
  assert.equal(probe.download.url, "https://www.facebook.com/watch/?v=42");
  w.port.onMessage.fire({ op: "media_info", request: probe.request, info: { kind: "page", video: [{ id: "hd", label: "720p" }], audio: [] } });
  const menu = await asked;
  same(menu.items.map((i) => i.label), ["Facebook video"]);
  await w.content({ type: "video:download", entry: menu.items[0].entry, video: "hd", title: "Launch | Facebook" }, 7, "https://www.facebook.com/NASA/videos/42/");
  const add = w.sent.find((m) => m.op === "add");
  same([add.url, add.filename, add.media], ["https://www.facebook.com/watch/?v=42", "Launch.mp4", { kind: "page", video: "hd" }]);

  // Another video replaces it.
  await w.chrome.webNavigation.onHistoryStateUpdated.fire({ tabId: 7, frameId: 0, url: "https://vimeo.com/76979871" });
  const next = w.content({ type: "video:menu" }, 7, "https://vimeo.com/76979871");
  await tick();
  const second = w.sent.filter((m) => m.op === "probe_media").at(-1);
  assert.equal(second.download.url, "https://vimeo.com/76979871");
  w.port.onMessage.fire({ op: "media_info", request: second.request, info: { kind: "page", video: [{ id: "x", label: "1080p" }], audio: [] } });
  same((await next).items.map((i) => i.label), ["Vimeo video"]);
});

test("HDS manifests are streams and their fragments are left out", () => {
  assert.equal(classifyMedia(response("https://v.net/z/manifest.f4m?hdnea=x", "text/xml")).kind, "hds");
  assert.equal(classifyMedia(response("https://v.net/z/manifest", "application/f4m+xml")).kind, "hds");
  assert.equal(classifyMedia(response("https://v.net/z/video_1000Seg1-Frag3", "video/f4f")), null);
  // The video types can leave HDS out.
  assert.equal(classifyMedia(response("https://v.net/z/manifest.f4m", "text/xml"), { video_types: ["m3u8"] }), null);
  const entries = [{ id: 1, url: "https://v.net/z/manifest.f4m", kind: "hds", time: 1 }];
  const infos = new Map([["https://v.net/z/manifest.f4m", { kind: "hds", video: [{ id: "video_1000", label: "1.0 Mbps" }], audio: [] }]]);
  const menu = mediaMenu(entries, infos);
  same(menu.map((i) => i.label), ["HDS stream"]);
  same(menu[0].options[0], { label: "1.0 Mbps", size: null, video: "video_1000" });
});

test("content scripts cannot use the popup's commands", async () => {
  const w = connected();
  const answer = await Promise.race([
    w.content({ type: "add", url: "https://evil.net/x.exe" }, 3),
    new Promise((r) => setTimeout(() => r("no answer"), 50)),
  ]);
  assert.equal(answer, "no answer");
  assert.equal(w.sent.filter((m) => m.op === "add").length, 0);
});

test("the button follows the settings, and the popup changes them", async () => {
  const w = connected({ video_button_excluded_sites: ["*.quiet.net"] });
  assert.equal((await w.content({ type: "video:enabled" }, 1, "https://www.quiet.net/v")).enabled, false);
  assert.equal((await w.content({ type: "video:enabled" }, 1, "https://loud.net/v")).enabled, true);

  await w.popup({ type: "setVideoButton", site: "loud.net", siteOn: false });
  let set = w.sent.filter((m) => m.op === "set_settings").at(-1);
  same(set.settings.video_button_excluded_sites, ["*.quiet.net", "loud.net"]);
  await w.popup({ type: "setVideoButton", site: "www.quiet.net", siteOn: true });
  set = w.sent.filter((m) => m.op === "set_settings").at(-1);
  same(set.settings.video_button_excluded_sites, []);
  await w.popup({ type: "setVideoButton", on: false });
  set = w.sent.filter((m) => m.op === "set_settings").at(-1);
  assert.equal(set.settings.video_button, false);
});

test("a stream offers its sound alone, from its audio track or its smallest video", () => {
  const entries = [
    { id: 1, url: "https://v.net/a.mpd", kind: "dash", time: 2 },
    { id: 2, url: "https://v.net/b.m3u8", kind: "hls", time: 1 },
  ];
  const infos = new Map([
    ["https://v.net/a.mpd", { video: [{ id: "v1", label: "1080p" }], audio: [{ id: "a1", label: "English", size: 900 }] }],
    ["https://v.net/b.m3u8", { video: [{ id: "big", label: "720p" }, { id: "small", label: "240p" }], audio: [] }],
  ]);
  const menu = mediaMenu(entries, infos);
  const sound = (item) => item.options.filter((o) => o.audioOnly);
  same(sound(menu[0]).map((o) => [o.label, o.audio, o.video, o.size]), [["Sound only (M4A)", "a1", undefined, 900], ["Sound only (MP3)", "a1", undefined, 900]]);
  same(sound(menu[1]).map((o) => [o.audioOnly, o.video]), [["m4a", "small"], ["mp3", "small"]]);
});

test("choosing the sound alone sends an audio_only stream", async () => {
  const w = loadWorker();
  w.port.onMessage.fire({ op: "hello", name: "uidm-daemon", version: "x" });
  w.chrome.webRequest.onHeadersReceived.fire({
    tabId: 7,
    url: "https://v.net/master.m3u8",
    type: "xmlhttprequest",
    responseHeaders: [{ name: "Content-Type", value: "application/vnd.apple.mpegurl" }],
  });
  const menu = await new Promise((resolve) => {
    const asked = w.content({ type: "video:menu" }, 7);
    setTimeout(() => {
      const probe = w.sent.find((m) => m.op === "probe_media");
      w.port.onMessage.fire({ op: "media_info", request: probe.request, info: { kind: "hls", video: [{ id: "v", label: "720p" }], audio: [] } });
    }, 0);
    asked.then(resolve);
  });
  const item = menu.items[0];
  const mp3 = item.options.find((o) => o.audioOnly === "mp3");
  await w.content({ type: "video:download", entry: item.entry, video: mp3.video, audioOnly: "mp3", title: "Talk" }, 7);
  const add = w.sent.find((m) => m.op === "add");
  assert.equal(add.filename, "Talk.mp3");
  assert.equal(add.media.audio_only, "mp3");
});
test("subtitle tracks are listed, ticked by default, and sent with the quality", async () => {
  const w = loadWorker();
  w.port.onMessage.fire({ op: "hello", name: "uidm-daemon", version: "x" });
  w.chrome.webRequest.onHeadersReceived.fire({
    tabId: 7,
    url: "https://v.net/master.m3u8",
    type: "xmlhttprequest",
    responseHeaders: [{ name: "Content-Type", value: "application/vnd.apple.mpegurl" }],
  });
  const subtitles = [
    { id: "https://v.net/en.m3u8", label: "English", language: "en", default: true },
    { id: "https://v.net/de.m3u8", label: "Deutsch", language: "de" },
  ];
  const menu = await new Promise((resolve) => {
    const asked = w.content({ type: "video:menu" }, 7);
    setTimeout(() => {
      const probe = w.sent.find((m) => m.op === "probe_media");
      w.port.onMessage.fire({ op: "media_info", request: probe.request, info: { kind: "hls", video: [{ id: "v", label: "720p" }], audio: [], subtitles } });
    }, 0);
    asked.then(resolve);
  });
  const item = menu.items[0];
  same(item.subtitles, [
    { id: "https://v.net/en.m3u8", label: "English", on: true },
    { id: "https://v.net/de.m3u8", label: "Deutsch", on: false },
  ]);
  await w.content({ type: "video:download", entry: item.entry, video: "v", subtitles: ["https://v.net/de.m3u8", 5], title: "Talk" }, 7);
  same(w.sent.find((m) => m.op === "add").media, { kind: "hls", video: "v", subtitles: ["https://v.net/de.m3u8"] });
  // The sound alone takes no subtitles.
  await w.content({ type: "video:download", entry: item.entry, audioOnly: "mp3", subtitles: ["https://v.net/de.m3u8"], title: "Talk" }, 7);
  assert.equal(w.sent.filter((m) => m.op === "add").at(-1).media.subtitles, undefined);
});

test("an address that is no stream is left out when another can be downloaded", () => {
  const entries = [
    { id: 1, url: "https://v.net/real.m3u8", kind: "hls", time: 1 },
    { id: 2, url: "https://v.net/coded.m3u8", kind: "hls", time: 2 },
  ];
  const notStream = { error: "This address is not an HLS playlist or a DASH manifest.", not_stream: true };
  const infos = new Map([
    ["https://v.net/real.m3u8", { video: [{ id: "", label: "Default" }], audio: [] }],
    ["https://v.net/coded.m3u8", notStream],
  ]);
  const menu = mediaMenu(entries, infos);
  same(menu.map((i) => i.entry), [1]);
  assert.equal("notStream" in menu[0], false);
  // Alone, it stays, so the menu says why there is nothing to download.
  const alone = mediaMenu([entries[1]], new Map([["https://v.net/coded.m3u8", notStream]]));
  same(alone.map((i) => [i.entry, i.error]), [[2, notStream.error]]);
});

for (const firefox of [false, true]) {
  test(`a video file caught under a playlist's name is offered and sent as a file${firefox ? " in Firefox" : ""}`, async () => {
    const w = loadWorker({ firefox });
    w.port.onMessage.fire({ op: "hello", name: "uidm-daemon", version: "x" });
    w.chrome.webRequest.onHeadersReceived.fire({
      tabId: 7,
      url: "https://v.net/movie.m3u8?t=1",
      type: "xmlhttprequest",
      responseHeaders: [{ name: "Content-Type", value: "application/vnd.apple.mpegurl" }],
    });
    const menu = await new Promise((resolve) => {
      const asked = w.content({ type: "video:menu" }, 7);
      setTimeout(() => {
        const probe = w.sent.find((m) => m.op === "probe_media");
        w.port.onMessage.fire({ op: "media_info", request: probe.request, info: { video: [], audio: [], file: { ext: "mp4", size: 5000000 } } });
      }, 0);
      asked.then(resolve);
    });
    same(menu.items.map((i) => [i.label, i.options.map((o) => [o.label, o.size])]), [["MP4 video", [["Download", 5000000]]]]);
    await w.content({ type: "video:download", entry: menu.items[0].entry, title: "Film" }, 7);
    const add = w.sent.find((m) => m.op === "add");
    assert.equal(add.url, "https://v.net/movie.m3u8?t=1");
    assert.equal(add.filename, "Film.mp4");
    assert.equal(add.media, undefined);
    // It is a file now: the next menu does not probe it again.
    const before = w.sent.filter((m) => m.op === "probe_media").length;
    await w.content({ type: "video:menu" }, 7);
    assert.equal(w.sent.filter((m) => m.op === "probe_media").length, before);
  });
}

test("a whole file's type comes from what UIDM found in it", () => {
  const { adoptFile } = require("../media.js");
  const entry = { id: 1, url: "https://v.net/x.m3u8", kind: "hls", mime: "audio/mpegurl", size: 0, time: 1 };
  assert.equal(adoptFile(entry, { file: { ext: "ts", size: 10 } }), true);
  assert.equal(mediaFileExtension(entry), "ts");
  same(mediaMenu([entry], new Map()).map((i) => i.label), ["TS video"]);
  assert.equal(adoptFile({ kind: "hls" }, { file: { ext: "exe" } }), false);
  assert.equal(adoptFile({ kind: "hls" }, { video: [] }), false);
});

test("Chromium browsers and Firefox each have their own context menu list", () => {
  const rules = { browser_menu_hidden_chromium: ["all_links"], browser_menu_hidden_firefox: ["recent_videos", "download"] };
  same(hiddenMenuItems(rules, false), ["all_links"]);
  same(hiddenMenuItems(rules, true), ["recent_videos", "download"]);
  // An older UIDM's one list serves both.
  same(hiddenMenuItems({ browser_menu_hidden: ["last_video"] }, true), ["last_video"]);
  same(hiddenMenuItems(null, false), []);

  const lists = { browser_menu_hidden_chromium: ["all_links"], browser_menu_hidden_firefox: ["download"] };
  const menu = (w) => [...w.chrome.contextMenus.items.keys()];
  const chrome = loadWorker();
  chrome.port.onMessage.fire({ op: "hello", name: "uidm-daemon", version: "x" });
  chrome.port.onMessage.fire({ op: "settings", settings: lists });
  assert.ok(!menu(chrome).includes("uidm-all-links") && menu(chrome).includes("uidm-link"));
  const firefox = loadWorker({ firefox: true });
  firefox.port.onMessage.fire({ op: "hello", name: "uidm-daemon", version: "x" });
  firefox.port.onMessage.fire({ op: "settings", settings: lists });
  assert.ok(menu(firefox).includes("uidm-all-links") && !menu(firefox).includes("uidm-link"));
});

test("web players' files of the listed types go to UIDM without the video button", async () => {
  const rules = { video_auto_capture: true, video_auto_types: ["mp4", "mp3"], capture_excluded_sites: ["blocked.net"] };
  const player = (url, type, extra = {}) => ({ type: "media", ...response(url, type, extra) });
  assert.equal(autoCaptureMedia(player("https://v.net/a.mp4", "video/mp4"), rules), true);
  assert.equal(autoCaptureMedia(player("https://v.net/song", "audio/mpeg"), rules), true, "the type from the MIME type");
  assert.equal(autoCaptureMedia(player("https://v.net/a.webm", "video/webm"), rules), false, "not a listed type");
  assert.equal(autoCaptureMedia(player("https://v.net/clip", "video/x-unknown"), rules), false, "an unknown type");
  assert.equal(autoCaptureMedia(player("https://v.net/a.m3u8", "application/vnd.apple.mpegurl"), { ...rules, video_auto_types: ["m3u8"] }), false, "no streams");
  assert.equal(autoCaptureMedia(player("https://v.net/s.ts", "video/mp2t"), rules), false, "no segments");
  assert.equal(autoCaptureMedia({ ...player("https://v.net/a.mp4", "video/mp4"), type: "xmlhttprequest" }, rules), false, "a script's fetch");
  assert.equal(autoCaptureMedia(player("https://blocked.net/a.mp4", "video/mp4"), rules), false, "an excluded site");
  assert.equal(autoCaptureMedia(player("https://v.net/a.mp4", "video/mp4"), { ...rules, video_auto_capture: false }), false, "off");
  assert.equal(autoCaptureMedia(player("https://v.net/a.mp4", "video/mp4"), rules, new Set(["https://v.net/a.mp4"])), false, "once");

  const w = connected(rules);
  const play = (tabId, url) =>
    w.chrome.webRequest.onHeadersReceived.fire({ tabId, initiator: "https://v.net", ...player(url, "video/mp4", { headers: { "Content-Length": "9000000" } }) });
  const adds = () => w.sent.filter((m) => m.op === "add").map((m) => m.url);
  await play(3, "https://v.net/a.mp4");
  await tick();
  await play(3, "https://v.net/a.mp4");
  await tick();
  same(adds(), ["https://v.net/a.mp4"]);
  assert.equal(w.sent.filter((m) => m.op === "add")[0].referrer, "https://v.net");
  // Another tab, or the same tab on a new page, takes it again.
  await play(4, "https://v.net/a.mp4");
  await w.chrome.webRequest.onBeforeRequest.fire({ tabId: 3, type: "main_frame", url: "https://v.net/next" });
  await play(3, "https://v.net/a.mp4");
  await tick();
  assert.equal(adds().length, 3);

  // Off by default.
  const plain = connected();
  await plain.chrome.webRequest.onHeadersReceived.fire({ tabId: 3, ...player("https://v.net/a.mp4", "video/mp4") });
  await tick();
  assert.equal(plain.sent.filter((m) => m.op === "add").length, 0);
});
