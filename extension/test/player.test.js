// Tests for the video button on more players (#153): <audio>, <object> and
// <embed> players, each player's own streams on a feed, hiding the button
// when its player is covered, names from the top page without the site's
// name, single-page sites starting their list over, and tabs open at
// install getting the content script. Run with:
// node --test extension/test/*.test.js

const test = require("node:test");
const assert = require("node:assert/strict");

const { siteLabel, stripSiteName, videoName, noteMediaStart, startedElement, elementEntries, forgetMedia, recordMedia, START_GRACE_MS } = require("../chromium/media.js");
const { mediaPlugin, playerContainer, samplePoints, playerCovered } = require("../chromium/rules.js");
const { loadWorker } = require("./harness.js");

const same = (a, b) => assert.equal(JSON.stringify(a), JSON.stringify(b));
const tick = () => new Promise((r) => setTimeout(r, 0));
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
// What background.js keeps from just before a page move (PAGE_MOVE_GRACE_MS).
const PAGE_GRACE = 1000;

test("a title loses the site's name after it, on any site", () => {
  assert.equal(siteLabel("www.youtube.com"), "youtube");
  assert.equal(siteLabel("www.bbc.co.uk"), "bbc");
  assert.equal(siteLabel("soundcloud.com"), "soundcloud");
  assert.equal(siteLabel("localhost"), "localhost");

  assert.equal(stripSiteName("Cute cat - YouTube", "www.youtube.com"), "Cute cat");
  assert.equal(stripSiteName("Song | SoundCloud", "soundcloud.com"), "Song");
  assert.equal(stripSiteName("Talk — Vimeo", "vimeo.com"), "Talk");
  assert.equal(stripSiteName("Review – The Verge", "www.theverge.com"), "Review");
  assert.equal(stripSiteName("Storm hits coast - BBC News", "www.bbc.co.uk"), "Storm hits coast");
  assert.equal(stripSiteName("Clip | Daily Show", "cc.com", "Daily Show"), "Clip", "named by og:site_name");
  // Dashes in the middle stay, and so does a last part that is not the site.
  assert.equal(stripSiteName("A - B - YouTube", "youtube.com"), "A - B");
  assert.equal(stripSiteName("Spider-Man - Trailer", "youtube.com"), "Spider-Man - Trailer");
  assert.equal(stripSiteName("Tips - YouTubers' guide", "youtube.com"), "Tips - YouTubers' guide");
  assert.equal(stripSiteName("Cute cat - YouTube", "vimeo.com"), "Cute cat - YouTube", "another site's name");
  assert.equal(stripSiteName("YouTube", "youtube.com"), "YouTube");

  // og:title first, when the title holds it; else the title.
  const url = "https://www.youtube.com/watch?v=x";
  assert.equal(videoName({ title: "(2) Cute cat - YouTube", ogTitle: "Cute cat" }, url), "Cute cat");
  assert.equal(videoName({ title: "", ogTitle: "Cute cat" }, url), "Cute cat");
  assert.equal(videoName({ title: "Next video - YouTube", ogTitle: "First video" }, url), "Next video", "a stale og:title");
  assert.equal(videoName({ title: "Song | SoundCloud" }, "https://soundcloud.com/a/b"), "Song");
  assert.equal(videoName(null, url), "");
});

test("a stream belongs to the player that was playing when its frame asked for it", () => {
  const starts = new Map();
  noteMediaStart(starts, 1, 0, 1, 100);
  noteMediaStart(starts, 1, 0, 2, 500);
  noteMediaStart(starts, 1, 3, 9, 50);
  const s = starts.get(1);
  assert.equal(startedElement(s, 0, 300), 1);
  assert.equal(startedElement(s, 0, 600), 2);
  assert.equal(startedElement(s, 3, 600), 9, "another frame's player");
  assert.equal(startedElement(s, 0, 100 - START_GRACE_MS + 10), 1, "asked just before its player started");
  assert.equal(startedElement(s, 0, 100 - START_GRACE_MS - 10), null);
  assert.equal(startedElement(s, 5, 600), null);

  const store = new Map();
  const a = recordMedia(store, 1, "https://v.net/a.m3u8", { kind: "hls", mime: "", size: 0 }, 200, 0);
  const b = recordMedia(store, 1, "https://v.net/b.m3u8", { kind: "hls", mime: "", size: 0 }, 700, 0);
  const file = recordMedia(store, 1, "https://v.net/c.mp4", { kind: "file", mime: "video/mp4", size: 0 }, 800, 0);
  const page = { id: 99, url: "https://www.youtube.com/watch?v=x", kind: "page", time: 1, requests: [] };
  const entries = [a, b, file, page];
  same(elementEntries(entries, s, 0, 1, []).map((e) => e.url), [a.url, page.url]);
  same(elementEntries(entries, s, 0, 2, []).map((e) => e.url), [b.url, page.url]);
  // A file goes by its address, not by time.
  same(elementEntries(entries, s, 0, 3, ["https://v.net/c.mp4", "blob:https://v.net/1"]).map((e) => e.url), [file.url, page.url]);
  // Nothing of its own: everything.
  same(elementEntries(entries, s, 0, 7, []).length, 4);
  same(elementEntries(entries, s, 0, null, null).length, 4);

  // Asked for again while another player plays: it is that one's too.
  recordMedia(store, 1, "https://v.net/a.m3u8", { kind: "hls", mime: "", size: 0 }, 900, 0);
  same(elementEntries(entries, s, 0, 2, []).map((e) => e.url), [a.url, b.url, page.url]);
});

