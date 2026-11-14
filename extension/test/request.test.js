// Tests for the whole request behind a browser download (request.js), and
// for the service worker sending it: the browser's headers, redirects and
// response, the download caught at its headers, the bubble kept hidden,
// the tab opened only for a download closed, the referrer past about:blank
// frames, and the browser's whole proxy setting. Run with:
// node --test extension/test/*.test.js

const test = require("node:test");
const assert = require("node:assert/strict");

const {
  downloadResponse,
  dispositionName,
  noteRequest,
  noteRedirect,
  noteResponse,
  findRequest,
  requestDetails,
  requestReferer,
  webReferrer,
  frameReferrer,
  openedOnlyFor,
  proxySetting,
  bubbleGate,
  REQUEST_MATCH_MS,
} = require("../chromium/request.js");
const { loadWorker, download, setBrowserProxy } = require("./harness.js");

const same = (a, b) => assert.equal(JSON.stringify(a), JSON.stringify(b));
const h = (pairs) => Object.entries(pairs).map(([name, value]) => ({ name, value }));
const tick = () => new Promise((r) => setTimeout(r, 0));

test("a response is a download when marked so or of a type the browser cannot show", () => {
  const at = (headers, statusCode = 200, url = "https://x.net/get") => downloadResponse({ url, statusCode, responseHeaders: h(headers) });
  same(at({ "Content-Type": "application/zip", "Content-Length": "2048" }), { filename: undefined, mime: "application/zip", size: 2048, attachment: false });
  same(at({ "Content-Type": "text/html", "Content-Disposition": 'attachment; filename="r.html"' }), {
    filename: "r.html",
    mime: "text/html",
    size: 0,
    attachment: true,
  });
  assert.equal(at({ "Content-Type": "text/html; charset=utf-8" }), null, "a page");
  assert.equal(at({ "Content-Type": "application/pdf" }), null, "the browser shows it");
  assert.equal(at({ "Content-Type": "image/svg+xml" }), null);
  assert.equal(at({ "Content-Type": "application/zip" }, 404), null, "an error");
  assert.equal(at({ "Content-Type": "application/zip" }, 302), null, "a redirect");
  assert.equal(at({}), null, "no type: the browser sniffs it");
  assert.equal(at({ "Content-Type": "application/zip" }, 200, "ftp://x/a"), null);

  assert.equal(dispositionName("attachment; filename*=UTF-8''%C3%A9t%C3%A9.zip; filename=\"ete.zip\""), "été.zip");
  assert.equal(dispositionName('attachment; filename="a \\"b\\".zip"'), 'a "b".zip');
  assert.equal(dispositionName("attachment; filename=../../evil.exe"), "evil.exe");
  assert.equal(dispositionName("inline"), undefined);
});

test("a request is followed through its redirects to its response", () => {
  const log = new Map();
  const now = 1_000_000;
  noteRequest(
    log,
    {
      requestId: "1",
      url: "https://x.net/get/7",
      method: "GET",
      type: "main_frame",
      tabId: 4,
      frameId: 0,
      parentFrameId: -1,
      requestHeaders: h({ Accept: "*/*", Cookie: "s=1", Referer: "https://x.net/list", "Accept-Language": "de" }),
    },
    now,
  );
  noteRedirect(log, { requestId: "1", url: "https://x.net/get/7", redirectUrl: "https://cdn.net/a.zip?sig=1" });
  const record = noteResponse(
    log,
    { requestId: "1", url: "https://cdn.net/a.zip?sig=1", statusCode: 200, responseHeaders: h({ "Content-Type": "application/zip", "Content-Length": "99" }) },
    now,
  );
  assert.equal(record.response.size, 99);
  assert.equal(findRequest(log, "https://cdn.net/a.zip?sig=1", now), record);
  assert.equal(findRequest(log, "https://x.net/get/7", now), record, "by an address it went through");
  assert.equal(findRequest(log, "https://cdn.net/a.zip?sig=1", now + REQUEST_MATCH_MS + 1), null, "too old");
  same(requestDetails(record), {
    request_headers: [
      ["Accept", "*/*"],
      ["Referer", "https://x.net/list"],
      ["Accept-Language", "de"],
    ],
    redirects: ["https://x.net/get/7"],
    size: 99,
    content_type: "application/zip",
  });
  assert.equal(requestReferer(record), "https://x.net/list");
  same(requestDetails(null), {});
});

