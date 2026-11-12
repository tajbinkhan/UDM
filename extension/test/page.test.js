// Tests for the web page a download came from, which the service worker
// sends apart from its referrer (as IDM keeps both). Run with:
// node --test extension/test/*.test.js

const test = require("node:test");
const assert = require("node:assert/strict");

const { loadWorker, download } = require("./harness.js");

const same = (a, b) => assert.equal(JSON.stringify(a), JSON.stringify(b));

function connected() {
  const w = loadWorker();
  w.port.onMessage.fire({ op: "hello", name: "udm-daemon", version: "x" });
  w.port.onMessage.fire({ op: "settings", settings: { capture_types: [], capture_min_size_kb: 0, capture_excluded_sites: [] } });
  return w;
}

const lastAdd = (w) => w.sent.filter((m) => m.op === "add").at(-1);

test("a captured download sends the tab in front as its page", async () => {
  const w = connected();
  w.chrome.tabs.query = async () => [{ id: 3, url: "https://news.net/article", incognito: false }];
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/a.zip", referrer: "https://ads.net/frame" }));
  const add = lastAdd(w);
  same([add.referrer, add.page], ["https://ads.net/frame", "https://news.net/article"]);

  // No referrer (an address typed in): no page either.
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/b.zip" }));
  same([lastAdd(w).referrer, lastAdd(w).page], [undefined, undefined]);

  // A tab showing the download itself, or no web page, is not its page.
  w.chrome.tabs.query = async () => [{ id: 4, url: "https://cdn.net/c.zip", incognito: false }];
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/c.zip", referrer: "https://news.net/" }));
  same([lastAdd(w).referrer, lastAdd(w).page], ["https://news.net/", undefined]);
  w.chrome.tabs.query = async () => [{ id: 5, url: "chrome://newtab/", incognito: false }];
  await w.chrome.downloads.onCreated.fire(download({ url: "https://cdn.net/d.zip", referrer: "https://news.net/" }));
  assert.equal(lastAdd(w).page, undefined);
});

test("a forced link sends its frame as referrer and the tab as page", async () => {
  const w = connected();
  const ok = await w.content({ type: "link:force", url: "https://cdn.net/a.zip" }, 7, "https://site.net/page", "https://embed.net/frame");
  assert.equal(ok.ok, true);
  same([lastAdd(w).referrer, lastAdd(w).page], ["https://embed.net/frame", "https://site.net/page"]);
});

test("a link from the context menu sends its frame and its page", async () => {
  const w = connected();
  await w.chrome.contextMenus.onClicked.fire(
    { menuItemId: "udm-link", linkUrl: "https://cdn.net/a.zip", pageUrl: "https://site.net/page", frameUrl: "https://embed.net/frame" },
    { id: 1, incognito: false },
  );
  same([lastAdd(w).referrer, lastAdd(w).page], ["https://embed.net/frame", "https://site.net/page"]);
  // A link in the page itself: both are the page.
  await w.chrome.contextMenus.onClicked.fire(
    { menuItemId: "udm-link", linkUrl: "https://cdn.net/b.zip", pageUrl: "https://site.net/page" },
    { id: 1, incognito: false },
  );
  same([lastAdd(w).referrer, lastAdd(w).page], ["https://site.net/page", "https://site.net/page"]);
});