test("a single-page site's move forgets what came before it", () => {
  const store = new Map();
  recordMedia(store, 1, "https://v.net/old.m3u8", { kind: "hls", mime: "", size: 0 }, 100);
  recordMedia(store, 1, "https://v.net/again.m3u8", { kind: "hls", mime: "", size: 0 }, 100);
  recordMedia(store, 1, "https://v.net/again.m3u8", { kind: "hls", mime: "", size: 0 }, 600);
  recordMedia(store, 1, "https://v.net/new.m3u8", { kind: "hls", mime: "", size: 0 }, 700);
  assert.equal(forgetMedia(store, 1, 500), true);
  same([...store.get(1).keys()], ["https://v.net/again.m3u8", "https://v.net/new.m3u8"]);
  assert.equal(forgetMedia(store, 1, 500), false);
  assert.equal(forgetMedia(store, 1, 1000), true);
  assert.equal(store.has(1), false);
  assert.equal(forgetMedia(store, 2, 1000), false);
});

test("<object> and <embed> players are told from other plugins", () => {
  assert.equal(mediaPlugin("video/mp4", ""), true);
  assert.equal(mediaPlugin("audio/mpeg; codecs=mp3", ""), true);
  assert.equal(mediaPlugin("application/vnd.apple.mpegurl", ""), true);
  assert.equal(mediaPlugin("", "https://v.net/clip.webm?x=1"), true);
  assert.equal(mediaPlugin("", "https://v.net/live/master.m3u8"), true);
  assert.equal(mediaPlugin("", "https://www.youtube.com/embed/abc"), true);
  assert.equal(mediaPlugin("", "https://player.vimeo.com/video/123"), true);
  assert.equal(mediaPlugin("application/pdf", "https://v.net/a.pdf"), false);
  assert.equal(mediaPlugin("", "https://v.net/widget.html"), false);
  assert.equal(mediaPlugin("", "javascript:alert(1)"), false);
  assert.equal(mediaPlugin(null, null), false);
});

// A stand-in for a page's elements: a tree of boxes.
function box(tag, rect, parent = null) {
  const node = {
    tagName: tag,
    parentElement: parent,
    getBoundingClientRect: () => ({ ...rect, right: rect.left + rect.width, bottom: rect.top + rect.height }),
    contains(other) {
      for (let n = other; n; n = n.parentElement) if (n === node) return true;
      return false;
    },
  };
  return node;
}

test("a player counts as covered only when something outside its player box is over it everywhere", () => {
  const html = box("HTML", { left: 0, top: 0, width: 1000, height: 2000 });
  const body = box("BODY", { left: 0, top: 0, width: 1000, height: 2000 }, html);
  const page = box("DIV", { left: 0, top: 0, width: 1000, height: 2000 }, body);
  const player = box("DIV", { left: 100, top: 100, width: 640, height: 400 }, page);
  const inner = box("DIV", { left: 100, top: 100, width: 640, height: 360 }, player);
  const video = box("VIDEO", { left: 100, top: 100, width: 640, height: 360 }, inner);
  const controls = box("DIV", { left: 100, top: 400, width: 640, height: 100 }, player);
  const dialog = box("DIV", { left: 0, top: 0, width: 1000, height: 800 }, body);
  const button = box("DIV", { left: 600, top: 108, width: 100, height: 28 }, body);

  assert.equal(playerContainer(video), player, "up to the player's box, not the page");
  const points = samplePoints(video.getBoundingClientRect(), 1000, 800);
  assert.equal(points.length, 5);
  same(points[0], [420, 280]);
  // Kept inside the window.
  assert.ok(samplePoints({ left: -500, top: 700, width: 400, height: 400 }, 1000, 800).every(([x, y]) => x >= 0 && x < 1000 && y >= 0 && y < 800));

  const covered = (hitAt) => playerCovered(video, playerContainer(video), points, hitAt, button);
  assert.equal(covered(() => video), false);
  assert.equal(covered(() => controls), false, "the player's own overlay");
  assert.equal(covered(() => body), false, "nothing over it");
  assert.equal(covered(() => dialog), true);
  assert.equal(covered((x, y) => (x < 300 ? video : dialog)), false, "partly visible");
  assert.equal(covered(() => button), false, "UDM's button covers nothing");
  assert.equal(covered(() => null), false, "nothing to look at");
});

