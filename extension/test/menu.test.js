// Tests for the context menu's page item, a link's `download` attribute and
// title, the notices when a menu item cannot do its job, Firefox hiding
// "selected links", and the popup handing over a download the browser
// already started (#150). Run with: node --test extension/test/*.test.js

const test = require("node:test");
const assert = require("node:assert/strict");

const { loadWorker, download } = require("./harness.js");
const { linkFileName, linkDescription } = require("../rules.js");

function connected(options, settings = {}) {
  const w = loadWorker(options);
  w.port.onMessage.fire({ op: "hello", name: "pullbay-daemon", version: "x" });
  w.port.onMessage.fire({ op: "settings", settings: { capture_types: [], capture_min_size_kb: 0, capture_excluded_sites: [], ...settings } });
  return w;
}

// Values from the worker come from another realm: compare them as JSON.
const same = (a, b) => assert.equal(JSON.stringify(a), JSON.stringify(b));
const lastAdd = (w) => w.sent.filter((m) => m.op === "add").at(-1);
const notices = (w) => w.chrome.notifications.shown.map(([, o]) => o.message);
const click = (w, info, tab = { id: 1, incognito: false }) => w.chrome.contextMenus.onClicked.fire(info, tab);

test("the page item sends the page's address, or the frame's", async () => {
  const w = connected();
  same(w.chrome.contextMenus.items.get("pullbay-page").contexts, ["page", "frame"]);
  assert.equal(w.chrome.contextMenus.items.get("pullbay-page").title, "Download page with Pullbay");

  await click(w, { menuItemId: "pullbay-page", pageUrl: "https://site.net/article" });
  const add = lastAdd(w);
  assert.deepEqual([add.url, add.referrer, add.page], ["https://site.net/article", "https://site.net/article", "https://site.net/article"]);

  await click(w, { menuItemId: "pullbay-page", pageUrl: "https://site.net/article", frameUrl: "https://embed.net/player" });
  assert.deepEqual([lastAdd(w).url, lastAdd(w).referrer, lastAdd(w).page], ["https://embed.net/player", "https://site.net/article", "https://site.net/article"]);

  // Not a web page (the browser's own pages): nothing.
  const before = w.sent.length;
  await click(w, { menuItemId: "pullbay-page", pageUrl: "chrome://newtab/" });
  assert.equal(w.sent.length, before);
});

test("the page item can be left out of the menu like the others", () => {
  const w = connected({}, { browser_menu_hidden_chromium: ["page"] });
  assert.equal(w.chrome.contextMenus.items.has("pullbay-page"), false);
  assert.ok(w.chrome.contextMenus.items.has("pullbay-link"));
});

test("a link's download attribute and title go with it as name and description", async () => {
  const w = connected();
  w.chrome.tabs.reply = (msg) =>
    msg.url === "https://cdn.net/get?id=7" ? { download: "../files/Report: 2024?.pdf", title: "  Annual\n report ", text: "Get it" } : null;
  await click(w, { menuItemId: "pullbay-link", linkUrl: "https://cdn.net/get?id=7", pageUrl: "https://site.net/", frameId: 3 });
  const [tabId, msg, options] = w.chrome.tabs.messages.at(-1);
  assert.deepEqual([tabId, msg.type, msg.url, options.frameId], [1, "link:details", "https://cdn.net/get?id=7", 3]);
  assert.deepEqual([lastAdd(w).filename, lastAdd(w).description], ["Report_ 2024_.pdf", "Annual report"]);

  // No title: its text describes it. A bare `download` names nothing.
  w.chrome.tabs.reply = () => ({ download: "", title: "", text: " Setup  for Windows " });
  await click(w, { menuItemId: "pullbay-later-link", linkUrl: "https://cdn.net/setup.exe", pageUrl: "https://site.net/" });
  assert.deepEqual([lastAdd(w).filename, lastAdd(w).description, lastAdd(w).start], [undefined, "Setup for Windows", false]);

  // The frame cannot say (not its link, or no content script): neither.
  w.chrome.tabs.reply = () => {
    throw new Error("Could not establish connection");
  };
  await click(w, { menuItemId: "pullbay-link", linkUrl: "https://cdn.net/b.zip", pageUrl: "https://site.net/" });
  assert.deepEqual([lastAdd(w).url, lastAdd(w).filename, lastAdd(w).description], ["https://cdn.net/b.zip", undefined, undefined]);
});

