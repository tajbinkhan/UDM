// Tests for capture turned off in one tab (the popup's "Capture in this
// tab", IDM's toolbar button), the skip and force keys held down applying
// to any download their tab starts, and private windows, which stay out of
// UDM unless its settings let them in (#152). Run with:
// node --test extension/test/*.test.js

const test = require("node:test");
const assert = require("node:assert/strict");

const { heldReport, heldApplies, greyPixels, HELD_REFRESH_MS, HELD_EXPIRE_MS } = require("../chromium/rules.js");
const { loadWorker, download } = require("./harness.js");

const same = (a, b) => assert.equal(JSON.stringify(a), JSON.stringify(b));
const h = (pairs) => Object.entries(pairs).map(([name, value]) => ({ name, value }));
const tick = () => new Promise((r) => setTimeout(r, 0));
const adds = (w) => w.sent.filter((m) => m.op === "add");

function connected(settings = {}, options = {}) {
  const w = loadWorker(options);
  w.port.onMessage.fire({ op: "hello", name: "udm-daemon", version: "x" });
  w.port.onMessage.fire({ op: "settings", settings: { capture_types: ["zip"], capture_min_size_kb: 0, capture_excluded_sites: [], ...settings } });
  return w;
}

// The browser getting `url` in tab `tabId`, answered with `headers`; what
// the webRequest listeners returned.
function browse(w, { id = "r1", url, tabId = 3, headers, incognito }) {
  const wr = w.chrome.webRequest;
  const d = { requestId: id, url, method: "GET", type: "main_frame", tabId, frameId: 0, parentFrameId: -1, incognito };
  wr.onSendHeaders.fire({ ...d, requestHeaders: [] });
  return wr.onHeadersReceived.call({ ...d, statusCode: 200, responseHeaders: h(headers) });
}

// The tab in front is `id`.
const front = (w, id, incognito = false) => {
  w.chrome.tabs.query = async () => [{ id, url: "https://site.net/", incognito }];
};

test("capture off in one tab leaves its downloads, forced links and players' files to the browser", async () => {
  const w = connected({ capture_types: [], video_auto_capture: true });
  await tick();
  let state = await w.popup({ type: "setTabCapture", tabId: 5, value: false });
  same(state.tabsOff, [5]);
  same(w.chrome.storage.session.data.tabsOff, [5]);
  assert.ok(w.chrome.action.calls.some(([kind, o]) => kind === "badge" && o.tabId === 5 && o.text === "X"));

  // A download its page starts stays in the browser...
  front(w, 5);
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/a.zip", referrer: "https://site.net/" }));
  same(w.browserCalls, []);
  // ...also when its request was seen, in another tab than the one in front.
  front(w, 6);
  browse(w, { url: "https://cdn.net/b.zip", tabId: 5, headers: { "Content-Type": "application/zip" } });
  same(w.chrome.downloads.ui, [], "the bubble is not even hidden for it");
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/b.zip" }));
  same(w.browserCalls, []);
  // A forced link opens in the browser.
  same(await w.content({ type: "link:force", url: "https://cdn.net/c.zip" }, 5), { ok: false });
  // A web player's file is not taken.
  w.chrome.webRequest.onHeadersReceived.fire({
    requestId: "m",
    url: "https://cdn.net/v.mp4",
    method: "GET",
    type: "media",
    tabId: 5,
    statusCode: 200,
    responseHeaders: h({ "Content-Type": "video/mp4", "Content-Length": "9000000" }),
  });
  await tick();
  same(adds(w), []);

  // Another tab's are taken as usual.
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/d.zip", referrer: "https://site.net/" }));
  same(adds(w).map((m) => m.url), ["https://cdn.net/d.zip"]);
  same((await w.content({ type: "link:force", url: "https://cdn.net/e.zip" }, 6)).ok, true);

  // On again: the tab's own badge goes, its icon comes back.
  state = await w.popup({ type: "setTabCapture", tabId: 5, value: true });
  same(state.tabsOff, []);
  await tick();
  same(w.chrome.action.calls.filter(([, o]) => o.tabId === 5).slice(-2), [
    ["badge", { tabId: 5, text: null }],
    ["icon", { tabId: 5, path: { 16: "icons/16.png", 32: "icons/32.png" } }],
  ]);
  front(w, 5);
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/f.zip", referrer: "https://site.net/" }));
  assert.equal(adds(w).at(-1).url, "https://cdn.net/f.zip");

  // A closed tab is forgotten.
  await w.popup({ type: "setTabCapture", tabId: 5, value: false });
  await w.chrome.tabs.onRemoved.fire(5);
  same(w.chrome.storage.session.data.tabsOff, []);
  same((await w.popup({ type: "state" })).tabsOff, []);
  // Only a tab id is taken.
  w.chrome.runtime.onMessage.fire({ type: "setTabCapture", tabId: "5", value: false }, { id: "udm", url: "chrome-extension://udm/popup.html" }, () => {});
  same((await w.popup({ type: "state" })).tabsOff, []);
});