test("a referrer is never a frame made from nothing", () => {
  assert.equal(webReferrer("about:blank", undefined, "https://site.net/page"), "https://site.net/page");
  assert.equal(webReferrer("about:srcdoc", "https://embed.net/f"), "https://embed.net/f");
  assert.equal(webReferrer("https://a.net/", "https://b.net/"), "https://a.net/");
  assert.equal(webReferrer("about:blank", ""), undefined);
  const frames = [
    { frameId: 0, parentFrameId: -1, url: "https://site.net/page" },
    { frameId: 3, parentFrameId: 0, url: "https://embed.net/f" },
    { frameId: 5, parentFrameId: 3, url: "about:blank" },
    { frameId: 6, parentFrameId: 5, url: "about:srcdoc" },
  ];
  assert.equal(frameReferrer(frames, 6), "https://embed.net/f");
  assert.equal(frameReferrer(frames, 0), "https://site.net/page");
  assert.equal(frameReferrer(frames, 42), undefined);
  assert.equal(frameReferrer(null, 0), undefined);
});

test("a tab is closed only when it was opened for the download alone", () => {
  const now = 50_000;
  const opened = { time: now - 1000, opener: 1, loaded: false };
  const urls = ["https://cdn.net/a.zip"];
  assert.ok(openedOnlyFor({ id: 9, url: "about:blank" }, opened, urls, now));
  assert.ok(openedOnlyFor({ id: 9, url: "", pendingUrl: "https://cdn.net/a.zip" }, opened, urls, now));
  assert.ok(!openedOnlyFor({ id: 9, url: "https://site.net/" }, opened, urls, now), "it shows a page");
  assert.ok(!openedOnlyFor({ id: 9, url: "about:blank" }, { ...opened, loaded: true }, urls, now), "it has history");
  assert.ok(!openedOnlyFor({ id: 9, url: "about:blank" }, { ...opened, opener: undefined }, urls, now), "the user opened it");
  assert.ok(!openedOnlyFor({ id: 9, url: "about:blank" }, { ...opened, time: 0 }, urls, now), "long ago");
  assert.ok(!openedOnlyFor({ id: 9, url: "about:blank" }, undefined, urls, now));
});

test("the browser's proxy setting is read in full", () => {
  // Chrome.
  same(proxySetting({ mode: "direct" }), { mode: "direct" });
  same(proxySetting({ mode: "system" }), { mode: "system" });
  same(proxySetting({ mode: "auto_detect" }), { mode: "auto_detect" });
  same(proxySetting({ mode: "pac_script", pacScript: { url: "http://wpad/p.pac" } }), { mode: "pac", pac_url: "http://wpad/p.pac" });
  same(proxySetting({ mode: "pac_script", pacScript: { data: "function FindProxyForURL(){}" } }), {
    mode: "pac",
    pac_script: "function FindProxyForURL(){}",
  });
  same(
    proxySetting({
      mode: "fixed_servers",
      rules: { proxyForHttp: { host: "web", port: 8080 }, proxyForHttps: { scheme: "socks5", host: "s" }, bypassList: ["<local>", "*.corp"] },
    }),
    { mode: "fixed", http: "http://web:8080", https: "socks5://s:1080", bypass: ["<local>", "*.corp"] },
  );
  same(proxySetting({ mode: "fixed_servers", rules: { singleProxy: { scheme: "socks4", host: "::1", port: 9 } } }), {
    mode: "fixed",
    http: "socks4://[::1]:9",
    https: "socks4://[::1]:9",
    socks: "socks4://[::1]:9",
  });
  // Firefox.
  same(proxySetting({ proxyType: "none" }), { mode: "direct" });
  same(proxySetting({ proxyType: "system" }), { mode: "system" });
  same(proxySetting({ proxyType: "autoDetect" }), { mode: "auto_detect" });
  same(proxySetting({ proxyType: "autoConfig", autoConfigUrl: "http://wpad/p.pac" }), { mode: "pac", pac_url: "http://wpad/p.pac" });
  same(
    proxySetting({ proxyType: "manual", http: "web:3128", ssl: "tls.corp", socks: "s:9050", socksVersion: 5, passthrough: "localhost, .corp.net" }),
    { mode: "fixed", http: "http://web:3128", https: "http://tls.corp:80", socks: "socks5://s:9050", bypass: ["localhost", ".corp.net"] },
  );
  same(proxySetting({ proxyType: "manual", http: "web:3128", httpProxyAll: true }), { mode: "fixed", http: "http://web:3128", https: "http://web:3128" });
  assert.equal(proxySetting(undefined), undefined);
  assert.equal(proxySetting({ mode: "unknown" }), undefined);
});

test("the bubble comes back when every download held for is decided, or after a while", async () => {
  const calls = [];
  const gate = bubbleGate((on) => calls.push(on), 20);
  gate.hold();
  gate.hold();
  gate.release();
  same(calls, [false]);
  gate.release();
  same(calls, [false, true]);
  gate.release();
  same(calls, [false, true], "nothing held");
  gate.hold();
  await new Promise((r) => setTimeout(r, 40));
  same(calls, [false, true, false, true], "never left hidden");
  assert.equal(gate.held, false);
});