test("a link's file name and description are made safe", () => {
  assert.equal(linkFileName("a/b\\c<d>.zip"), "c_d_.zip");
  assert.equal(linkFileName(" ..hidden. "), "hidden");
  assert.equal(linkFileName(""), undefined);
  assert.equal(linkFileName(null), undefined);
  assert.equal(linkFileName("x".repeat(500)).length, 200);
  assert.equal(linkDescription("", "  "), undefined);
  assert.equal(linkDescription(" A\ttitle ", "text"), "A title");
  assert.equal(linkDescription(null, "y".repeat(400)).length, 300);
});

test("download all or selected links says when there are none", async () => {
  const w = connected();
  w.chrome.scripting = { executeScript: async () => [{ result: [] }, { result: [{ url: "javascript:void(0)", text: "x" }] }] };
  await click(w, { menuItemId: "pullbay-all-links", pageUrl: "https://site.net/" });
  await click(w, { menuItemId: "pullbay-selection-links", pageUrl: "https://site.net/" });
  assert.deepEqual(notices(w), ["No links found on this page.", "No links found in the selection."]);
  assert.equal(w.sent.some((m) => m.op === "pick_links"), false);

  // A page extensions may not read says so too.
  w.chrome.scripting = { executeScript: async () => Promise.reject(new Error("Cannot access a chrome:// URL")) };
  await click(w, { menuItemId: "pullbay-all-links", pageUrl: "https://chromewebstore.google.com/" });
  assert.equal(notices(w).at(-1), "The browser does not let Pullbay read the links of this page.");
});

test("a menu item says when Pullbay cannot be reached", async () => {
  const w = loadWorker();
  await click(w, { menuItemId: "pullbay-page", pageUrl: "https://site.net/" });
  await click(w, { menuItemId: "pullbay-all-links", pageUrl: "https://site.net/" });
  assert.deepEqual(notices(w), ["Cannot reach Pullbay. Is it running?", "Cannot reach Pullbay. Is it running?"]);
  assert.equal(w.sent.some((m) => m.op === "add" || m.op === "pick_links"), false);

  // A link still downloads, in the browser, and the notice says so.
  await click(w, { menuItemId: "pullbay-link", linkUrl: "https://cdn.net/a.zip", pageUrl: "https://site.net/" });
  assert.equal(notices(w).at(-1), "Cannot reach Pullbay. Is it running? The browser downloads it instead.");
  assert.deepEqual(w.browserCalls.at(-1).slice(0, 2), ["download", "https://cdn.net/a.zip"]);
});

test("the cannot-reach notice gives the browser's reason when the host went away", async () => {
  const w = connected();
  w.chrome.runtime.lastError = { message: "Native host has exited." };
  await w.port.onDisconnect.fire();
  w.chrome.runtime.lastError = null;
  await click(w, { menuItemId: "pullbay-page", pageUrl: "https://site.net/" });
  assert.deepEqual(notices(w), ["Cannot reach Pullbay (Native host has exited.)."]);
  // The worker reconnects on its own; answer it so nothing is left waiting.
  await new Promise((r) => setTimeout(r, 1100));
  w.port.onMessage.fire({ op: "hello" });
});