test("the tabs capture is off in outlast the worker, or live in memory without session storage", async () => {
  const w = connected({}, { session: { tabsOff: [7] } });
  await tick();
  same((await w.popup({ type: "state" })).tabsOff, [7]);

  const old = connected({}, { session: false });
  same((await old.popup({ type: "setTabCapture", tabId: 2, value: false })).tabsOff, [2]);
  front(old, 2);
  await old.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/a.zip", referrer: "https://site.net/" }));
  same(old.browserCalls, []);
});

test("a tab with capture off gets UDM's icon in grey", async () => {
  // A stand-in for the canvas: one red pixel.
  class OffscreenCanvas {
    constructor(w, h) {
      this.size = [w, h];
    }
    getContext() {
      return { drawImage: () => {}, getImageData: (_x, _y, w, h) => ({ width: w, height: h, data: new Uint8ClampedArray([200, 0, 0, 255]) }) };
    }
  }
  const fetched = [];
  const globals = {
    OffscreenCanvas,
    createImageBitmap: async () => ({}),
    fetch: async (url) => (fetched.push(url), { blob: async () => ({}) }),
  };
  const w = connected({}, { globals });
  await w.popup({ type: "setTabCapture", tabId: 4, value: false });
  await tick();
  await tick();
  const icon = w.chrome.action.calls.find(([kind, o]) => kind === "icon" && o.tabId === 4);
  assert.ok(icon, "the icon was set");
  same(Object.keys(icon[1].imageData), ["16", "32"]);
  const [r, g, b, a] = icon[1].imageData[16].data;
  assert.ok(r === g && g === b, "grey");
  assert.equal(a, 255);
  same(fetched, ["chrome-extension://udm/icons/16.png", "chrome-extension://udm/icons/32.png"]);
  // Firefox forgets a tab's icon when it loads a page: it is set again.
  const before = w.chrome.action.calls.length;
  await w.chrome.tabs.onUpdated.fire(4, { status: "loading" }, { id: 4 });
  await tick();
  assert.ok(w.chrome.action.calls.slice(before).some(([kind, o]) => kind === "badge" && o.tabId === 4 && o.text === "X"));
});

test("grey pixels keep their transparency", () => {
  same([...greyPixels(new Uint8ClampedArray([255, 255, 255, 0, 0, 0, 0, 128]))], [243, 243, 243, 0, 90, 90, 90, 128]);
});

test("held keys are reported when they change, and refreshed while held", () => {
  const state = { sent: "", time: 0 };
  assert.equal(heldReport(state, "", 1000), false, "nothing held, nothing said");
  assert.equal(heldReport(state, "force", 1000), true);
  assert.equal(heldReport(state, "force", 1100), false, "a key's repeat");
  assert.equal(heldReport(state, "force", 1000 + HELD_REFRESH_MS), true, "refreshed");
  assert.equal(heldReport(state, "", 1000 + HELD_REFRESH_MS + 5), true, "let go");
  assert.equal(heldReport(state, "", 99999), false);

  assert.equal(heldApplies({ action: "skip", time: 1000 }, 1000 + HELD_EXPIRE_MS), true);
  assert.equal(heldApplies({ action: "skip", time: 1000 }, 1001 + HELD_EXPIRE_MS), false, "a release never heard of");
  assert.equal(heldApplies({ action: "", time: 1000 }, 1000), false);
  assert.equal(heldApplies(undefined, 1000), false);
});