// --- The service worker ---------------------------------------------------------

function connected(options) {
  const w = loadWorker(options);
  w.port.onMessage.fire({ op: "hello", name: "udm-daemon", version: "x" });
  w.port.onMessage.fire({ op: "settings", settings: { capture_types: ["zip"], capture_min_size_kb: 0, capture_excluded_sites: [] } });
  return w;
}

// The browser asking for `url` in tab `tabId`, redirected from `from`, and
// answered with `headers`.
function browse(w, { id = "r1", url, from, tabId = 3, type = "main_frame", headers, sent = {} }) {
  const wr = w.chrome.webRequest;
  wr.onSendHeaders.fire({ requestId: id, url: from || url, method: "GET", type, tabId, frameId: 0, parentFrameId: -1, requestHeaders: h(sent) });
  if (from) wr.onBeforeRedirect.fire({ requestId: id, url: from, redirectUrl: url, type, tabId });
  return wr.onHeadersReceived.call({ requestId: id, url, method: "GET", type, tabId, frameId: 0, parentFrameId: -1, statusCode: 200, responseHeaders: h(headers) });
}

const lastAdd = (w) => w.sent.filter((m) => m.op === "add").at(-1);

test("Chrome: the whole request goes to UDM and the bubble stays hidden", async () => {
  const w = connected();
  browse(w, {
    url: "https://cdn.net/a.zip",
    from: "https://site.net/get/a",
    headers: { "Content-Type": "application/octet-stream", "Content-Length": "5000", "Content-Disposition": 'attachment; filename="a.zip"' },
    sent: { Accept: "*/*", Cookie: "s=1", Referer: "https://site.net/list", "Accept-Language": "fr" },
  });
  same(w.chrome.downloads.ui, [false], "hidden as soon as the headers say it is taken");
  // Chrome cannot cancel at the headers: the download still appears.
  assert.equal(w.browserCalls.length, 0);
  await w.chrome.downloads.onCreated.fire(download({ url: "https://site.net/get/a", finalUrl: "https://cdn.net/a.zip", referrer: "about:blank" }));
  const add = lastAdd(w);
  same(
    [add.url, add.referrer, add.redirects, add.size, add.content_type, add.filename],
    ["https://cdn.net/a.zip", "https://site.net/list", ["https://site.net/get/a"], 5000, "application/octet-stream", "a.zip"],
  );
  same(add.request_headers, [
    ["Accept", "*/*"],
    ["Referer", "https://site.net/list"],
    ["Accept-Language", "fr"],
  ]);
  same(w.browserCalls.map((c) => c[0]), ["cancel", "erase"]);
  assert.equal(w.chrome.downloads.ui.at(-1), true, "shown again once it is taken");
});

test("Chrome: a download left in the browser shows its bubble again", async () => {
  const w = connected();
  browse(w, { url: "https://cdn.net/a.zip", headers: { "Content-Type": "application/zip" } });
  same(w.chrome.downloads.ui, [false]);
  await tick(); // the stored settings are read
  w.chrome.storage.onChanged.fire({ capture: { newValue: false } });
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/a.zip" }));
  same(w.chrome.downloads.ui, [false, true]);
  assert.equal(w.browserCalls.length, 0);
  // A type the rules leave out is not even held for.
  w.chrome.storage.onChanged.fire({ capture: { newValue: true } });
  browse(w, { id: "r2", url: "https://cdn.net/a.exe", headers: { "Content-Type": "application/x-msdownload" } });
  same(w.chrome.downloads.ui, [false, true]);
});

test("a tab opened only for a download UDM took is closed", async () => {
  const w = connected();
  await w.chrome.tabs.onCreated.fire({ id: 9, openerTabId: 1, url: "", pendingUrl: "https://cdn.net/a.zip" });
  w.chrome.tabs.known.set(9, { id: 9, url: "", pendingUrl: "https://cdn.net/a.zip" });
  browse(w, { url: "https://cdn.net/a.zip", tabId: 9, headers: { "Content-Type": "application/zip" } });
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/a.zip" }));
  await tick();
  same(w.chrome.tabs.removed, [9]);

  // A tab that showed a page first keeps it.
  await w.chrome.tabs.onCreated.fire({ id: 10, openerTabId: 1 });
  await w.chrome.tabs.onUpdated.fire(10, { status: "complete" }, { id: 10, url: "https://site.net/files" });
  w.chrome.tabs.known.set(10, { id: 10, url: "about:blank" });
  browse(w, { id: "r2", url: "https://cdn.net/b.zip", tabId: 10, headers: { "Content-Type": "application/zip" } });
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/b.zip" }));
  await tick();
  same(w.chrome.tabs.removed, [9]);
});