function connected() {
  const w = loadWorker();
  w.port.onMessage.fire({ op: "hello", name: "udm-daemon", version: "x" });
  w.port.onMessage.fire({
    op: "settings",
    settings: { capture_types: [], capture_min_size_kb: 0, capture_excluded_sites: [], video_button: true, video_button_excluded_sites: [] },
  });
  return w;
}

// A message from the content script of frame `frameId` in tab `tabId`.
const fromFrame = (w, msg, tabId, frameId, tab = {}) =>
  new Promise((resolve) =>
    w.chrome.runtime.onMessage.fire(
      msg,
      { id: "udm", tab: { id: tabId, url: "https://feed.net/", incognito: false, ...tab }, url: "https://feed.net/", frameId },
      resolve,
    ),
  );
const playlist = (w, tabId, url, frameId = 0) =>
  w.chrome.webRequest.onHeadersReceived.fire({
    tabId,
    frameId,
    url,
    type: "xmlhttprequest",
    statusCode: 200,
    responseHeaders: [{ name: "Content-Type", value: "application/vnd.apple.mpegurl" }],
  });

// Answer every probe the worker sent with one quality.
function answerProbes(w, done) {
  for (const p of w.sent.filter((m) => m.op === "probe_media")) {
    if (done.has(p.request)) continue;
    done.add(p.request);
    w.port.onMessage.fire({ op: "media_info", request: p.request, info: { kind: "hls", video: [{ id: "", label: p.download.url }], audio: [] } });
  }
}

test("on a feed each player's button lists that player's streams", async () => {
  const w = connected();
  await fromFrame(w, { type: "video:started", element: 1 }, 7, 0);
  await wait(5);
  await playlist(w, 7, "https://cdn.net/one.m3u8");
  await wait(5);
  await fromFrame(w, { type: "video:started", element: 2 }, 7, 0);
  await wait(5);
  await playlist(w, 7, "https://cdn.net/two.m3u8");

  const done = new Set();
  const menuOf = async (element) => {
    const asked = fromFrame(w, { type: "video:menu", element, srcs: ["blob:https://feed.net/x"] }, 7, 0);
    await tick();
    answerProbes(w, done);
    return (await asked).items.map((i) => i.options[0].label);
  };
  same(await menuOf(1), ["https://cdn.net/one.m3u8"]);
  same(await menuOf(2), ["https://cdn.net/two.m3u8"]);
  // A player nothing is known about gets the page's whole list.
  same((await menuOf(5)).sort(), ["https://cdn.net/one.m3u8", "https://cdn.net/two.m3u8"]);
  // Only its own streams are probed: the first menu asked about one.
  assert.equal(w.sent.filter((m) => m.op === "probe_media").length, 2);
});