test("a key held down applies to a download its tab starts without a click, until let go", async () => {
  const w = connected();
  front(w, 3);
  const page = (name) => download({ url: `https://site.net/${name}.html`, mime: "text/html", referrer: "https://site.net/" });

  // A form sent with the force key down: UDM takes even a type it does not.
  await w.content({ type: "keys:held", action: "force" }, 3);
  await w.chrome.downloads.onCreated.fire(page("report"));
  same(adds(w).map((m) => m.url), ["https://site.net/report.html"]);
  // Let go: the rules decide again.
  await w.content({ type: "keys:held", action: "" }, 3);
  await w.chrome.downloads.onCreated.fire(page("other"));
  same(adds(w).length, 1);

  // The skip key keeps a type UDM takes in the browser.
  await w.content({ type: "keys:held", action: "skip" }, 3);
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/a.zip", referrer: "https://site.net/" }));
  same(adds(w).length, 1);
  // Only in its own tab.
  front(w, 4);
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/b.zip", referrer: "https://site.net/" }));
  same(adds(w).at(-1).url, "https://cdn.net/b.zip");

  // Switching tabs forgets every held key.
  front(w, 3);
  await w.content({ type: "keys:held", action: "force" }, 3);
  await w.chrome.tabs.onActivated.fire({ tabId: 4 });
  await w.chrome.downloads.onCreated.fire(page("third"));
  same(adds(w).length, 2);
  // So does closing the tab, and an unknown action is no key.
  await w.content({ type: "keys:held", action: "force" }, 3);
  await w.chrome.tabs.onRemoved.fire(3);
  await w.content({ type: "keys:held", action: "open" }, 3);
  await w.chrome.downloads.onCreated.fire(page("fourth"));
  same(adds(w).length, 2);
});

test("Firefox: a key held down decides at the headers", async () => {
  const w = connected({}, { firefox: true });
  await w.content({ type: "keys:held", action: "force" }, 3);
  const page = browse(w, { url: "https://site.net/r.pdf", headers: { "Content-Type": "application/octet-stream", "Content-Disposition": "attachment" } });
  assert.ok(page.some((a) => a && a.cancel), "a type the rules leave out, forced");
  await w.content({ type: "keys:held", action: "skip" }, 3);
  const zip = browse(w, { id: "r2", url: "https://cdn.net/a.zip", headers: { "Content-Type": "application/zip" } });
  assert.ok(!zip.some(Boolean), "a type the rules take, skipped");
});

test("private windows stay out of UDM unless its settings let them in", async () => {
  const w = connected({ capture_types: [] });
  w.chrome.cookies.stores = [
    { id: "0", tabIds: [1] },
    { id: "1", tabIds: [8] },
  ];
  await w.chrome.tabs.onCreated.fire({ id: 8, incognito: true });
  const privateDownload = () => download({ url: "https://cdn.net/p.zip", incognito: true });
  const menu = () =>
    w.chrome.contextMenus.onClicked.fire({ menuItemId: "udm-link", linkUrl: "https://cdn.net/m.zip", pageUrl: "https://site.net/" }, { id: 8, incognito: true });

  // Off, as by default.
  await w.chrome.downloads.onCreated.fire(privateDownload());
  await menu();
  same(browse(w, { url: "https://cdn.net/h.zip", tabId: 8, headers: { "Content-Type": "application/zip" } }), [undefined, undefined]);
  same(await w.content({ type: "video:enabled" }, 8, "https://site.net/", undefined, true), { enabled: false, connected: false, ok: false, items: [] });
  same(adds(w), []);
  same(w.chrome.downloads.ui, [], "not even held for");

  // On: they go, with the private window's cookies.
  w.port.onMessage.fire({ op: "settings", settings: { capture_types: [], capture_private: true } });
  await w.chrome.downloads.onCreated.fire(privateDownload());
  await menu();
  same(adds(w).map((m) => m.url), ["https://cdn.net/p.zip", "https://cdn.net/m.zip"]);
  same(w.chrome.cookies.queries.map((q) => q.storeId), ["1", "1"]);
  assert.equal((await w.content({ type: "video:enabled" }, 8, "https://site.net/", undefined, true)).enabled, true);
  same(await w.content({ type: "link:force", url: "https://cdn.net/f.zip" }, 8, "https://site.net/", undefined, true), { ok: true });
  assert.equal(w.chrome.cookies.queries.at(-1).storeId, "1");
  browse(w, { id: "r2", url: "https://cdn.net/h.zip", tabId: 8, headers: { "Content-Type": "application/zip" } });
  assert.equal(w.chrome.downloads.ui.at(-1), false, "held at its headers too");
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/h.zip", incognito: true }));
  assert.equal(w.chrome.downloads.ui.at(-1), true);

  // A normal window's cookies still come from the normal store.
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/n.zip" }));
  assert.equal(w.chrome.cookies.queries.at(-1).storeId, undefined);
});

test("Firefox: a private download's cookies come from the store it names", async () => {
  const w = connected({ capture_types: [], capture_private: true }, { firefox: true });
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/p.zip", incognito: true, cookieStoreId: "firefox-private" }));
  same(adds(w).map((m) => m.url), ["https://cdn.net/p.zip"]);
  same(w.chrome.cookies.queries.map((q) => q.storeId), ["firefox-private"]);
});