test("a link in an about:blank frame has the frame above it as referrer", async () => {
  const w = connected();
  w.chrome.webNavigation.frames.set(1, [
    { frameId: 0, parentFrameId: -1, url: "https://site.net/page" },
    { frameId: 3, parentFrameId: 0, url: "https://embed.net/f" },
    { frameId: 5, parentFrameId: 3, url: "about:blank" },
  ]);
  await w.chrome.contextMenus.onClicked.fire(
    { menuItemId: "udm-link", linkUrl: "https://cdn.net/a.zip", pageUrl: "https://site.net/page", frameUrl: "about:blank", frameId: 5 },
    { id: 1, incognito: false },
  );
  same([lastAdd(w).referrer, lastAdd(w).page], ["https://embed.net/f", "https://site.net/page"]);
  // Without the frames, the page.
  await w.chrome.contextMenus.onClicked.fire(
    { menuItemId: "udm-link", linkUrl: "https://cdn.net/b.zip", pageUrl: "https://site.net/page", frameUrl: "about:srcdoc", frameId: 8 },
    { id: 2, incognito: false },
  );
  assert.equal(lastAdd(w).referrer, "https://site.net/page");
});

test("Firefox: a download is taken at its headers and never reaches the browser", async () => {
  const w = connected({ firefox: true });
  // Firefox lets the worker hold the response, and has no extraHeaders.
  assert.ok(w.chrome.webRequest.onHeadersReceived.options.some((o) => o.includes("blocking")));
  assert.ok(!w.chrome.webRequest.onHeadersReceived.options.some((o) => o.includes("extraHeaders")));
  w.chrome.tabs.known.set(3, { id: 3, url: "https://site.net/list" });
  const answers = browse(w, {
    url: "https://cdn.net/a.zip",
    headers: { "Content-Type": "application/zip", "Content-Length": "42" },
    sent: { Referer: "https://site.net/list" },
  });
  assert.ok(answers.some((a) => a && a.cancel === true));
  await tick();
  await tick();
  const add = lastAdd(w);
  same([add.url, add.referrer, add.page, add.size], ["https://cdn.net/a.zip", "https://site.net/list", "https://site.net/list", 42]);
  assert.equal(w.chrome.downloads.ui, undefined, "Firefox has no bubble to hide");

  // A page is not held, nor anything while capture is off.
  assert.ok(!browse(w, { id: "p", url: "https://site.net/", headers: { "Content-Type": "text/html" } }).some(Boolean));
  w.chrome.storage.onChanged.fire({ capture: { newValue: false } });
  assert.ok(!browse(w, { id: "q", url: "https://cdn.net/b.zip", headers: { "Content-Type": "application/zip" } }).some(Boolean));
});

test("Chrome asks for the headers it hides, and only Chrome for the bubble", () => {
  const w = loadWorker();
  assert.ok(w.chrome.webRequest.onSendHeaders.options.some((o) => o.includes("extraHeaders")));
  assert.ok(!w.chrome.webRequest.onHeadersReceived.options.some((o) => o.includes("blocking")));
  const { firefoxManifest } = require("./harness.js");
  const chrome = w.chrome.runtime.getManifest();
  assert.ok(chrome.permissions.includes("downloads.ui"));
  assert.ok(!chrome.permissions.includes("webRequestBlocking"));
  const firefox = firefoxManifest();
  assert.ok(firefox.permissions.includes("webRequestBlocking"));
  assert.ok(!firefox.permissions.includes("downloads.ui"), "Firefox does not know it");
  same(
    chrome.permissions.filter((p) => p !== "downloads.ui"),
    firefox.permissions.filter((p) => p !== "webRequestBlocking"),
  );
});

test("the browser's whole proxy setting goes along with a download", async () => {
  const add = async (value) => {
    setBrowserProxy(value);
    const w = connected();
    await w.chrome.downloads.onCreated.fire(download({ url: "https://site.net/a.zip" }));
    return lastAdd(w);
  };
  try {
    const pac = await add({ mode: "pac_script", pacScript: { url: "http://wpad/p.pac" } });
    same([pac.browser_proxy, pac.proxy], [{ mode: "pac", pac_url: "http://wpad/p.pac" }, undefined]);
    const fixed = await add({ mode: "fixed_servers", rules: { singleProxy: { host: "p", port: 1 }, bypassList: ["<local>"] } });
    same([fixed.browser_proxy.bypass, fixed.proxy], [["<local>"], "http://p:1"]);
    same((await add({ mode: "direct" })).browser_proxy, { mode: "direct" });
  } finally {
    setBrowserProxy({ mode: "system" });
  }
});