test("Firefox hides selected links when the selection has none", async () => {
  const w = connected({ firefox: true });
  const item = () => w.chrome.contextMenus.items.get("pullbay-selection-links");
  const shown = (info) => w.chrome.contextMenus.onShown.fire({ menuIds: ["pullbay-selection-links"], contexts: ["selection"], frameId: 2, ...info }, { id: 5 });

  w.chrome.tabs.reply = (msg) => (msg.type === "selection:hasLinks" ? { links: false } : null);
  await shown({ selectionText: "just words" });
  assert.equal(item().visible, false);
  assert.equal(w.chrome.contextMenus.refreshed, 1);
  same(w.chrome.tabs.messages.at(-1), [5, { type: "selection:hasLinks" }, { frameId: 2 }]);

  // Firefox leaves a hidden item out of menuIds; it still comes back.
  w.chrome.tabs.reply = () => ({ links: true });
  await shown({ selectionText: "a link", menuIds: [] });
  assert.equal(item().visible, true);

  // A bare address in the selection needs no question.
  w.chrome.tabs.reply = () => ({ links: false });
  const asked = w.chrome.tabs.messages.length;
  await shown({ selectionText: "see https://x.net/a.zip" });
  assert.equal(item().visible, true);
  assert.equal(w.chrome.tabs.messages.length, asked);

  // A menu without the item is left alone.
  await w.chrome.contextMenus.onShown.fire({ menuIds: ["pullbay-link"], contexts: ["link"] }, { id: 5 });
  assert.equal(w.chrome.tabs.messages.length, asked);
});

test("Chrome, without menus.onShown, leaves selected links as it is", () => {
  const w = connected();
  assert.equal(w.chrome.contextMenus.onShown, undefined);
  assert.equal(w.chrome.contextMenus.items.get("pullbay-selection-links").visible, undefined);
});

test("the popup lists the browser's downloads in progress and hands one to Pullbay", async () => {
  const w = connected();
  w.chrome.cookies.getAll = async ({ url }) => (url.startsWith("https://cdn.net/") ? [{ name: "s", value: "1" }] : []);
  // The tab in front is not where a download that started earlier came from.
  w.chrome.tabs.query = async () => [{ id: 9, url: "https://other.net/", incognito: false }];
  const big = download({ url: "https://cdn.net/big.iso", referrer: "https://cdn.net/files", filename: "/home/me/Downloads/big.iso", state: "in_progress", bytesReceived: 100, totalBytes: 1000 });
  w.chrome.downloads.items.push(
    big,
    download({ url: "https://cdn.net/done.zip", state: "complete" }),
    download({ url: "https://cdn.net/private.zip", state: "in_progress", incognito: true }),
    download({ url: "blob:https://cdn.net/1", state: "in_progress" }),
  );
  const { items } = await w.popup({ type: "browserDownloads" });
  same(items, [{ id: big.id, url: "https://cdn.net/big.iso", filename: "big.iso", done: 100, total: 1000 }]);

  const { ok } = await w.popup({ type: "sendBrowserDownload", id: big.id });
  assert.equal(ok, true);
  assert.deepEqual(w.browserCalls, [["cancel", big.id], ["erase", big.id]]);
  const add = lastAdd(w);
  assert.deepEqual([add.url, add.referrer, add.filename, add.cookies, add.page], ["https://cdn.net/big.iso", "https://cdn.net/files", "big.iso", "s=1", undefined]);

  // Pullbay refusing it gives it back to the browser, as a captured download.
  w.port.onMessage.fire({ op: "rejected", url: "https://cdn.net/big.iso", message: "no" });
  assert.deepEqual(w.browserCalls.at(-1).slice(0, 2), ["download", "https://cdn.net/big.iso"]);

  // A private window's, or one that is not in progress, is not taken.
  for (const d of w.chrome.downloads.items.slice(1, 3)) assert.equal((await w.popup({ type: "sendBrowserDownload", id: d.id })).ok, false);
});

test("the popup cannot hand over a download while Pullbay is away", async () => {
  const w = loadWorker();
  const d = download({ url: "https://cdn.net/a.zip", state: "in_progress" });
  w.chrome.downloads.items.push(d);
  assert.equal((await w.popup({ type: "sendBrowserDownload", id: d.id })).ok, false);
  assert.deepEqual(w.browserCalls, [], "the browser keeps it");
  assert.deepEqual(notices(w), ["Cannot reach Pullbay. Is it running?"]);
});