test("a single-page site's move starts the tab's list over and resets its buttons", async () => {
  const w = connected();
  await playlist(w, 7, "https://cdn.net/old.m3u8");
  await wait(PAGE_GRACE + 20);
  await playlist(w, 7, "https://cdn.net/new.m3u8");
  await playlist(w, 8, "https://cdn.net/other-tab.m3u8");
  await w.chrome.webNavigation.onHistoryStateUpdated.fire({ tabId: 7, frameId: 0, url: "https://feed.net/next" });
  same(w.chrome.tabs.messages.map(([tabId, msg]) => [tabId, msg.type]), [[7, "video:reset"]]);
  // A frame's own history is not the page's.
  await w.chrome.webNavigation.onHistoryStateUpdated.fire({ tabId: 8, frameId: 4, url: "https://ads.net/x" });
  assert.equal(w.chrome.tabs.messages.length, 1);

  const done = new Set();
  const menuOf = async (tabId) => {
    const asked = fromFrame(w, { type: "video:menu" }, tabId, 0);
    await tick();
    answerProbes(w, done);
    return (await asked).items.map((i) => i.options[0].label);
  };
  same(await menuOf(7), ["https://cdn.net/new.m3u8"], "what came just before the move stays");
  same(await menuOf(8), ["https://cdn.net/other-tab.m3u8"]);
});
test("a video from a frame is named after the tab's top page", async () => {
  const w = connected();
  await playlist(w, 7, "https://cdn.net/v.m3u8", 3);
  const done = new Set();
  const asked = fromFrame(w, { type: "video:menu" }, 7, 3);
  await tick();
  answerProbes(w, done);
  const item = (await asked).items[0];

  w.chrome.tabs.reply = (msg, options) =>
    msg.type === "page:names" && options.frameId === 0 ? { title: "(1) Talk — Vimeo", ogTitle: "Talk", siteName: "Vimeo" } : undefined;
  await fromFrame(w, { type: "video:download", entry: item.entry, title: "Embedded player" }, 7, 3);
  assert.equal(w.sent.filter((m) => m.op === "add").at(-1).filename, "Talk.mp4");

  // No answer from the top frame: the tab's title.
  w.chrome.tabs.reply = () => {
    throw new Error("no receiver");
  };
  await fromFrame(w, { type: "video:download", entry: item.entry, title: "Embedded player" }, 7, 3, { title: "Song | SoundCloud", url: "https://soundcloud.com/a" });
  assert.equal(w.sent.filter((m) => m.op === "add").at(-1).filename, "Song.mp4");

  // The top frame names its own videos.
  await fromFrame(w, { type: "video:download", entry: item.entry, title: "Cute cat - Feed", ogTitle: "" }, 7, 0);
  assert.equal(w.sent.filter((m) => m.op === "add").at(-1).filename, "Cute cat.mp4");
});

for (const firefox of [false, true]) {
  test(`tabs open at install or the first connection get the content script${firefox ? " in Firefox" : ""}`, async () => {
    const w = loadWorker({ firefox });
    const calls = [];
    // Tab 1 has it in its top frame only; tab 2 nowhere; tab 3 cannot be scripted.
    const has = { 1: [true, false], 2: [false], 3: null };
    w.chrome.scripting = {
      executeScript: async (o) => {
        calls.push(o);
        if (has[o.target.tabId] === null) throw new Error("Cannot access contents of the page");
        if (o.func) return has[o.target.tabId].map((result, frameId) => ({ frameId, result }));
        return o.target.frameIds.map((frameId) => ({ frameId }));
      },
    };
    w.chrome.tabs.query = async (q) => {
      same(q, { url: ["http://*/*", "https://*/*"] });
      return [
        { id: 1, url: "https://a.net/" },
        { id: 2, url: "http://b.net/" },
        { id: 3, url: "https://chromewebstore.google.com/" },
        { id: 4, url: "https://c.net/", discarded: true },
      ];
    };
    await w.chrome.runtime.onInstalled.fire({ reason: "install" });
    await wait(5);
    const injected = calls.filter((o) => o.files);
    same(
      injected.map((o) => [o.target.tabId, o.target.frameIds, o.files]),
      [
        [1, [1], ["rules.js", "links.js", "content.js"]],
        [2, [0], ["rules.js", "links.js", "content.js"]],
      ],
    );
    // The check runs in every frame.
    assert.ok(calls.filter((o) => o.func).every((o) => o.target.allFrames === true));

    // The first connection does it too, once a browser session.
    calls.length = 0;
    w.port.onMessage.fire({ op: "hello", name: "udm-daemon", version: "x" });
    await wait(5);
    assert.equal(calls.filter((o) => o.files).length, 2);
    assert.equal(w.chrome.storage.session.data.contentInjected, true);
    calls.length = 0;
    w.port.onMessage.fire({ op: "hello", name: "udm-daemon", version: "x" });
    await wait(5);
    assert.equal(calls.length, 0);

    // A worker started again in the same session leaves it.
    const again = loadWorker({ firefox, session: { contentInjected: true } });
    again.chrome.scripting = w.chrome.scripting;
    again.chrome.tabs.query = w.chrome.tabs.query;
    again.port.onMessage.fire({ op: "hello", name: "udm-daemon", version: "x" });
    await wait(5);
    assert.equal(calls.length, 0);
  });
}
